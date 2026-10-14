//! The long jobs, end to end: convert or repair to a new PST, export, verify.
//!
//! Each one opens its own `Mailbox` on the source (so a window browsing the same file
//! is never blocked), writes to a `.partial` beside the destination, validates, and only
//! then moves the result into place. Progress goes out as [`JobProgress`] snapshots; the
//! callback's `false` cancels at the next safe point, and a cancelled job leaves nothing
//! behind that could be mistaken for a finished file.

use crate::fsutil;
use crate::mailbox::{Mailbox, OpenOptions};
use crate::model::*;
use crate::report::{JobReport, OutlookCompatibility, ValidationStatus};
use crate::validate::{validate_output, Expected};
use std::path::Path;
use std::time::Instant;

/// What a job calls with its progress. `false` asks it to stop.
pub type Reporter<'a> = &'a mut dyn FnMut(&JobProgress) -> bool;

/// Tracks one job's progress across phases and throttles what is reported.
struct Tracker<'a> {
    on: Reporter<'a>,
    started: Instant,
    last: Instant,
    p: JobProgress,
    cancelled: bool,
}

impl<'a> Tracker<'a> {
    fn new(on: Reporter<'a>) -> Tracker<'a> {
        Tracker {
            on,
            started: Instant::now(),
            last: Instant::now() - std::time::Duration::from_secs(1),
            p: JobProgress::default(),
            cancelled: false,
        }
    }

    fn phase(&mut self, name: &str, determinate: bool) -> bool {
        self.p.phase = name.to_string();
        self.p.done = 0;
        self.p.total = 0;
        self.p.determinate = determinate;
        self.p.eta_ms = None;
        self.emit(true)
    }

    fn emit(&mut self, force: bool) -> bool {
        if self.cancelled {
            return false;
        }
        let now = Instant::now();
        if !force && now.duration_since(self.last).as_millis() < 100 {
            return true;
        }
        self.last = now;
        self.p.elapsed_ms = self.started.elapsed().as_millis() as u64;
        if self.p.determinate && self.p.done > 0 && self.p.total > self.p.done && self.p.elapsed_ms > 2000 {
            let per = self.p.elapsed_ms as f64 / self.p.done as f64;
            self.p.eta_ms = Some((per * (self.p.total - self.p.done) as f64) as u64);
        }
        if !(self.on)(&self.p) {
            self.cancelled = true;
            return false;
        }
        true
    }

    /// An engine progress callback bound to this tracker for the current phase.
    fn engine(&mut self) -> impl FnMut(u64, u64) -> bool + use<'_, 'a> {
        move |done, total| {
            self.p.done = done;
            self.p.total = total;
            self.p.determinate = total > 0;
            self.emit(done >= total)
        }
    }
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn fill_source(report: &mut JobReport, mb: &Mailbox) {
    let info = mb.info();
    report.source.size_bytes = info.size_bytes;
    report.source.format = Some(info.format);
    report.source.is_ost = Some(info.is_ost);
    report.source_health = Some(info.health);
    report.index_source = Some(info.index_source);
    report.source_counts.folders = info.folder_count;
    report.source_counts.messages = info.message_count;
    report.source_counts.nodes = info.node_count;
    report.source_counts.blocks = info.block_count;
    report.corruption = info.warnings.clone();
    report.corruption_count = info.problem_count;
    if let Some(r) = &info.recovery {
        report.recovery_actions.push(format!(
            "The header's index was unusable or bypassed. Swept {} pages: {} node pages and {} block pages survived, {} failed their checksum.",
            r.pages_scanned, r.nbt_pages, r.bbt_pages, r.damaged_pages
        ));
        if r.carved_blocks > 0 {
            report.recovery_actions.push(format!(
                "Carved {} blocks out of the file with no index at all.",
                r.carved_blocks
            ));
        }
        report
            .recovery_actions
            .push(format!("Recovered {} nodes from surviving index pages.", r.swept_nodes));
        if r.superseded > 0 {
            report.recovery_actions.push(format!(
                "{} superseded index entries ignored (older copies of things that later moved); {} could not be confirmed against the bytes on disk.",
                r.superseded, r.unresolved
            ));
        }
        if !r.stale.is_empty() {
            let gone = r.stale.iter().filter(|s| s.dangling).count();
            report.recovery_actions.push(format!(
                "{} node(s) recovered at a revision that cannot be confirmed, {} of them pointing at data no longer in the file. These read normally but may be an older copy.",
                r.stale.len(),
                gone
            ));
            for s in r.stale.iter().take(20) {
                report.recovery_actions.push(if s.dangling {
                    format!("  node 0x{:X}: its data block {} is not in the file", s.nid, s.bid_data)
                } else {
                    format!(
                        "  node 0x{:X}: {} revisions survive, took block {} as the newest",
                        s.nid, s.versions, s.bid_data
                    )
                });
            }
        }
    }
    if !info.essentials_missing.is_empty() {
        report.warnings.push(format!(
            "The source is missing {}. A PST written from it will not open in Outlook; export is the way to get the mail out.",
            info.essentials_missing.join(" and ")
        ));
    }
}

/// Convert an OST or ANSI PST to a Unicode PST, or repair a PST to a new file. The two
/// are the same job: `repair::rebuild` copies a 512-byte-page Unicode file block for
/// block and converts anything else a level up.
pub fn run_convert(id: &str, kind: JobKind, opts: &ConvertOptions, on: Reporter) -> JobReport {
    let mut report = JobReport::new(id, kind, &opts.source, &opts.destination);
    let mut t = Tracker::new(on);
    let src = Path::new(&opts.source);
    let dst = Path::new(&opts.destination);

    macro_rules! fail {
        ($r:expr, $e:expr) => {{
            let e: String = $e;
            if crate::is_cancelled(&e) {
                $r.finish(JobStatus::Cancelled);
            } else {
                $r.errors.push(e);
                $r.finish(JobStatus::Failed);
            }
            return $r;
        }};
    }

    if !t.phase("Opening source", false) {
        fail!(report, crate::CANCELLED.into());
    }
    let mut mb = match Mailbox::open(src, OpenOptions { salvage: opts.salvage }, &mut t.engine()) {
        Ok(m) => m,
        Err(e) => fail!(report, e),
    };
    fill_source(&mut report, &mb);
    if opts.salvage {
        report
            .recovery_actions
            .insert(0, "Salvage mode: the header's index was ignored and rebuilt by sweeping.".into());
    }

    // Where the output goes, decided before any work is done.
    let target = match fsutil::resolve_output(dst, opts.collision) {
        Ok(p) => p,
        Err(e) => fail!(report, e),
    };
    report.destination.path = target.to_string_lossy().to_string();
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                fail!(report, format!("{}: {e}", parent.display()));
            }
        }
    }
    // An OST's blocks are compressed and a converted PST is plain, so the output can be
    // larger than the source. Two times is a ceiling seen in practice, not a rule.
    let needed = if mb.info().format == FileFormat::Unicode4k {
        mb.info().size_bytes * 2
    } else {
        mb.info().size_bytes + mb.info().size_bytes / 50
    };
    if let Err(e) = fsutil::check_space(&target, needed) {
        fail!(report, e);
    }
    let partial = fsutil::partial_path(&target);
    let partial_str = partial.to_string_lossy().to_string();

    let phase = if mb.info().format == FileFormat::Unicode && !mb.info().is_ost {
        "Copying blocks and rebuilding the index"
    } else {
        "Converting data streams to PST blocks"
    };
    if !t.phase(phase, true) {
        fail!(report, crate::CANCELLED.into());
    }
    let nodes = mb.nodes().to_vec();
    let blocks = mb.blocks().to_vec();
    let rebuilt = {
        let mut engine = t.engine();
        crate::repair::rebuild(mb.pst_mut(), &nodes, &blocks, &partial_str, &mut engine)
    };
    let rebuilt = match rebuilt {
        Ok(r) => r,
        Err(e) => {
            fsutil::discard_partial(&partial);
            fail!(report, e);
        }
    };
    report.output_counts.nodes = rebuilt.nodes;
    report.output_counts.blocks = rebuilt.blocks;
    report.dropped_nodes = rebuilt.dropped_nodes;
    report.dropped_blocks = rebuilt.dropped_blocks;
    report.skipped.extend(rebuilt.problems.iter().cloned());
    if rebuilt.converted {
        report.notes.push(format!(
            "Converted from {}. Every data stream was decoded and laid out again as a Unicode PST stores them, so this is a new file rather than a copy.",
            rebuilt.source
        ));
    }
    if !rebuilt.missing.is_empty() {
        report.errors.push(format!(
            "The output has no {}. That node's data did not survive in the source, so no index can point at it. The file will not open in Outlook; this program can still read it and export the mail.",
            rebuilt.missing.join(", no ")
        ));
    }

    // Allocation maps, so Outlook opens the file without SCANPST.
    let mut maps_written = false;
    if opts.write_allocation_maps {
        if !t.phase("Writing allocation maps", false) {
            fsutil::discard_partial(&partial);
            fail!(report, crate::CANCELLED.into());
        }
        match crate::amap::write_allocation_maps(&partial) {
            Ok(summary) => {
                maps_written = true;
                report.notes.push(summary);
            }
            Err(e) => report.warnings.push(format!(
                "Allocation maps could not be written ({e}); the file is left marked for SCANPST instead."
            )),
        }
    }

    // Validation, against the source's counts.
    if opts.validate {
        if !t.phase("Validating the output", true) {
            fsutil::discard_partial(&partial);
            fail!(report, crate::CANCELLED.into());
        }
        let expected = Expected {
            nodes: nodes.len(),
            folders: mb.info().folder_count,
            messages: mb.info().message_count,
            dropped_nodes: rebuilt.dropped_nodes,
            allocation_maps_written: maps_written,
        };
        let v = validate_output(&partial, &expected, &mut t.engine());
        report.output_counts.folders = v.folder_count;
        report.output_counts.messages = v.message_count;
        report.output_counts.attachments = v.attachments_read;
        report.validation = v;
    } else {
        report.notes.push("Validation was switched off for this job.".into());
    }

    if t.cancelled {
        fsutil::discard_partial(&partial);
        fail!(report, crate::CANCELLED.into());
    }
    if let Err(e) = fsutil::commit_partial(&partial, &target, opts.collision) {
        fail!(report, e);
    }
    report.destination.size_bytes = file_size(&target);
    report.destination.format = Some(FileFormat::Unicode);
    report.destination.is_ost = Some(false);
    report.outlook = if !rebuilt.missing.is_empty() {
        OutlookCompatibility::Unverified
    } else if maps_written {
        OutlookCompatibility::MapsWritten
    } else {
        OutlookCompatibility::RequiresScanpst
    };

    let status = if !report.errors.is_empty() || report.validation.status == ValidationStatus::Failed {
        JobStatus::Failed
    } else if !report.warnings.is_empty()
        || !report.skipped.is_empty()
        || report.dropped_nodes > 0
        || report.validation.status == ValidationStatus::PassedWithWarnings
    {
        JobStatus::CompletedWithWarnings
    } else {
        JobStatus::Completed
    };
    if status == JobStatus::Failed && report.validation.status == ValidationStatus::Failed {
        report.errors.push(
            "The output did not pass validation. It was kept so the mail can still be exported from it, but it should not be trusted as a finished PST.".into(),
        );
    }
    report.finish(status);
    report
}

/// Export a selection to a directory in one of the mail formats.
pub fn run_export(id: &str, opts: &ExportOptions, on: Reporter) -> JobReport {
    let mut report = JobReport::new(id, JobKind::Export, &opts.source, &opts.destination);
    report.export_format = Some(opts.format);
    let mut t = Tracker::new(on);
    let src = Path::new(&opts.source);
    let dst = Path::new(&opts.destination);

    macro_rules! fail {
        ($r:expr, $e:expr) => {{
            let e: String = $e;
            if crate::is_cancelled(&e) {
                $r.finish(JobStatus::Cancelled);
            } else {
                $r.errors.push(e);
                $r.finish(JobStatus::Failed);
            }
            return $r;
        }};
    }

    if !t.phase("Opening source", false) {
        fail!(report, crate::CANCELLED.into());
    }
    let mut mb = match Mailbox::open(src, OpenOptions { salvage: opts.salvage }, &mut t.engine()) {
        Ok(m) => m,
        Err(e) => fail!(report, e),
    };
    fill_source(&mut report, &mb);

    let messages = mb.select(&opts.selection);
    if messages.is_empty() {
        fail!(report, "nothing is selected for export".into());
    }
    if let Err(e) = std::fs::create_dir_all(dst) {
        fail!(report, format!("{}: {e}", dst.display()));
    }

    if !t.phase("Writing messages", true) {
        fail!(report, crate::CANCELLED.into());
    }
    let result = crate::exporters::export_messages(&mut mb, &messages, dst, opts, &mut t.engine());
    let st = match result {
        Ok(s) => s,
        Err(e) => fail!(report, e),
    };
    report.files_written = st.files;
    report.output_counts.messages = st.messages;
    report.output_counts.attachments = st.attachments;
    report.output_counts.folders = st.folders;
    report.skipped.extend(st.errors.iter().cloned());
    if st.cancelled {
        report.notes.push(format!(
            "Stopped by the user after {} message(s). What was written is complete and valid.",
            st.messages
        ));
        report.finish(JobStatus::Cancelled);
        return report;
    }
    report.destination.size_bytes = st.bytes;
    let status = if st.messages == 0 && st.failed > 0 {
        JobStatus::Failed
    } else if st.failed > 0 || !report.warnings.is_empty() {
        JobStatus::CompletedWithWarnings
    } else {
        JobStatus::Completed
    };
    if st.failed > 0 {
        report.warnings.push(format!(
            "{} message(s) could not be written; they are listed under skipped items.",
            st.failed
        ));
    }
    report.finish(status);
    report
}

/// What is wrong with a file, and what a sweep would recover. Writes nothing.
pub fn run_verify(id: &str, source: &str, on: Reporter) -> JobReport {
    let mut report = JobReport::new(id, JobKind::Verify, source, "");
    let mut t = Tracker::new(on);

    if !t.phase("Opening source", false) {
        report.finish(JobStatus::Cancelled);
        return report;
    }
    let mut mb = match Mailbox::open(Path::new(source), OpenOptions::default(), &mut t.engine()) {
        Ok(m) => m,
        Err(e) => {
            report.errors.push(e);
            report.finish(JobStatus::Failed);
            return report;
        }
    };
    fill_source(&mut report, &mb);

    if !t.phase("Checking every block against its checksum", true) {
        report.finish(JobStatus::Cancelled);
        return report;
    }
    let blocks = mb.blocks().to_vec();
    let before = mb.pst_mut().problem_count();
    let (mut ok, mut bad) = (0usize, 0usize);
    {
        let mut engine = t.engine();
        let total = blocks.len() as u64;
        for (i, b) in blocks.iter().enumerate() {
            if !engine(i as u64 + 1, total) {
                report.finish(JobStatus::Cancelled);
                return report;
            }
            match mb.pst_mut().block(b.bid) {
                Ok(_) => ok += 1,
                Err(_) => bad += 1,
            }
        }
    }
    let rotten = mb.pst_mut().problem_count().saturating_sub(before);
    report.notes.push(format!(
        "{ok} blocks read, {bad} unreadable, {rotten} failed their checksum."
    ));

    if !t.phase("Sweeping for surviving index pages", false) {
        report.finish(JobStatus::Cancelled);
        return report;
    }
    let r = mb.pst_mut().scan();
    report.notes.push(format!(
        "{} pages swept, {} of them index pages ({} node, {} block), {} failed their checksum, {} superseded entries, {} unresolved.",
        r.pages_scanned,
        r.nbt_pages + r.bbt_pages,
        r.nbt_pages,
        r.bbt_pages,
        r.damaged_pages,
        r.superseded,
        r.unresolved
    ));
    let known: std::collections::HashSet<u32> = mb.nodes().iter().map(|n| n.nid).collect();
    let extra: Vec<&crate::ndb::Node> = r.nodes.iter().filter(|n| !known.contains(&n.nid)).collect();
    if extra.is_empty() {
        report
            .notes
            .push("Sweeping finds nothing the index has lost: the two agree.".into());
    } else {
        let msgs = extra.iter().filter(|n| n.nid_type() == 0x04).count();
        let folders = extra.iter().filter(|n| n.nid_type() == 0x02).count();
        report.notes.push(format!(
            "Sweeping recovers {} node(s) the index cannot reach: {msgs} message(s) and {folders} folder(s). Open the file in salvage mode to use them; they are usually deleted items whose index entries were dropped.",
            extra.len()
        ));
    }

    if !t.phase("Carving blocks out of the file", false) {
        report.finish(JobStatus::Cancelled);
        return report;
    }
    let carved = mb.pst_mut().carve().len();
    report
        .notes
        .push(format!("{carved} blocks can be carved out of the file with no index at all."));

    // The folders' own tables against the index's parent pointers.
    if !t.phase("Cross-checking folder tables", false) {
        report.finish(JobStatus::Cancelled);
        return report;
    }
    let nodes = mb.nodes().to_vec();
    let by_nid: std::collections::BTreeMap<u32, crate::ndb::Node> =
        nodes.iter().map(|n| (n.nid, *n)).collect();
    let (mut compared, mut disagree) = (0usize, 0usize);
    for f in nodes.iter().filter(|n| n.nid_type() == 0x02) {
        for (table_type, want) in [(0x0Du32, [0x02u8, 0x03].as_slice()), (0x0E, [0x04].as_slice())] {
            let Some(tnode) = by_nid.get(&((f.nid & !0x1F) | table_type)) else { continue };
            let Ok(tc) = crate::ltp::read_tc(mb.pst_mut(), tnode.bid_data, tnode.bid_sub) else { continue };
            let listed: std::collections::BTreeSet<u32> = tc.rows.iter().map(|r| r.id).collect();
            let pointed: std::collections::BTreeSet<u32> = nodes
                .iter()
                .filter(|n| want.contains(&n.nid_type()) && n.nid_parent == f.nid && n.nid != f.nid)
                .map(|n| n.nid)
                .collect();
            compared += 1;
            if listed != pointed {
                disagree += 1;
                report.warnings.push(format!(
                    "folder 0x{:X}: its own table and the node parents disagree ({} only in the table, {} only by parent)",
                    f.nid,
                    listed.difference(&pointed).count(),
                    pointed.difference(&listed).count()
                ));
            }
        }
    }
    report.notes.push(format!(
        "{compared} folder tables cross-checked against the node parents, {disagree} disagreed."
    ));

    // The reader's accumulated problems, after everything above touched the file.
    let info_warnings = mb.pst_mut().warnings.clone();
    report.corruption = info_warnings;
    report.corruption_count = mb.pst_mut().problem_count();

    let status = if bad > 0 || rotten > 0 || !report.corruption.is_empty() || disagree > 0 {
        JobStatus::CompletedWithWarnings
    } else {
        JobStatus::Completed
    };
    report.finish(status);
    report
}
