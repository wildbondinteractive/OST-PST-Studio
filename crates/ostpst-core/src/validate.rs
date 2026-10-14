//! Check a written PST against what it was built from.
//!
//! This is the engine checking its own work, which proves less than it sounds: a reader
//! that agrees with the writer it shares code with has found nothing. It still catches
//! the failures that matter most, a structure written wrong (every checksum in the file
//! was computed over bytes the writer laid down, so a single warning on reopening means
//! a bug), a node that went missing, a folder or a message that cannot be read back.
//! Independent validation with libpff, and opening in Outlook, are separate steps and
//! are reported separately.

use crate::mailbox::{Mailbox, OpenOptions};
use crate::model::*;
use crate::report::{Check, CheckStatus, ValidationReport, ValidationStatus};
use std::path::Path;

/// What the source said, for the comparison.
#[derive(Debug, Clone, Default)]
pub struct Expected {
    pub nodes: usize,
    pub folders: usize,
    pub messages: usize,
    /// Nodes the writer reported dropping; the node count is compared net of these.
    pub dropped_nodes: usize,
    pub allocation_maps_written: bool,
}

/// Reopen `path` and check it. Reads every folder and every message so a value that
/// points at a lost subnode is found now rather than by the person reading their mail.
pub fn validate_output(path: &Path, expected: &Expected, on: crate::Progress) -> ValidationReport {
    let started = std::time::Instant::now();
    let mut r = ValidationReport::not_run();
    let mut checks: Vec<Check> = Vec::new();
    let mut fail = false;
    let mut warn = false;

    let mut push = |name: &str, status: CheckStatus, detail: String| {
        match status {
            CheckStatus::Failed => fail = true,
            CheckStatus::Warning => warn = true,
            _ => {}
        }
        checks.push(Check {
            name: name.into(),
            status,
            detail,
        });
    };

    let mut mb = match Mailbox::open(
        path,
        OpenOptions {
            salvage: false,
            cache_dir: None,
        },
        &mut crate::silent(),
    ) {
        Ok(m) => {
            push(
                "Reopen",
                CheckStatus::Passed,
                "the file opens from its own header and index".into(),
            );
            m
        }
        Err(e) => {
            push("Reopen", CheckStatus::Failed, e);
            r.checks = checks;
            r.status = ValidationStatus::Failed;
            r.elapsed_ms = started.elapsed().as_millis() as u64;
            return r;
        }
    };
    let info = mb.info().clone();
    r.node_count = info.node_count;
    r.folder_count = info.folder_count;
    r.message_count = info.message_count;

    push(
        "Format",
        if !info.is_ost && info.format == FileFormat::Unicode {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        format!(
            "{} with {:?} layout, version {}",
            if info.is_ost { "OST" } else { "PST" },
            info.format,
            info.format_version
        ),
    );
    push(
        "Index source",
        if info.index_source == IndexSource::Header {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        format!("{:?}", info.index_source),
    );
    push(
        "Essential nodes",
        if info.essentials_missing.is_empty() {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        if info.essentials_missing.is_empty() {
            "message store and root folder present".into()
        } else {
            format!("missing: {}", info.essentials_missing.join(", "))
        },
    );

    // Warnings other than the allocation-map one mean a structure was written wrong.
    let structural: Vec<&String> = info
        .warnings
        .iter()
        .filter(|w| !w.contains("allocation map"))
        .collect();
    push(
        "Structural warnings on reopen",
        if structural.is_empty() {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        if structural.is_empty() {
            "none".into()
        } else {
            structural
                .iter()
                .take(5)
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        },
    );
    push(
        "Allocation maps",
        if info.amap_valid {
            CheckStatus::Passed
        } else if expected.allocation_maps_written {
            CheckStatus::Failed
        } else {
            CheckStatus::Warning
        },
        if info.amap_valid {
            "marked valid".into()
        } else {
            "marked invalid; Outlook will ask for SCANPST before opening the file".into()
        },
    );

    let want_nodes = expected.nodes.saturating_sub(expected.dropped_nodes);
    push(
        "Node count",
        if expected.nodes == 0 || info.node_count == want_nodes {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        format!("{} in output, {} expected", info.node_count, want_nodes),
    );
    push(
        "Folder count",
        if expected.folders == 0 || info.folder_count == expected.folders {
            CheckStatus::Passed
        } else if info.folder_count + expected.dropped_nodes >= expected.folders {
            CheckStatus::Warning
        } else {
            CheckStatus::Failed
        },
        format!(
            "{} in output, {} in source",
            info.folder_count, expected.folders
        ),
    );
    push(
        "Message count",
        if expected.messages == 0 || info.message_count == expected.messages {
            CheckStatus::Passed
        } else if info.message_count + expected.dropped_nodes >= expected.messages {
            CheckStatus::Warning
        } else {
            CheckStatus::Failed
        },
        format!(
            "{} in output, {} in source",
            info.message_count, expected.messages
        ),
    );

    // Every folder readable, and every message readable all the way down.
    let folders = mb.folders();
    let unreadable_folders = folders.iter().filter(|f| f.unreadable).count();
    push(
        "Folders readable",
        if unreadable_folders == 0 {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        format!(
            "{} of {} folders read",
            folders.len() - unreadable_folders,
            folders.len()
        ),
    );

    let message_nids: Vec<u32> = mb
        .nodes()
        .iter()
        .filter(|n| n.nid_type() == 0x04)
        .map(|n| n.nid)
        .collect();
    let total = message_nids.len() as u64;
    let mut read = 0usize;
    let mut bad = 0usize;
    let mut attachments = 0usize;
    let mut lost_values = 0usize;
    let mut cancelled = false;
    for (i, nid) in message_nids.iter().enumerate() {
        if !on(i as u64 + 1, total) {
            cancelled = true;
            break;
        }
        match mb.message(*nid) {
            Ok(d) => {
                read += 1;
                attachments += d.attachments.len();
                lost_values += d.problems.len();
            }
            Err(_) => bad += 1,
        }
    }
    r.messages_read = read;
    r.messages_unreadable = bad;
    r.attachments_read = attachments;
    push(
        "Messages readable",
        if cancelled {
            CheckStatus::Skipped
        } else if bad == 0 {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        if cancelled {
            format!("stopped after {read} of {total}")
        } else {
            format!("{read} read, {bad} unreadable, {attachments} attachments listed")
        },
    );
    if lost_values > 0 {
        push(
            "Lost values",
            CheckStatus::Warning,
            format!("{lost_values} property value(s) point at subnodes that are not in the file"),
        );
    }

    r.checks = checks;
    r.status = if fail {
        ValidationStatus::Failed
    } else if warn {
        ValidationStatus::PassedWithWarnings
    } else {
        ValidationStatus::Passed
    };
    r.elapsed_ms = started.elapsed().as_millis() as u64;
    r
}
