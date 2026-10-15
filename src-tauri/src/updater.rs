//! Software updates: find a newer signed release on GitHub, download it, and install it
//! only when nothing is running.
//!
//! The work is done by the official Tauri updater plugin, driven from here rather than
//! from the window, so that the rules below are enforced by the host and cannot be
//! bypassed by the user interface:
//!
//! - **Stable channel only.** The manifest is GitHub's `releases/latest/download/latest.json`,
//!   which resolves to the newest *published, non-prerelease* release; drafts and
//!   prereleases are never served there. On top of that, a manifest announcing a
//!   prerelease version (`1.2.3-rc.1`) or anything not newer than the running version is
//!   ignored ([`is_stable_update`]).
//! - **Signed updates only.** The plugin verifies every download against the public key
//!   compiled into the application (minisign, `plugins.updater.pubkey`) and, with
//!   `requireSignedVersion`, checks that the signature was made for the version the
//!   manifest announces, so an old release cannot be replayed as a new one. A package that
//!   fails is discarded; nothing unverified is ever executed.
//! - **The same installer type.** A copy installed by the NSIS installer updates from the
//!   NSIS entry of the manifest, an MSI install from the MSI entry, by the bundle marker the
//!   bundler writes into the executable. A portable copy is told about a new version and
//!   pointed at the download page; it never runs an installer.
//! - **Never during work.** An install exits the application (Windows installers cannot
//!   replace a running program), so it is refused while any job is queued or running or any
//!   file is being written ([`Busy`]). Downloading may happen while jobs run.
//! - **Respect the user's choice.** `Off` makes no request unless the user presses
//!   "Check for updates". `Notify` checks at most once a day and asks before downloading.
//!   `Automatic` also downloads in the background and installs when the user closes the
//!   application, or immediately if they choose "Restart to update".
//!
//! Nothing about the user's mailboxes is ever part of a request: the plugin sends a GET for
//! a fixed URL with its own user agent, and a GET for the installer the manifest names.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Error as UpdaterError, Update, Updater, UpdaterExt};

/// The public repository; release pages and the manifest live under it.
pub const REPOSITORY_URL: &str = "https://github.com/wildbondinteractive/OST-PST-Studio";
/// Event carrying an [`UpdateStatus`] whenever anything about updates changes.
pub const STATUS_EVENT: &str = "update://status";
/// Background checks are at most this frequent after a successful check.
pub const CHECK_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
/// ...and after a failed one, so a laptop that started offline still finds out the same day.
pub const RETRY_INTERVAL_MS: i64 = 6 * 60 * 60 * 1000;
/// "Remind me later" hides the notice for this long.
pub const REMIND_LATER_MS: i64 = 24 * 60 * 60 * 1000;
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
const SCHEDULER_TICK: Duration = Duration::from_secs(60 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// The user's update preference, stored in `settings.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UpdateMode {
    /// Check daily, download in the background, install on close or on request.
    Automatic,
    /// Check daily and say so; download and install only when asked.
    #[default]
    Notify,
    /// No background requests at all; the manual check still works.
    Off,
}

/// How this copy of the application was installed, read from the bundle marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallKind {
    Nsis,
    Msi,
    /// The bare executable: the portable download, a development build, or a copy run
    /// from anywhere other than where an installer put it.
    Portable,
}

impl InstallKind {
    pub fn current() -> InstallKind {
        match tauri::utils::platform::bundle_type() {
            Some(BundleType::Nsis) => InstallKind::Nsis,
            Some(BundleType::Msi) => InstallKind::Msi,
            _ => InstallKind::Portable,
        }
    }

    /// The `platforms` entry of `latest.json` this copy reads.
    pub fn manifest_target(self) -> &'static str {
        match self {
            InstallKind::Nsis => "windows-x86_64-nsis",
            InstallKind::Msi => "windows-x86_64-msi",
            // A portable copy reads the installer entry only to learn the version; it is
            // never downloaded or run.
            InstallKind::Portable => "windows-x86_64-nsis",
        }
    }

    pub fn installs_updates(self) -> bool {
        !matches!(self, InstallKind::Portable)
    }
}

/// What the window is told about an available release.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub date_ms: Option<i64>,
    pub notes: Option<String>,
    pub release_url: String,
}

/// Why an update step failed, so the window can say something useful.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FailureKind {
    /// GitHub could not be reached, or the connection broke.
    Network,
    /// GitHub answered, but there is no manifest for the latest release.
    Unavailable,
    /// The manifest is malformed, or has no entry for this installer type.
    Metadata,
    /// The downloaded package failed signature verification and was discarded.
    Signature,
    /// The installer could not be started.
    Install,
    /// Work is in progress; installing would interrupt it.
    Busy,
    /// This copy cannot install updates (portable) or the action does not apply now.
    Unsupported,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available {
        info: UpdateInfo,
    },
    Downloading {
        info: UpdateInfo,
        received: u64,
        total: Option<u64>,
    },
    Verifying {
        info: UpdateInfo,
    },
    /// Downloaded and verified; installing needs the application to close.
    Ready {
        info: UpdateInfo,
    },
    Installing {
        info: UpdateInfo,
    },
    Failed {
        kind: FailureKind,
        message: String,
        detail: Option<String>,
        info: Option<UpdateInfo>,
    },
}

impl Phase {
    pub fn info(&self) -> Option<&UpdateInfo> {
        match self {
            Phase::Available { info }
            | Phase::Downloading { info, .. }
            | Phase::Verifying { info }
            | Phase::Ready { info }
            | Phase::Installing { info } => Some(info),
            Phase::Failed { info, .. } => info.as_ref(),
            _ => None,
        }
    }

    fn working(&self) -> bool {
        matches!(
            self,
            Phase::Checking
                | Phase::Downloading { .. }
                | Phase::Verifying { .. }
                | Phase::Installing { .. }
        )
    }
}

/// Work that an install would interrupt.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Busy {
    /// Jobs queued or running: conversions, repairs, exports, verifications, batches.
    pub jobs: usize,
    /// Other operations writing files or reading a mailbox right now (opening a file,
    /// saving or extracting attachments, saving a report).
    pub operations: usize,
}

impl Busy {
    pub fn any(&self) -> bool {
        self.jobs > 0 || self.operations > 0
    }

    pub fn message(&self) -> String {
        let mut parts = Vec::new();
        if self.jobs > 0 {
            parts.push(plural(self.jobs, "job is", "jobs are"));
        }
        if self.operations > 0 {
            parts.push(plural(
                self.operations,
                "file operation is",
                "file operations are",
            ));
        }
        format!(
            "{} still in progress. The update installs once they have finished, so no output is left half written.",
            parts.join(" and ")
        )
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Everything the window shows about updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub state: Phase,
    pub mode: UpdateMode,
    pub install_kind: InstallKind,
    pub current_version: String,
    pub last_check_ms: Option<i64>,
    pub last_success_ms: Option<i64>,
    /// "Remind me later" is in effect for the available version.
    pub snoozed: bool,
    /// A downloaded update installs when the application closes.
    pub install_on_close: bool,
    pub busy: Busy,
}

/// Bookkeeping kept apart from the user's settings, so the window saving its copy of the
/// settings can never roll it back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    pub last_attempt_ms: Option<i64>,
    pub last_success_ms: Option<i64>,
    pub remind_after_ms: Option<i64>,
    pub remind_version: Option<String>,
}

impl Record {
    pub fn load(path: &Path) -> Record {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    pub fn snoozed(&self, version: &str, now: i64) -> bool {
        self.remind_version.as_deref() == Some(version)
            && self.remind_after_ms.is_some_and(|t| now < t)
    }
}

struct Inner {
    phase: Phase,
    record: Record,
    /// The update found by the last check, with the URL and signature to fetch it.
    pending: Option<Update>,
    /// The verified package, once downloaded.
    package: Option<Vec<u8>>,
}

/// The updater's state, managed by Tauri next to `AppState`.
pub struct UpdaterState {
    inner: Mutex<Inner>,
    record_path: PathBuf,
    kind: InstallKind,
}

impl UpdaterState {
    pub fn new(data_dir: &Path) -> UpdaterState {
        let record_path = data_dir.join("updater.json");
        UpdaterState {
            inner: Mutex::new(Inner {
                phase: Phase::Idle,
                record: Record::load(&record_path),
                pending: None,
                package: None,
            }),
            record_path,
            kind: InstallKind::current(),
        }
    }

    /// Whether no downloaded package is held; used by the tests.
    #[cfg(test)]
    pub(crate) fn has_no_package(&self) -> bool {
        self.inner
            .lock()
            .map(|i| i.package.is_none())
            .unwrap_or(false)
    }

    /// The same, for an installer type other than this build's; used by the tests.
    #[cfg(test)]
    pub(crate) fn with_kind(data_dir: &Path, kind: InstallKind) -> UpdaterState {
        UpdaterState {
            kind,
            ..UpdaterState::new(data_dir)
        }
    }
}

// ---- Pure rules, unit-tested ------------------------------------------------------------

/// Whether a release on the stable channel is an update for the running version: newer, and
/// not a prerelease. A release candidate published by mistake as a full release is still
/// ignored here.
pub fn is_stable_update(current: &semver::Version, candidate: &semver::Version) -> bool {
    candidate.pre.is_empty() && candidate > current
}

/// Whether a background check is due.
pub fn check_due(mode: UpdateMode, record: &Record, now: i64) -> bool {
    if mode == UpdateMode::Off {
        return false;
    }
    let Some(last) = record.last_attempt_ms else {
        return true;
    };
    // A clock set back by days should not stop checks until it catches up again.
    if now < last {
        return true;
    }
    let interval = if record.last_success_ms == Some(last) {
        CHECK_INTERVAL_MS
    } else {
        RETRY_INTERVAL_MS
    };
    now - last >= interval
}

/// Classify a plugin error for the window.
pub fn classify(e: &UpdaterError) -> FailureKind {
    match e {
        UpdaterError::Reqwest(r) if r.is_decode() => FailureKind::Metadata,
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) | UpdaterError::Http(_) => {
            FailureKind::Network
        }
        UpdaterError::ReleaseNotFound => FailureKind::Unavailable,
        UpdaterError::Minisign(_)
        | UpdaterError::Base64(_)
        | UpdaterError::SignatureUtf8(_)
        | UpdaterError::SignedVersionMismatch { .. }
        | UpdaterError::MissingSignedVersion => FailureKind::Signature,
        UpdaterError::Serialization(_)
        | UpdaterError::Semver(_)
        | UpdaterError::UrlParse(_)
        | UpdaterError::TargetNotFound(_)
        | UpdaterError::TargetsNotFound(_)
        | UpdaterError::InvalidUpdaterFormat
        | UpdaterError::InsecureTransportProtocol
        | UpdaterError::EmptyEndpoints => FailureKind::Metadata,
        UpdaterError::Io(_) => FailureKind::Install,
        _ => FailureKind::Other,
    }
}

/// A sentence for the window, with what to do next.
pub fn describe(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Network => "Could not reach GitHub to check for updates. Check the internet connection, or a proxy or firewall, and try again.",
        FailureKind::Unavailable => "GitHub has no update information for the latest release yet. Try again later, or look on the releases page.",
        FailureKind::Metadata => "The update information on GitHub could not be read. Nothing was downloaded. Try again later, or download the release from GitHub.",
        FailureKind::Signature => "The downloaded update did not pass its signature check and was discarded. Nothing was installed. Try again, or download the release from GitHub and check its signature.",
        FailureKind::Install => "The installer could not be started. Download the release from GitHub and run it by hand.",
        FailureKind::Busy => "Work is in progress.",
        FailureKind::Unsupported => "This copy of OST/PST Studio is portable and does not install updates. Download the new version from GitHub.",
        FailureKind::Other => "The update failed. Try again, or download the release from GitHub.",
    }
}

fn failed(kind: FailureKind, detail: Option<String>, info: Option<UpdateInfo>) -> Phase {
    Phase::Failed {
        kind,
        message: describe(kind).to_string(),
        detail,
        info,
    }
}

pub fn release_url(version: &str) -> String {
    format!("{REPOSITORY_URL}/releases/tag/v{version}")
}

pub fn info_of(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        date_ms: update
            .date
            .map(|d| (d.unix_timestamp_nanos() / 1_000_000) as i64),
        notes: update.body.clone().filter(|n| !n.trim().is_empty()),
        release_url: release_url(&update.version),
    }
}

/// An updater for this copy: the stable-channel comparator, this installer type's manifest
/// entry, and a bounded timeout. Public so the integration tests drive exactly this.
pub fn updater_for<R: Runtime>(
    app: &AppHandle<R>,
    kind: InstallKind,
) -> Result<Updater, UpdaterError> {
    app.updater_builder()
        .version_comparator(|current, release| is_stable_update(&current, &release.version))
        .target(kind.manifest_target())
        .timeout(CHECK_TIMEOUT)
        .build()
}

// ---- The state machine ------------------------------------------------------------------

fn now_ms() -> i64 {
    ostpst_core::model::now_ms()
}

fn mode_of<R: Runtime>(app: &AppHandle<R>) -> UpdateMode {
    app.state::<crate::AppState>()
        .settings
        .lock()
        .map(|s| s.update_mode)
        .unwrap_or_default()
}

pub fn status<R: Runtime>(app: &AppHandle<R>) -> UpdateStatus {
    let st = app.state::<UpdaterState>();
    let inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
    let mode = mode_of(app);
    let now = now_ms();
    let snoozed = inner
        .phase
        .info()
        .is_some_and(|i| inner.record.snoozed(&i.version, now));
    UpdateStatus {
        state: inner.phase.clone(),
        mode,
        install_kind: st.kind,
        current_version: app.package_info().version.to_string(),
        last_check_ms: inner.record.last_attempt_ms,
        last_success_ms: inner.record.last_success_ms,
        snoozed,
        install_on_close: mode == UpdateMode::Automatic
            && st.kind.installs_updates()
            && matches!(inner.phase, Phase::Ready { .. }),
        busy: crate::busy(app),
    }
}

pub fn emit_status<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(STATUS_EVENT, status(app));
}

fn set_phase<R: Runtime>(app: &AppHandle<R>, phase: Phase) {
    {
        let st = app.state::<UpdaterState>();
        let mut inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.phase = phase;
    }
    emit_status(app);
}

/// Look for an update. `manual` is a press of "Check for updates", which runs whatever the
/// mode; a background check runs only when [`check_due`] says so, which the scheduler decides.
pub async fn check<R: Runtime>(app: &AppHandle<R>, manual: bool) -> UpdateStatus {
    let previous = {
        let st = app.state::<UpdaterState>();
        let mut inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
        // Never restart a check over a download in progress or a package ready to install.
        if inner.phase.working() || matches!(inner.phase, Phase::Ready { .. }) {
            drop(inner);
            return status(app);
        }
        std::mem::replace(&mut inner.phase, Phase::Checking)
    };
    emit_status(app);

    let kind = app.state::<UpdaterState>().kind;
    let result = match updater_for(app, kind) {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };

    let now = now_ms();
    {
        let st = app.state::<UpdaterState>();
        let mut inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.record.last_attempt_ms = Some(now);
        match result {
            Ok(Some(update)) => {
                inner.record.last_success_ms = Some(now);
                let info = info_of(&update);
                inner.pending = Some(update);
                inner.package = None;
                inner.phase = Phase::Available { info };
            }
            Ok(None) => {
                inner.record.last_success_ms = Some(now);
                inner.pending = None;
                inner.package = None;
                inner.phase = Phase::UpToDate;
            }
            Err(e) => {
                let kind = classify(&e);
                // A failed background check leaves a known update where it was; the window
                // already knows about it, and the next check will try again.
                inner.phase = match previous {
                    Phase::Available { .. } if !manual => previous,
                    _ => failed(kind, Some(e.to_string()), None),
                };
            }
        }
        inner.record.save(&st.record_path);
    }
    emit_status(app);
    status(app)
}

/// Download and verify the available update. The package is held in memory (an installer is
/// a few megabytes) until it is installed or a newer check replaces it.
pub async fn download<R: Runtime>(app: &AppHandle<R>) -> Result<UpdateStatus, String> {
    let st = app.state::<UpdaterState>();
    let (update, info) = {
        let mut inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !st.kind.installs_updates() {
            return Err(describe(FailureKind::Unsupported).into());
        }
        if inner.phase.working() {
            return Err("An update step is already in progress.".into());
        }
        if matches!(inner.phase, Phase::Ready { .. }) {
            drop(inner);
            return Ok(status(app));
        }
        let (Some(update), Some(info)) = (inner.pending.clone(), inner.phase.info().cloned())
        else {
            return Err("There is no update to download. Check for updates first.".into());
        };
        inner.phase = Phase::Downloading {
            info: info.clone(),
            received: 0,
            total: None,
        };
        (update, info)
    };
    emit_status(app);

    let mut received: u64 = 0;
    let mut last_emit = Instant::now();
    let app_chunk = app.clone();
    let app_done = app.clone();
    let info_chunk = info.clone();
    let info_done = info.clone();
    let result = update
        .download(
            move |chunk, total| {
                received += chunk as u64;
                if last_emit.elapsed() >= Duration::from_millis(150) {
                    last_emit = Instant::now();
                    set_phase(
                        &app_chunk,
                        Phase::Downloading {
                            info: info_chunk.clone(),
                            received,
                            total,
                        },
                    );
                }
            },
            move || set_phase(&app_done, Phase::Verifying { info: info_done }),
        )
        .await;

    {
        let mut inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(bytes) => {
                inner.package = Some(bytes);
                inner.phase = Phase::Ready { info };
            }
            Err(e) => {
                inner.package = None;
                inner.phase = failed(classify(&e), Some(e.to_string()), Some(info));
            }
        }
    }
    emit_status(app);
    Ok(status(app))
}

/// Install the downloaded update. On success the installer is started and this process
/// exits (Windows cannot replace a running program); `restart` asks the installer to start
/// the new version afterwards. Refused while any work is in progress.
pub fn install<R: Runtime>(app: &AppHandle<R>, restart: bool) -> Result<(), String> {
    let busy = crate::busy(app);
    if busy.any() {
        return Err(busy.message());
    }
    let st = app.state::<UpdaterState>();
    let (update, package, info) = {
        let mut inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Phase::Ready { info } = inner.phase.clone() else {
            return Err("The update has not been downloaded yet.".into());
        };
        let (Some(update), Some(package)) = (inner.pending.clone(), inner.package.clone()) else {
            return Err(
                "The downloaded update is no longer available. Check for updates again.".into(),
            );
        };
        inner.phase = Phase::Installing { info: info.clone() };
        (update, package, info)
    };
    emit_status(app);
    // `install` verifies the package again, then starts the installer and exits.
    match update.restart_after_install(restart).install(&package) {
        Ok(()) => {
            // Only reached where installing does not exit the process (not Windows).
            if restart {
                app.restart();
            }
            app.exit(0);
            Ok(())
        }
        Err(e) => {
            let kind = classify(&e);
            set_phase(app, failed(kind, Some(e.to_string()), Some(info)));
            Err(describe(kind).into())
        }
    }
}

/// "Remind me later": the notice stays away for a day for this version.
pub fn remind_later<R: Runtime>(app: &AppHandle<R>) -> UpdateStatus {
    let st = app.state::<UpdaterState>();
    {
        let mut inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = inner.phase.info().map(|i| i.version.clone()) {
            inner.record.remind_version = Some(v);
            inner.record.remind_after_ms = Some(now_ms() + REMIND_LATER_MS);
            inner.record.save(&st.record_path);
        }
    }
    emit_status(app);
    status(app)
}

/// Whether closing the window should hand over to the installer: automatic mode, a
/// verified package ready, nothing running.
pub fn install_on_close_pending<R: Runtime>(app: &AppHandle<R>) -> bool {
    let s = status(app);
    s.install_on_close && !s.busy.any()
}

/// React to the user changing the mode: switching to automatic with an update already
/// known starts its download.
pub fn on_mode_changed<R: Runtime>(app: &AppHandle<R>) {
    let s = status(app);
    if s.mode == UpdateMode::Automatic
        && s.install_kind.installs_updates()
        && matches!(s.state, Phase::Available { .. })
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = download(&app).await;
        });
    }
    emit_status(app);
}

/// Background checks: shortly after launch, then hourly, each one acting only when
/// [`check_due`] says a day (or six hours after a failure) has passed.
pub fn start_scheduler<R: Runtime>(app: AppHandle<R>) {
    let _ = std::thread::Builder::new()
        .name("update-scheduler".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                let mode = mode_of(&app);
                let due = {
                    let st = app.state::<UpdaterState>();
                    let inner = st.inner.lock().unwrap_or_else(|e| e.into_inner());
                    check_due(mode, &inner.record, now_ms())
                };
                if due {
                    let s = tauri::async_runtime::block_on(check(&app, false));
                    if s.mode == UpdateMode::Automatic
                        && s.install_kind.installs_updates()
                        && matches!(s.state, Phase::Available { .. })
                    {
                        let _ = tauri::async_runtime::block_on(download(&app));
                    }
                }
                std::thread::sleep(SCHEDULER_TICK);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    #[test]
    fn only_newer_stable_versions_are_updates() {
        let current = v("0.1.3");
        assert!(
            is_stable_update(&current, &v("0.1.4")),
            "a newer release is offered"
        );
        assert!(is_stable_update(&current, &v("1.0.0")));
        assert!(
            !is_stable_update(&current, &v("0.1.3")),
            "the running version is not"
        );
        assert!(
            !is_stable_update(&current, &v("0.1.2")),
            "an older one never is"
        );
        assert!(
            !is_stable_update(&current, &v("0.1.4-rc.1")),
            "a prerelease never is"
        );
        // A release candidate build is offered the final release of its version.
        assert!(is_stable_update(&v("0.1.4-rc.1"), &v("0.1.4")));
    }

    #[test]
    fn background_checks_respect_the_mode_and_the_interval() {
        let now = 10 * CHECK_INTERVAL_MS;
        let never = Record::default();
        assert!(
            !check_due(UpdateMode::Off, &never, now),
            "off makes no request"
        );
        assert!(
            check_due(UpdateMode::Notify, &never, now),
            "first launch checks"
        );
        assert!(check_due(UpdateMode::Automatic, &never, now));

        let ok_recently = Record {
            last_attempt_ms: Some(now - 1000),
            last_success_ms: Some(now - 1000),
            ..Record::default()
        };
        assert!(
            !check_due(UpdateMode::Notify, &ok_recently, now),
            "not twice a day"
        );
        let ok_yesterday = Record {
            last_attempt_ms: Some(now - CHECK_INTERVAL_MS),
            last_success_ms: Some(now - CHECK_INTERVAL_MS),
            ..Record::default()
        };
        assert!(check_due(UpdateMode::Notify, &ok_yesterday, now));
        assert!(
            !check_due(UpdateMode::Off, &ok_yesterday, now),
            "off stays off"
        );

        let failed_recently = Record {
            last_attempt_ms: Some(now - RETRY_INTERVAL_MS + 1000),
            last_success_ms: Some(now - 3 * CHECK_INTERVAL_MS),
            ..Record::default()
        };
        assert!(!check_due(UpdateMode::Notify, &failed_recently, now));
        let failed_earlier = Record {
            last_attempt_ms: Some(now - RETRY_INTERVAL_MS),
            ..failed_recently.clone()
        };
        assert!(
            check_due(UpdateMode::Notify, &failed_earlier, now),
            "a failure retries sooner"
        );

        let clock_set_back = Record {
            last_attempt_ms: Some(now + CHECK_INTERVAL_MS),
            last_success_ms: Some(now + CHECK_INTERVAL_MS),
            ..Record::default()
        };
        assert!(check_due(UpdateMode::Notify, &clock_set_back, now));
    }

    #[test]
    fn remind_later_applies_to_one_version_for_a_while() {
        let r = Record {
            remind_version: Some("0.1.4".into()),
            remind_after_ms: Some(1000),
            ..Record::default()
        };
        assert!(r.snoozed("0.1.4", 999));
        assert!(!r.snoozed("0.1.4", 1000), "it expires");
        assert!(
            !r.snoozed("0.1.5", 999),
            "a newer release is announced at once"
        );
    }

    #[test]
    fn work_in_progress_blocks_installing() {
        assert!(!Busy::default().any());
        let jobs = Busy {
            jobs: 2,
            operations: 0,
        };
        assert!(jobs.any());
        assert!(jobs.message().starts_with("2 jobs are still in progress"));
        let one_write = Busy {
            jobs: 0,
            operations: 1,
        };
        assert!(one_write.any());
        assert!(one_write.message().contains("1 file operation is"));
    }

    #[test]
    fn errors_are_told_apart() {
        assert_eq!(
            classify(&UpdaterError::ReleaseNotFound),
            FailureKind::Unavailable
        );
        assert_eq!(
            classify(&UpdaterError::Network("refused".into())),
            FailureKind::Network
        );
        assert_eq!(
            classify(&UpdaterError::MissingSignedVersion),
            FailureKind::Signature
        );
        assert_eq!(
            classify(&UpdaterError::SignedVersionMismatch {
                signed: "0.1.2".into(),
                announced: "0.1.4".into()
            }),
            FailureKind::Signature
        );
        assert_eq!(
            classify(&UpdaterError::TargetsNotFound(vec![])),
            FailureKind::Metadata
        );
        let bad_json = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        assert_eq!(
            classify(&UpdaterError::Serialization(bad_json)),
            FailureKind::Metadata
        );
    }

    #[test]
    fn status_serialises_as_the_window_expects() {
        let p = Phase::Downloading {
            info: UpdateInfo {
                version: "0.1.4".into(),
                current_version: "0.1.3".into(),
                date_ms: Some(1),
                notes: None,
                release_url: release_url("0.1.4"),
            },
            received: 10,
            total: Some(20),
        };
        let j = serde_json::to_value(&p).unwrap();
        assert_eq!(j["phase"], "downloading");
        assert_eq!(j["info"]["currentVersion"], "0.1.3");
        assert_eq!(j["received"], 10);
        assert_eq!(
            j["info"]["releaseUrl"],
            "https://github.com/wildbondinteractive/OST-PST-Studio/releases/tag/v0.1.4"
        );
        let f = serde_json::to_value(failed(FailureKind::Signature, None, None)).unwrap();
        assert_eq!(f["phase"], "failed");
        assert_eq!(f["kind"], "signature");
    }

    #[test]
    fn each_installer_reads_its_own_manifest_entry() {
        assert_eq!(InstallKind::Nsis.manifest_target(), "windows-x86_64-nsis");
        assert_eq!(InstallKind::Msi.manifest_target(), "windows-x86_64-msi");
        assert!(!InstallKind::Portable.installs_updates());
        assert!(InstallKind::Msi.installs_updates());
        // A test build is not packaged, so it reports itself as portable.
        assert_eq!(InstallKind::current(), InstallKind::Portable);
    }
}
