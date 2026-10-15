//! The updater driven end to end inside a mock Tauri app, against a local HTTP server
//! standing in for GitHub: the real plugin fetches the manifest, compares versions,
//! downloads and verifies minisign signatures. The fixtures were signed once with a
//! throwaway key whose private half was never kept; only its public key is here.
//!
//! Nothing in these tests can install anything: every path that would start an installer
//! (and exit the process) is either refused by the busy check or not taken.

use crate::jobs::JobSpec;
use crate::updater::{self, FailureKind, InstallKind, Phase, UpdateMode, UpdaterState};
use crate::OperationGuard;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/updater");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).expect("updater fixture")
}

fn fixture_text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap().trim().to_string()
}

#[derive(Clone)]
struct Route {
    path: &'static str,
    status: u16,
    body: Vec<u8>,
}

/// A tiny HTTP/1.1 server on a free local port. Routes can be swapped while it runs, and
/// every request is counted by path.
struct Server {
    base: String,
    routes: Arc<Mutex<Vec<Route>>>,
    hits: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(routes: Vec<Route>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes = Arc::new(Mutex::new(routes));
        let hits = Arc::new(Mutex::new(Vec::new()));
        let (r, h) = (routes.clone(), hits.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let req = String::from_utf8_lossy(&buf);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                h.lock().unwrap().push(path.clone());
                let (status, body) = r
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|x| x.path == path)
                    .map(|x| (x.status, x.body.clone()))
                    .unwrap_or((404, b"not found".to_vec()));
                let head = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(&body);
            }
        });
        Server { base, routes, hits }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn hits(&self, path: &str) -> usize {
        self.hits
            .lock()
            .unwrap()
            .iter()
            .filter(|p| *p == path)
            .count()
    }

    fn set(&self, route: Route) {
        let mut r = self.routes.lock().unwrap();
        r.retain(|x| x.path != route.path);
        r.push(route);
    }
}

fn manifest(server: &Server, version: &str, signature: &str, targets: &[&str]) -> Vec<u8> {
    let mut platforms = serde_json::Map::new();
    for t in targets {
        let file = if t.ends_with("msi") {
            "/msi.bin"
        } else {
            "/nsis.bin"
        };
        platforms.insert(
            t.to_string(),
            serde_json::json!({ "url": server.url(file), "signature": signature }),
        );
    }
    serde_json::json!({
        "version": version,
        "notes": "## Changed\n- Test notes",
        "pub_date": "2026-10-15T12:00:00Z",
        "platforms": platforms,
    })
    .to_string()
    .into_bytes()
}

fn route(path: &'static str, status: u16, body: Vec<u8>) -> Route {
    Route { path, status, body }
}

/// A server publishing `version`, signed with `sig`, for both installer types.
fn release(version: &str, sig: &str) -> Server {
    let server = Server::start(vec![
        route("/nsis.bin", 200, fixture("update-0.2.0.bin")),
        route("/msi.bin", 200, fixture("update-0.2.0.bin")),
    ]);
    let m = manifest(
        &server,
        version,
        &fixture_text(sig),
        &["windows-x86_64-nsis", "windows-x86_64-msi"],
    );
    server.set(route("/latest.json", 200, m));
    server
}

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "ostpst-updater-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A mock application running `current`, installed as `kind`, reading `endpoint`.
fn app(endpoint: &str, current: &str, kind: InstallKind, mode: UpdateMode) -> App<MockRuntime> {
    let mut ctx = mock_context(noop_assets());
    ctx.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey": fixture_text("test-key.pub"),
            "endpoints": [endpoint],
            // The local test server speaks plain HTTP; the shipped configuration does not
            // allow this (see the configuration test below).
            "dangerousInsecureTransportProtocol": true,
            "requireSignedVersion": true,
        }),
    );
    ctx.package_info_mut().version = current.parse().unwrap();
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(ctx)
        .expect("mock app");
    let dir = temp_dir();
    let state = crate::new_app_state(dir.clone(), dir.join("cache"));
    state.settings.lock().unwrap().update_mode = mode;
    app.manage(state);
    app.manage(UpdaterState::with_kind(&dir, kind));
    app
}

fn check(app: &App<MockRuntime>, manual: bool) -> updater::UpdateStatus {
    tauri::async_runtime::block_on(updater::check(app.handle(), manual))
}

fn download(app: &App<MockRuntime>) -> Result<updater::UpdateStatus, String> {
    tauri::async_runtime::block_on(updater::download(app.handle()))
}

fn failure(s: &updater::UpdateStatus) -> FailureKind {
    match &s.state {
        Phase::Failed { kind, .. } => *kind,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn a_newer_signed_release_is_found_downloaded_and_verified() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    let s = check(&app, true);
    let Phase::Available { info } = &s.state else {
        panic!("expected an update, got {:?}", s.state)
    };
    assert_eq!(info.version, "0.2.0");
    assert_eq!(info.current_version, "0.1.0");
    assert_eq!(info.notes.as_deref(), Some("## Changed\n- Test notes"));
    assert!(info.date_ms.is_some(), "the release date is passed on");
    assert!(s.last_success_ms.is_some());

    let s = download(&app).expect("download");
    assert!(matches!(s.state, Phase::Ready { .. }), "{:?}", s.state);
    // An NSIS install fetched the NSIS package, never the MSI.
    assert_eq!(server.hits("/nsis.bin"), 1);
    assert_eq!(server.hits("/msi.bin"), 0);
}

#[test]
fn an_msi_install_updates_from_the_msi() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Msi,
        UpdateMode::Notify,
    );
    check(&app, true);
    let s = download(&app).expect("download");
    assert!(matches!(s.state, Phase::Ready { .. }));
    assert_eq!(server.hits("/msi.bin"), 1);
    assert_eq!(server.hits("/nsis.bin"), 0);
}

#[test]
fn the_running_version_is_up_to_date() {
    let server = release("0.1.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    let s = check(&app, true);
    assert_eq!(s.state, Phase::UpToDate);
    assert!(s.last_success_ms.is_some());
}

#[test]
fn an_older_release_is_never_offered() {
    let server = release("0.0.9", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    assert_eq!(check(&app, true).state, Phase::UpToDate);
}

#[test]
fn a_prerelease_never_reaches_the_stable_channel() {
    let server = release("0.2.0-rc.1", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    assert_eq!(check(&app, true).state, Phase::UpToDate);
}

#[test]
fn no_network_is_reported_as_such() {
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/latest.json", l.local_addr().unwrap())
    };
    let app = app(&closed, "0.1.0", InstallKind::Nsis, UpdateMode::Notify);
    let s = check(&app, true);
    assert_eq!(failure(&s), FailureKind::Network);
    assert!(s.last_check_ms.is_some(), "the attempt is recorded");
    assert!(s.last_success_ms.is_none());
}

#[test]
fn an_endpoint_error_is_reported_as_no_update_information() {
    for status in [404u16, 500] {
        let server = Server::start(vec![route("/latest.json", status, b"no".to_vec())]);
        let app = app(
            &server.url("/latest.json"),
            "0.1.0",
            InstallKind::Nsis,
            UpdateMode::Notify,
        );
        assert_eq!(
            failure(&check(&app, true)),
            FailureKind::Unavailable,
            "HTTP {status}"
        );
    }
}

#[test]
fn malformed_or_incomplete_metadata_is_rejected() {
    let bodies: [&[u8]; 4] = [
        b"{this is not json",
        br#"{"notes":"no version or platforms"}"#,
        br#"{"version":"0.2.0"}"#,
        br#"{"version":"not a version","platforms":{}}"#,
    ];
    for body in bodies {
        let server = Server::start(vec![route("/latest.json", 200, body.to_vec())]);
        let app = app(
            &server.url("/latest.json"),
            "0.1.0",
            InstallKind::Nsis,
            UpdateMode::Notify,
        );
        assert_eq!(
            failure(&check(&app, true)),
            FailureKind::Metadata,
            "{}",
            String::from_utf8_lossy(body)
        );
    }
}

#[test]
fn a_release_without_this_installer_type_is_not_installed_from_another() {
    // Only an MSI is published: an NSIS install must not be offered it.
    let server = Server::start(vec![]);
    let m = manifest(
        &server,
        "0.2.0",
        &fixture_text("update-0.2.0.bin.sig"),
        &["windows-x86_64-msi"],
    );
    server.set(route("/latest.json", 200, m));
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    assert_eq!(failure(&check(&app, true)), FailureKind::Metadata);
}

#[test]
fn a_tampered_package_is_rejected_and_discarded() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    server.set(route("/nsis.bin", 200, b"something else entirely".to_vec()));
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    let s = download(&app).expect("the command itself succeeds");
    assert_eq!(failure(&s), FailureKind::Signature);
    let st = app.state::<UpdaterState>();
    assert!(st.has_no_package(), "nothing unverified is kept");
}

#[test]
fn a_package_signed_with_another_key_is_rejected() {
    let server = release("0.2.0", "update-0.2.0.bin.otherkey.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    assert_eq!(failure(&download(&app).unwrap()), FailureKind::Signature);
}

#[test]
fn an_old_signed_package_cannot_be_passed_off_as_a_new_version() {
    // The package is genuinely signed, but for 0.1.5; the manifest claims 0.2.0.
    let server = release("0.2.0", "update-0.2.0.bin.v0.1.5.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    assert_eq!(failure(&download(&app).unwrap()), FailureKind::Signature);
}

#[test]
fn a_signature_that_is_not_a_signature_is_rejected() {
    let server = Server::start(vec![route("/nsis.bin", 200, fixture("update-0.2.0.bin"))]);
    let m = manifest(
        &server,
        "0.2.0",
        "not base64 at all",
        &["windows-x86_64-nsis"],
    );
    server.set(route("/latest.json", 200, m));
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    assert_eq!(failure(&download(&app).unwrap()), FailureKind::Signature);
}

#[test]
fn a_failed_download_is_reported_without_crashing() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    server.set(route("/nsis.bin", 503, b"busy".to_vec()));
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    let s = download(&app).unwrap();
    assert_eq!(failure(&s), FailureKind::Network);
    // The update is still known, so the user can try again.
    assert_eq!(s.state.info().map(|i| i.version.as_str()), Some("0.2.0"));
}

#[test]
fn checking_by_hand_works_when_updates_are_off() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Off,
    );
    assert_eq!(
        server.hits("/latest.json"),
        0,
        "nothing is requested on its own"
    );
    let s = check(&app, true);
    assert!(matches!(s.state, Phase::Available { .. }));
    assert_eq!(server.hits("/latest.json"), 1);
    assert_eq!(s.mode, UpdateMode::Off);
}

#[test]
fn a_failed_background_check_keeps_a_known_update() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, false);
    server.set(route("/latest.json", 500, b"down".to_vec()));
    assert!(matches!(check(&app, false).state, Phase::Available { .. }));
    // ...but a check the user asked for says what happened.
    assert_eq!(failure(&check(&app, true)), FailureKind::Unavailable);
}

#[test]
fn installing_waits_for_jobs_and_file_operations() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Automatic,
    );
    check(&app, true);
    download(&app).unwrap();
    let state = app.state::<crate::AppState>();

    // A job that is queued (concurrency 0 keeps it from starting) blocks the install.
    state.jobs.start(
        app.handle().clone(),
        JobSpec::Verify {
            source: "C:\\nowhere\\mailbox.pst".into(),
        },
        0,
        std::env::temp_dir(),
    );
    let err = updater::install(app.handle(), true).unwrap_err();
    assert!(err.starts_with("1 job is still in progress"), "{err}");
    assert!(!updater::install_on_close_pending(app.handle()));
    assert!(
        matches!(updater::status(app.handle()).state, Phase::Ready { .. }),
        "still ready, not installing"
    );

    state.jobs.cancel_all();
    assert_eq!(state.jobs.active_count(), 0);

    // So does a file being written outside the job queue.
    {
        let _op = OperationGuard::new(&state);
        let err = updater::install(app.handle(), true).unwrap_err();
        assert!(err.contains("1 file operation is"), "{err}");
        assert!(!updater::install_on_close_pending(app.handle()));
    }

    // With nothing running, automatic mode would now install on close. (Not exercised:
    // installing exits the process.)
    assert!(updater::install_on_close_pending(app.handle()));
    assert!(updater::status(app.handle()).install_on_close);
}

#[test]
fn notify_mode_never_installs_on_close() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    download(&app).unwrap();
    assert!(!updater::install_on_close_pending(app.handle()));
}

#[test]
fn installing_before_downloading_is_refused() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    assert!(updater::install(app.handle(), true).is_err());
}

#[test]
fn a_portable_copy_is_told_about_updates_but_never_downloads_them() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Portable,
        UpdateMode::Automatic,
    );
    let s = check(&app, true);
    assert!(matches!(s.state, Phase::Available { .. }));
    assert!(!s.install_on_close);
    assert!(download(&app).is_err());
    assert_eq!(server.hits("/nsis.bin") + server.hits("/msi.bin"), 0);
}

#[test]
fn remind_me_later_snoozes_this_version() {
    let server = release("0.2.0", "update-0.2.0.bin.sig");
    let app = app(
        &server.url("/latest.json"),
        "0.1.0",
        InstallKind::Nsis,
        UpdateMode::Notify,
    );
    check(&app, true);
    assert!(!updater::status(app.handle()).snoozed);
    let s = updater::remind_later(app.handle());
    assert!(s.snoozed);
    assert!(
        matches!(s.state, Phase::Available { .. }),
        "the update is still shown in Settings"
    );
}

/// The configuration that ships: the stable manifest over HTTPS, signed versions required,
/// nothing insecure, a real minisign public key, and no updater artifacts in ordinary builds
/// (so CI and local builds never need the private key).
#[test]
fn the_shipped_configuration_is_the_stable_secure_one() {
    let conf: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json")).unwrap(),
    )
    .unwrap();
    let u = &conf["plugins"]["updater"];
    assert_eq!(
        u["endpoints"],
        serde_json::json!([
            "https://github.com/wildbondinteractive/OST-PST-Studio/releases/latest/download/latest.json"
        ])
    );
    assert_eq!(u["requireSignedVersion"], true);
    assert_eq!(u["windows"]["installMode"], "passive");
    for flag in [
        "dangerousInsecureTransportProtocol",
        "dangerousAcceptInvalidCerts",
        "dangerousAcceptInvalidHostnames",
        "allowDowngrades",
    ] {
        assert!(
            u.get(flag).is_none() || u[flag] == false,
            "{flag} must not be set"
        );
    }
    use base64::Engine;
    let key = base64::engine::general_purpose::STANDARD
        .decode(u["pubkey"].as_str().unwrap())
        .expect("the public key is base64");
    let key = String::from_utf8(key).unwrap();
    assert!(
        key.starts_with("untrusted comment: minisign public key"),
        "{key}"
    );
    assert!(minisign_verify::PublicKey::decode(&key).is_ok());
    assert_ne!(conf["bundle"]["createUpdaterArtifacts"], true);

    let release: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tauri.release.conf.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert!(
        release.get("plugins").is_none(),
        "the release build uses the same updater settings"
    );
}
