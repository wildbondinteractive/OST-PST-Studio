//! Persistent preferences and the dashboard's history, as JSON in the app's config
//! directory. Nothing in either is sent anywhere; both are plain files the user can
//! read and delete.

use crate::updater::UpdateMode;
use ostpst_core::model::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// How many mailboxes the recent list keeps.
pub const RECENT_MAX: usize = 10;
/// How many mailboxes a restored session reopens at most, so a long session list can
/// never turn a launch into minutes of scanning.
pub const SESSION_MAX: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "system", "light" or "dark".
    pub theme: String,
    pub default_output_dir: Option<String>,
    pub default_export_format: ExportFormat,
    pub collision: Collision,
    pub max_concurrent_jobs: u32,
    pub write_allocation_maps: bool,
    pub validate_after_write: bool,
    pub block_remote_content: bool,
    pub show_html_bodies: bool,
    pub log_verbosity: String,
    pub remember_recent_files: bool,
    /// Reopen, on launch, the mailboxes that were open when the app last closed.
    pub restore_session: bool,
    /// Return to the folder last viewed when a recent mailbox is reopened.
    pub remember_last_folder: bool,
    /// Automatic, notify (the default) or off. See `updater.rs`.
    pub update_mode: UpdateMode,
    pub message_page_size: u32,
    /// Keep saved scans so large or damaged files reopen without a new sweep.
    pub use_scan_cache: bool,
    /// Where saved scans live; empty means the application's cache directory.
    pub cache_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: "system".into(),
            default_output_dir: None,
            default_export_format: ExportFormat::Eml,
            collision: Collision::Rename,
            max_concurrent_jobs: 2,
            write_allocation_maps: true,
            validate_after_write: true,
            block_remote_content: true,
            show_html_bodies: true,
            log_verbosity: "normal".into(),
            remember_recent_files: true,
            restore_session: false,
            remember_last_folder: true,
            update_mode: UpdateMode::Notify,
            message_page_size: 200,
            use_scan_cache: true,
            cache_dir: None,
        }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        std::fs::read_to_string(dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .map(|s: Settings| s.normalised())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join("settings.json"), s);
        }
    }

    pub fn normalised(mut self) -> Settings {
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        self.max_concurrent_jobs = self.max_concurrent_jobs.clamp(1, 8);
        self.message_page_size = self.message_page_size.clamp(50, 2000);
        if let Some(d) = &self.default_output_dir {
            if d.trim().is_empty() {
                self.default_output_dir = None;
            }
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub is_ost: bool,
    pub health: HealthStatus,
    pub folder_count: usize,
    pub message_count: usize,
    pub opened_ms: i64,
    /// The folder last viewed in this mailbox, by node id, when remembering it is on.
    #[serde(default)]
    pub last_folder: Option<u32>,
}

/// Whether a recent file can still be opened, checked when the list is shown.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecentState {
    Available,
    Missing,
    Inaccessible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentStatus {
    pub path: String,
    pub state: RecentState,
    pub detail: Option<String>,
}

/// Whether a path still names a readable file. Opens it for reading and reads nothing,
/// which is what tells "moved" apart from "locked or no permission".
pub fn recent_status(path: &str) -> RecentStatus {
    let p = Path::new(path);
    let (state, detail) = match std::fs::metadata(p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            RecentState::Missing,
            Some("not found; moved, renamed or deleted".into()),
        ),
        Err(e) => (RecentState::Inaccessible, Some(e.to_string())),
        Ok(m) if !m.is_file() => (RecentState::Missing, Some("no longer a file".into())),
        Ok(_) => match std::fs::File::open(p) {
            Ok(_) => (RecentState::Available, None),
            Err(e) => (RecentState::Inaccessible, Some(e.to_string())),
        },
    };
    RecentStatus {
        path: path.to_string(),
        state,
        detail,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PastJob {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub source: String,
    pub destination: String,
    pub finished_ms: i64,
    pub duration_ms: u64,
    pub messages: usize,
    pub validation: Option<String>,
    pub report_json: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    pub recent_files: Vec<RecentFile>,
    pub jobs: Vec<PastJob>,
    /// Paths of the mailboxes open right now, for restoring the session next launch.
    pub session: Vec<String>,
}

impl History {
    pub fn load(dir: &Path) -> History {
        std::fs::read_to_string(dir.join("history.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join("history.json"), s);
        }
    }

    pub fn touch_file(&mut self, path: &str, info: &FileInfo) {
        let last_folder = self
            .recent_files
            .iter()
            .find(|f| f.path == path)
            .and_then(|f| f.last_folder);
        self.recent_files.retain(|f| f.path != path);
        self.recent_files.insert(
            0,
            RecentFile {
                path: path.to_string(),
                file_name: info.file_name.clone(),
                size_bytes: info.size_bytes,
                is_ost: info.is_ost,
                health: info.health,
                folder_count: info.folder_count,
                message_count: info.message_count,
                opened_ms: now_ms(),
                last_folder,
            },
        );
        self.recent_files.truncate(RECENT_MAX);
    }

    pub fn remove_recent(&mut self, path: &str) {
        self.recent_files.retain(|f| f.path != path);
    }

    pub fn set_last_folder(&mut self, path: &str, folder: Option<u32>) {
        if let Some(f) = self.recent_files.iter_mut().find(|f| f.path == path) {
            f.last_folder = folder;
        }
    }

    pub fn session_add(&mut self, path: &str) {
        if !self.session.iter().any(|p| p == path) {
            self.session.push(path.to_string());
        }
    }

    pub fn session_remove(&mut self, path: &str) {
        self.session.retain(|p| p != path);
    }

    pub fn push_job(&mut self, rec: &super::jobs::JobRecord) {
        self.jobs.retain(|j| j.id != rec.id);
        let report = rec.report.as_ref();
        self.jobs.insert(
            0,
            PastJob {
                id: rec.id.clone(),
                kind: rec.kind,
                status: rec.status,
                source: rec.source.clone(),
                destination: rec.destination.clone(),
                finished_ms: rec.finished_ms.unwrap_or_else(now_ms),
                duration_ms: report.map(|r| r.duration_ms).unwrap_or(0),
                messages: report.map(|r| r.output_counts.messages).unwrap_or(0),
                validation: report.map(|r| format!("{:?}", r.validation.status)),
                report_json: report.map(|r| r.to_json()),
            },
        );
        self.jobs.truncate(50);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str) -> FileInfo {
        let mut i: FileInfo = serde_json::from_value(serde_json::json!({
            "path": name, "fileName": name, "sizeBytes": 1, "declaredBytes": 1, "isOst": false,
            "format": "unicode", "formatVersion": 23, "encoding": "none", "amapValid": true,
            "indexSource": "header", "health": "healthy", "nodeCount": 1, "blockCount": 1,
            "folderCount": 1, "messageCount": 1, "nodeTypes": [], "warnings": [],
            "problemCount": 0, "recovery": null, "essentialsMissing": []
        }))
        .expect("a FileInfo");
        i.file_name = name.into();
        i
    }

    #[test]
    fn recent_list_is_capped_most_recent_first_and_keeps_the_last_folder() {
        let mut h = History::default();
        for n in 0..15 {
            h.touch_file(&format!("C:\\m{n}.pst"), &info(&format!("m{n}.pst")));
        }
        assert_eq!(h.recent_files.len(), RECENT_MAX);
        assert_eq!(h.recent_files[0].path, "C:\\m14.pst");
        h.set_last_folder("C:\\m10.pst", Some(0x8022));
        h.touch_file("C:\\m10.pst", &info("m10.pst"));
        assert_eq!(h.recent_files[0].path, "C:\\m10.pst");
        assert_eq!(
            h.recent_files[0].last_folder,
            Some(0x8022),
            "reopening keeps the folder"
        );
        assert_eq!(h.recent_files.len(), RECENT_MAX, "no duplicate entry");
        h.remove_recent("C:\\m10.pst");
        assert!(h.recent_files.iter().all(|f| f.path != "C:\\m10.pst"));
    }

    #[test]
    fn session_tracks_open_paths_once() {
        let mut h = History::default();
        h.session_add("a.pst");
        h.session_add("a.pst");
        h.session_add("b.ost");
        assert_eq!(h.session, vec!["a.pst", "b.ost"]);
        h.session_remove("a.pst");
        assert_eq!(h.session, vec!["b.ost"]);
    }

    #[test]
    fn recent_status_tells_missing_from_available() {
        let dir = std::env::temp_dir().join(format!("ostpst-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("present.pst");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            recent_status(file.to_str().unwrap()).state,
            RecentState::Available
        );
        let gone = dir.join("gone.pst");
        assert_eq!(
            recent_status(gone.to_str().unwrap()).state,
            RecentState::Missing
        );
        assert_eq!(
            recent_status(dir.to_str().unwrap()).state,
            RecentState::Missing,
            "a folder is not a mailbox"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_settings_files_load_with_the_new_defaults() {
        // A 0.1.2 settings file: has the retired checkForUpdates switch, lacks the new keys.
        let old = r#"{"theme":"dark","checkForUpdates":false,"maxConcurrentJobs":3}"#;
        let s: Settings = serde_json::from_str(old).expect("old settings still parse");
        assert_eq!(s.theme, "dark");
        assert_eq!(s.max_concurrent_jobs, 3);
        assert_eq!(s.update_mode, UpdateMode::Notify);
        assert!(!s.restore_session);
        assert!(s.remember_last_folder);
    }

    #[test]
    fn update_mode_round_trips_through_the_settings_file() {
        let dir = std::env::temp_dir().join(format!("ostpst-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for mode in [UpdateMode::Automatic, UpdateMode::Off, UpdateMode::Notify] {
            let s = Settings {
                update_mode: mode,
                ..Settings::default()
            };
            s.save(&dir);
            assert_eq!(
                Settings::load(&dir).update_mode,
                mode,
                "the mode survives a restart"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
