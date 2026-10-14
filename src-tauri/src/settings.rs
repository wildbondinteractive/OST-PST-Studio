//! Persistent preferences and the dashboard's history, as JSON in the app's config
//! directory. Nothing in either is sent anywhere; both are plain files the user can
//! read and delete.

use ostpst_core::model::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    pub check_for_updates: bool,
    pub message_page_size: u32,
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
            check_for_updates: false,
            message_page_size: 200,
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
            },
        );
        self.recent_files.truncate(12);
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
