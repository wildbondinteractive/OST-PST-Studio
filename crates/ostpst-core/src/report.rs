//! What a job leaves behind: a record of what was read, what was written, what was lost
//! and what was checked. Serialisable as JSON, and renderable as text or HTML.
//!
//! The report is careful about one distinction: *completed* means the writer finished
//! and the file was moved into place; *validated* means the result was reopened and
//! checked against the source. Only the second is a claim about the output, and even
//! that is a claim this engine makes about itself. Outlook compatibility is reported
//! separately and only as far as it has actually been established.

use crate::model::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationStatus {
    NotRun,
    Passed,
    PassedWithWarnings,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub status: ValidationStatus,
    pub checks: Vec<Check>,
    pub node_count: usize,
    pub folder_count: usize,
    pub message_count: usize,
    pub messages_read: usize,
    pub messages_unreadable: usize,
    pub attachments_read: usize,
    pub elapsed_ms: u64,
}

impl ValidationReport {
    pub fn not_run() -> ValidationReport {
        ValidationReport {
            status: ValidationStatus::NotRun,
            checks: Vec::new(),
            node_count: 0,
            folder_count: 0,
            message_count: 0,
            messages_read: 0,
            messages_unreadable: 0,
            attachments_read: 0,
            elapsed_ms: 0,
        }
    }
}

/// How far Outlook compatibility of an output file has actually been established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutlookCompatibility {
    /// Nothing about Outlook has been checked for this file.
    Unverified,
    /// The file is structurally complete but its allocation maps are marked invalid;
    /// Outlook will ask for the Inbox Repair Tool (SCANPST) before opening it.
    RequiresScanpst,
    /// Written with valid allocation maps. Files written this way opened directly in
    /// classic Outlook during testing on the public fixtures; this particular file has
    /// not been opened in Outlook.
    MapsWritten,
    /// Opened in Outlook by a person and confirmed. Set only by a manual workflow.
    VerifiedManually,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub path: String,
    pub size_bytes: u64,
    pub format: Option<FileFormat>,
    pub is_ost: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub folders: usize,
    pub messages: usize,
    pub attachments: usize,
    pub nodes: usize,
    pub blocks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobReport {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub app_version: String,
    pub source: FileSummary,
    pub destination: FileSummary,
    pub started_ms: i64,
    pub finished_ms: i64,
    pub duration_ms: u64,
    pub source_health: Option<HealthStatus>,
    pub index_source: Option<IndexSource>,
    pub source_counts: Counts,
    pub output_counts: Counts,
    /// Items the writer or exporter left out, with the reason for each.
    pub skipped: Vec<String>,
    pub dropped_nodes: usize,
    pub dropped_blocks: usize,
    /// Everything the reader found wrong with the source.
    pub corruption: Vec<String>,
    pub corruption_count: usize,
    /// What was done about it: sweeping, carving, revisions chosen.
    pub recovery_actions: Vec<String>,
    pub validation: ValidationReport,
    pub outlook: OutlookCompatibility,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    /// Export-specific: how many files were written, in what format.
    pub export_format: Option<ExportFormat>,
    pub files_written: usize,
}

impl JobReport {
    pub fn new(id: &str, kind: JobKind, source: &str, destination: &str) -> JobReport {
        JobReport {
            id: id.to_string(),
            kind,
            status: JobStatus::Running,
            app_version: crate::VERSION.to_string(),
            source: FileSummary {
                path: source.to_string(),
                ..Default::default()
            },
            destination: FileSummary {
                path: destination.to_string(),
                ..Default::default()
            },
            started_ms: now_ms(),
            finished_ms: 0,
            duration_ms: 0,
            source_health: None,
            index_source: None,
            source_counts: Counts::default(),
            output_counts: Counts::default(),
            skipped: Vec::new(),
            dropped_nodes: 0,
            dropped_blocks: 0,
            corruption: Vec::new(),
            corruption_count: 0,
            recovery_actions: Vec::new(),
            validation: ValidationReport::not_run(),
            outlook: OutlookCompatibility::Unverified,
            warnings: Vec::new(),
            errors: Vec::new(),
            notes: Vec::new(),
            export_format: None,
            files_written: 0,
        }
    }

    pub fn finish(&mut self, status: JobStatus) {
        self.finished_ms = now_ms();
        self.duration_ms = (self.finished_ms - self.started_ms).max(0) as u64;
        self.status = status;
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    /// A plain-text rendering, for the console and for saving beside the output.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        let kind = match self.kind {
            JobKind::Convert => "Conversion",
            JobKind::Repair => "Repair",
            JobKind::Export => "Export",
            JobKind::Verify => "Verification",
            JobKind::ExtractAttachments => "Attachment extraction",
        };
        s.push_str(&format!(
            "{kind} report (OST/PST Studio {})\n",
            self.app_version
        ));
        s.push_str(&format!("Status: {}\n", status_word(self.status)));
        s.push_str(&format!(
            "Source: {} ({})\n",
            self.source.path,
            crate::fsutil::human_bytes(self.source.size_bytes)
        ));
        if !self.destination.path.is_empty() {
            s.push_str(&format!(
                "Destination: {} ({})\n",
                self.destination.path,
                crate::fsutil::human_bytes(self.destination.size_bytes)
            ));
        }
        s.push_str(&format!(
            "Started: {}   Finished: {}   Duration: {}\n",
            iso(self.started_ms),
            iso(self.finished_ms),
            human_ms(self.duration_ms)
        ));
        if let Some(h) = self.source_health {
            s.push_str(&format!("Source health: {h:?}"));
            if let Some(i) = self.index_source {
                s.push_str(&format!(", index: {i:?}"));
            }
            s.push('\n');
        }
        s.push_str(&format!(
            "Source: {} folders, {} messages, {} attachments, {} nodes, {} blocks\n",
            self.source_counts.folders,
            self.source_counts.messages,
            self.source_counts.attachments,
            self.source_counts.nodes,
            self.source_counts.blocks
        ));
        if self.kind != JobKind::Verify {
            s.push_str(&format!(
                "Output: {} folders, {} messages, {} attachments, {} nodes, {} blocks\n",
                self.output_counts.folders,
                self.output_counts.messages,
                self.output_counts.attachments,
                self.output_counts.nodes,
                self.output_counts.blocks
            ));
        }
        if let Some(f) = self.export_format {
            s.push_str(&format!(
                "Format: {:?}, {} file(s) written\n",
                f, self.files_written
            ));
        }
        if self.dropped_nodes > 0 || self.dropped_blocks > 0 {
            s.push_str(&format!(
                "Left out: {} node(s), {} block(s) that did not survive\n",
                self.dropped_nodes, self.dropped_blocks
            ));
        }
        section(&mut s, "Skipped items", &self.skipped);
        if self.corruption_count > self.corruption.len() {
            s.push_str(&format!(
                "\nDetected corruption ({} problems, {} listed):\n",
                self.corruption_count,
                self.corruption.len()
            ));
            for c in &self.corruption {
                s.push_str(&format!("  - {c}\n"));
            }
        } else {
            section(&mut s, "Detected corruption", &self.corruption);
        }
        section(&mut s, "Recovery actions", &self.recovery_actions);
        s.push_str(&format!("\nValidation: {:?}\n", self.validation.status));
        for c in &self.validation.checks {
            s.push_str(&format!(
                "  [{}] {}: {}\n",
                check_word(c.status),
                c.name,
                c.detail
            ));
        }
        s.push_str(&format!(
            "Outlook compatibility: {}\n",
            match self.outlook {
                OutlookCompatibility::Unverified => "not verified",
                OutlookCompatibility::RequiresScanpst =>
                    "allocation maps marked invalid; Outlook will require SCANPST before opening",
                OutlookCompatibility::MapsWritten =>
                    "written with valid allocation maps; files written this way open directly in classic Outlook in testing (this file itself was not opened in Outlook)",
                OutlookCompatibility::VerifiedManually =>
                    "opened in Outlook and confirmed manually",
            }
        ));
        section(&mut s, "Warnings", &self.warnings);
        section(&mut s, "Errors", &self.errors);
        section(&mut s, "Notes", &self.notes);
        s
    }

    /// A self-contained HTML rendering, for saving and sharing.
    pub fn to_html(&self) -> String {
        let text = self.to_text();
        let mut h = String::new();
        h.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>");
        h.push_str(&esc(&format!("{:?} report", self.kind)));
        h.push_str("</title><style>body{font:14px/1.5 Segoe UI,system-ui,sans-serif;color:#1b1b1b;background:#fff;max-width:960px;margin:2rem auto;padding:0 1rem}h1{font-size:1.4rem}h2{font-size:1.05rem;margin-top:1.5rem;border-bottom:1px solid #ddd;padding-bottom:.25rem}table{border-collapse:collapse;width:100%}td,th{text-align:left;padding:.3rem .5rem;border-bottom:1px solid #eee;vertical-align:top}.ok{color:#107c10}.warn{color:#9d5d00}.fail{color:#c42b1c}.muted{color:#666}pre{white-space:pre-wrap;background:#f6f6f6;padding:1rem;border-radius:6px}</style></head><body>");
        h.push_str(&format!(
            "<h1>{:?} report <span class=\"muted\">OST/PST Studio {}</span></h1>",
            self.kind,
            esc(&self.app_version)
        ));
        h.push_str("<table>");
        let row = |h: &mut String, k: &str, v: &str| {
            h.push_str(&format!("<tr><th>{}</th><td>{}</td></tr>", esc(k), esc(v)));
        };
        row(&mut h, "Status", status_word(self.status));
        row(
            &mut h,
            "Source",
            &format!(
                "{} ({})",
                self.source.path,
                crate::fsutil::human_bytes(self.source.size_bytes)
            ),
        );
        if !self.destination.path.is_empty() {
            row(
                &mut h,
                "Destination",
                &format!(
                    "{} ({})",
                    self.destination.path,
                    crate::fsutil::human_bytes(self.destination.size_bytes)
                ),
            );
        }
        row(&mut h, "Started", &iso(self.started_ms));
        row(&mut h, "Finished", &iso(self.finished_ms));
        row(&mut h, "Duration", &human_ms(self.duration_ms));
        row(
            &mut h,
            "Validation",
            &format!("{:?}", self.validation.status),
        );
        row(
            &mut h,
            "Outlook compatibility",
            match self.outlook {
                OutlookCompatibility::Unverified => "not verified",
                OutlookCompatibility::RequiresScanpst => "requires SCANPST",
                OutlookCompatibility::MapsWritten => {
                    "valid allocation maps written; opens directly in classic Outlook in testing"
                }
                OutlookCompatibility::VerifiedManually => "confirmed manually in Outlook",
            },
        );
        h.push_str("</table>");
        if !self.validation.checks.is_empty() {
            h.push_str("<h2>Validation checks</h2><table>");
            for c in &self.validation.checks {
                let cls = match c.status {
                    CheckStatus::Passed => "ok",
                    CheckStatus::Warning | CheckStatus::Skipped => "warn",
                    CheckStatus::Failed => "fail",
                };
                h.push_str(&format!(
                    "<tr><td class=\"{cls}\">{}</td><td>{}</td><td>{}</td></tr>",
                    check_word(c.status),
                    esc(&c.name),
                    esc(&c.detail)
                ));
            }
            h.push_str("</table>");
        }
        h.push_str("<h2>Full report</h2><pre>");
        h.push_str(&esc(&text));
        h.push_str("</pre></body></html>");
        h
    }
}

fn section(s: &mut String, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    s.push_str(&format!("\n{title}:\n"));
    for i in items {
        s.push_str(&format!("  - {i}\n"));
    }
}

pub fn status_word(s: JobStatus) -> &'static str {
    match s {
        JobStatus::Queued => "queued",
        JobStatus::Running => "running",
        JobStatus::Completed => "completed",
        JobStatus::CompletedWithWarnings => "completed with warnings",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

fn check_word(s: CheckStatus) -> &'static str {
    match s {
        CheckStatus::Passed => "pass",
        CheckStatus::Warning => "warn",
        CheckStatus::Failed => "FAIL",
        CheckStatus::Skipped => "skip",
    }
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Unix milliseconds as `YYYY-MM-DD HH:MM:SS UTC`.
pub fn iso(ms: i64) -> String {
    if ms <= 0 {
        return "-".into();
    }
    crate::ltp::filetime_seconds(ms_to_filetime(ms))
}

pub fn human_ms(ms: u64) -> String {
    let s = ms / 1000;
    if s < 60 {
        format!("{}.{:01} s", s, (ms % 1000) / 100)
    } else if s < 3600 {
        format!("{} min {} s", s / 60, s % 60)
    } else {
        format!("{} h {} min", s / 3600, (s % 3600) / 60)
    }
}
