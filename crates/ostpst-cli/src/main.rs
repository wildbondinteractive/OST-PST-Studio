//! `ostpst`: the OST/PST Studio engine on the command line, for technicians and scripts.
//!
//! Every command the window offers is here, with `--json` for anything that wants to
//! parse the result. The engine is the same code the window runs, so the two agree.

use ostpst_core::jobs::{run_convert, run_export, run_verify};
use ostpst_core::mailbox::{Mailbox, OpenOptions};
use ostpst_core::model::*;
use ostpst_core::report::JobReport;
use std::io::Write;
use std::path::{Path, PathBuf};

const USAGE: &str = "\
ostpst - read, search, export, convert and repair Outlook PST/OST files

  ostpst info <file>                       what the file is, and what is wrong with it
  ostpst tree <file>                       the folder tree with message counts
  ostpst list <file> [--folder <nid|name>] [--limit N]
                                           messages, newest first
  ostpst show <file> <nid>                 one message in full
  ostpst props <file> <nid>                every property on one node, labelled
  ostpst search <file> <text> [--folder <nid|name>] [--subject] [--sender]
                              [--body] [--recipients] [--attachments] [--limit N]
  ostpst export <file> <dir> [--format eml|msg|mbox|html|txt|csv|vcf|ics|json]
                             [--folder <nid|name>]... [--message <nid>]...
                             [--extract-attachments] [--overwrite|--rename]
  ostpst convert <file.ost> <out.pst> [--no-validate] [--no-maps]
                                      [--overwrite|--rename]
  ostpst repair  <file.pst> <out.pst> [--no-validate] [--no-maps]
                                      [--overwrite|--rename]
  ostpst verify <file>                     every checksum, and what a sweep would recover
  ostpst version

Global options:
  --salvage      ignore the header's index and rebuild it by sweeping the file
  --json         print machine-readable JSON instead of text
  --report <path>  (convert, repair, export, verify) also save the report as JSON
  --quiet        no progress on stderr

No Outlook is needed and no password is ever asked for: a PST password is a
checksum, not a key, and it protects nothing.
";

struct Args {
    command: String,
    positional: Vec<String>,
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Args {
    fn parse() -> Args {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        let mut values = Vec::new();
        let valued = [
            "--folder",
            "--message",
            "--limit",
            "--format",
            "--report",
            "--collision",
        ];
        let mut i = 0;
        while i < raw.len() {
            let a = &raw[i];
            if a.starts_with("--") {
                if valued.contains(&a.as_str()) {
                    if let Some(v) = raw.get(i + 1) {
                        values.push((a.clone(), v.clone()));
                        i += 2;
                        continue;
                    }
                }
                flags.push(a.clone());
            } else {
                positional.push(a.clone());
            }
            i += 1;
        }
        let command = positional.first().cloned().unwrap_or_default();
        Args {
            command,
            positional: positional.into_iter().skip(1).collect(),
            flags,
            values,
        }
    }
    fn flag(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x == f)
    }
    fn value(&self, k: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(a, _)| a == k)
            .map(|(_, v)| v.as_str())
    }
    fn all(&self, k: &str) -> Vec<&str> {
        self.values
            .iter()
            .filter(|(a, _)| a == k)
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

fn die(msg: &str, code: i32) -> ! {
    eprintln!("{msg}");
    std::process::exit(code);
}

/// Progress on one line of stderr, once a job has run long enough to need it.
fn progress(quiet: bool) -> impl FnMut(&JobProgress) -> bool {
    let start = std::time::Instant::now();
    let mut last = start;
    let mut last_phase = String::new();
    move |p: &JobProgress| {
        if quiet {
            return true;
        }
        let now = std::time::Instant::now();
        if p.phase != last_phase {
            if !last_phase.is_empty() {
                eprintln!();
            }
            last_phase = p.phase.clone();
            eprint!("  {}...", p.phase);
            let _ = std::io::stderr().flush();
            return true;
        }
        if now - last < std::time::Duration::from_millis(200) && p.done < p.total {
            return true;
        }
        last = now;
        if p.determinate && p.total > 0 {
            eprint!(
                "\r  {}: {} of {} ({}%)   ",
                p.phase,
                p.done,
                p.total,
                p.done * 100 / p.total
            );
        } else if p.done > 0 {
            eprint!("\r  {}: {}   ", p.phase, p.done);
        }
        let _ = std::io::stderr().flush();
        true
    }
}

fn open(path: &str, salvage: bool, quiet: bool) -> Mailbox {
    let mut on = |done: u64, _total: u64| {
        if !quiet && done == 0 {
            eprintln!("  Opening {path}...");
        }
        true
    };
    match Mailbox::open(
        Path::new(path),
        OpenOptions {
            salvage,
            cache_dir: None,
        },
        &mut on,
    ) {
        Ok(m) => m,
        Err(e) => die(&e, 1),
    }
}

fn folder_arg(mb: &Mailbox, want: &str) -> u32 {
    let folders = mb.folders();
    if let Some(f) = folders
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(want) || f.path.eq_ignore_ascii_case(want))
    {
        return f.nid;
    }
    if let Ok(nid) = u32::from_str_radix(want.trim_start_matches("0x"), 16) {
        if folders.iter().any(|f| f.nid == nid) {
            return nid;
        }
    }
    die(
        &format!("no folder called \"{want}\" in this file. `tree` lists them."),
        1,
    )
}

fn nid_arg(s: Option<&String>) -> u32 {
    match s.and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()) {
        Some(n) => n,
        None => die("a node id in hex is needed, e.g. 0x200044", 2),
    }
}

fn print_json<T: serde::Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn date(ms: Option<i64>) -> String {
    ms.map(ostpst_core::report::iso).unwrap_or_default()
}

fn trim(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "~"
    }
}

fn save_report(args: &Args, r: &JobReport) {
    if let Some(p) = args.value("--report") {
        if let Err(e) = std::fs::write(p, r.to_json()) {
            eprintln!("could not write {p}: {e}");
        }
    }
}

fn finish_job(args: &Args, r: JobReport) {
    save_report(args, &r);
    if !args.flag("--quiet") {
        eprintln!();
    }
    if args.flag("--json") {
        print_json(&r);
    } else {
        print!("{}", r.to_text());
    }
    std::process::exit(match r.status {
        JobStatus::Completed | JobStatus::CompletedWithWarnings => 0,
        JobStatus::Cancelled => 3,
        _ => 1,
    });
}

fn main() {
    let args = Args::parse();
    let json = args.flag("--json");
    let quiet = args.flag("--quiet");
    let salvage = args.flag("--salvage");
    let collision = if args.flag("--overwrite") {
        Collision::Overwrite
    } else if args.flag("--rename") {
        Collision::Rename
    } else {
        Collision::Fail
    };

    match args.command.as_str() {
        "version" | "--version" | "-V" => println!("ostpst {}", ostpst_core::VERSION),
        "" | "help" | "--help" | "-h" => print!("{USAGE}"),
        "info" => {
            let file = args.positional.first().unwrap_or_else(|| die(USAGE, 2));
            let mb = open(file, salvage, quiet);
            let info = mb.info();
            if json {
                print_json(info);
                return;
            }
            println!("{}", info.path);
            println!(
                "  {} in {:?} layout (version {}), {} on disk, {} declared",
                if info.is_ost { "OST" } else { "PST" },
                info.format,
                info.format_version,
                ostpst_core::fsutil::human_bytes(info.size_bytes),
                ostpst_core::fsutil::human_bytes(info.declared_bytes)
            );
            println!("  encoding: {}", info.encoding);
            println!(
                "  health: {:?}, index from {:?}",
                info.health, info.index_source
            );
            println!(
                "  {} folders, {} messages, {} nodes, {} blocks",
                info.folder_count, info.message_count, info.node_count, info.block_count
            );
            for t in &info.node_types {
                println!("    {:>7}  0x{:02X}  {}", t.count, t.nid_type, t.name);
            }
            if let Some(r) = &info.recovery {
                println!(
                    "\n  Index recovered by sweeping: {} pages scanned, {} node pages, {} block pages, {} damaged, {} blocks carved, {} nodes",
                    r.pages_scanned, r.nbt_pages, r.bbt_pages, r.damaged_pages, r.carved_blocks, r.swept_nodes
                );
                for s in r.stale.iter().take(20) {
                    println!(
                        "    - node 0x{:X}: {} revisions{}",
                        s.nid,
                        s.versions,
                        if s.dangling {
                            ", data block missing"
                        } else {
                            ""
                        }
                    );
                }
            }
            if !info.essentials_missing.is_empty() {
                println!("\n  MISSING: {}", info.essentials_missing.join(", "));
            }
            if info.warnings.is_empty() {
                println!("\n  No structural damage found.");
            } else {
                println!("\n  {} problem(s):", info.problem_count);
                for w in &info.warnings {
                    println!("    - {w}");
                }
            }
        }
        "tree" => {
            let file = args.positional.first().unwrap_or_else(|| die(USAGE, 2));
            let mb = open(file, salvage, quiet);
            let folders = mb.folders();
            if json {
                print_json(&folders);
                return;
            }
            let by: std::collections::BTreeMap<u32, &FolderInfo> =
                folders.iter().map(|f| (f.nid, f)).collect();
            fn branch(nid: u32, depth: usize, by: &std::collections::BTreeMap<u32, &FolderInfo>) {
                let Some(f) = by.get(&nid) else { return };
                let counts = match (f.message_count, f.unread_count) {
                    (0, _) => String::new(),
                    (c, 0) => format!("  ({c})"),
                    (c, u) => format!("  ({c}, {u} unread)"),
                };
                println!(
                    "{}{}{}  [0x{:X}]",
                    "  ".repeat(depth),
                    f.name,
                    counts,
                    f.nid
                );
                for c in &f.children {
                    branch(*c, depth + 1, by);
                }
            }
            branch(0x122, 0, &by);
            let orphans: Vec<&FolderInfo> = folders.iter().filter(|f| f.orphan).collect();
            if !orphans.is_empty() {
                println!("\n{} folder(s) not reachable from the root:", orphans.len());
                for o in orphans {
                    println!("  {}  ({})  [0x{:X}]", o.name, o.message_count, o.nid);
                }
            }
        }
        "list" => {
            let file = args.positional.first().unwrap_or_else(|| die(USAGE, 2));
            let mut mb = open(file, salvage, quiet);
            let limit: usize = args
                .value("--limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(usize::MAX);
            let folders: Vec<u32> = match args.value("--folder") {
                Some(f) => {
                    let root = folder_arg(&mb, f);
                    mb.subtree(root)
                }
                None => mb.folders().iter().map(|f| f.nid).collect(),
            };
            let names = mb.folder_names();
            let mut rows: Vec<MessageSummary> = Vec::new();
            for f in folders {
                rows.extend(mb.all_summaries(f));
            }
            rows.sort_by_key(|m| std::cmp::Reverse(m.date_ms));
            rows.truncate(limit);
            if json {
                print_json(&rows);
                return;
            }
            println!(
                "{:<10}  {:<19}  {:<20}  {:<20}  subject",
                "nid", "date (UTC)", "folder", "from"
            );
            for m in &rows {
                println!(
                    "0x{:<8X}  {:<19}  {:<20}  {:<20}  {}{}{}",
                    m.nid,
                    date(m.date_ms),
                    trim(
                        names
                            .get(&m.folder)
                            .map(String::as_str)
                            .unwrap_or("(no folder)"),
                        20
                    ),
                    trim(&m.sender_name, 20),
                    m.subject,
                    if m.has_attachments { "  [att]" } else { "" },
                    if m.size > 0 {
                        format!("  [{} bytes]", m.size)
                    } else {
                        String::new()
                    }
                );
            }
            println!("\n{} message(s).", rows.len());
        }
        "show" => {
            let file = args.positional.first().unwrap_or_else(|| die(USAGE, 2));
            let nid = nid_arg(args.positional.get(1));
            let mut mb = open(file, salvage, quiet);
            let d = mb.message(nid).unwrap_or_else(|e| die(&e, 1));
            if json {
                print_json(&d);
                return;
            }
            println!("Subject: {}", d.summary.subject);
            println!(
                "From: {} <{}>",
                d.summary.sender_name, d.summary.sender_email
            );
            for (k, label) in [(1, "To"), (2, "Cc"), (3, "Bcc")] {
                let list: Vec<String> = d
                    .recipients
                    .iter()
                    .filter(|r| r.kind == k)
                    .map(|r| format!("{} <{}>", r.name, r.email))
                    .collect();
                if !list.is_empty() {
                    println!("{label}: {}", list.join(", "));
                }
            }
            println!("Date: {}", date(d.summary.date_ms));
            println!(
                "Class: {}  Size: {}  Type: {:?}",
                d.summary.message_class, d.summary.size, d.summary.item_type
            );
            for a in &d.attachments {
                println!(
                    "Attachment: {} ({}, {}){}",
                    a.name,
                    ostpst_core::fsutil::human_bytes(a.size),
                    a.mime,
                    if a.is_embedded_message {
                        " [embedded message]"
                    } else {
                        ""
                    }
                );
            }
            for (k, v) in &d.extra {
                println!("{k}: {v}");
            }
            for p in &d.problems {
                println!("PROBLEM: {p}");
            }
            println!();
            match (&d.body_text, &d.body_html) {
                (Some(t), _) => println!("{t}"),
                (None, Some(h)) => println!("{}", ostpst_core::html::to_text(h)),
                _ => println!("(no body)"),
            }
        }
        "props" => {
            let file = args.positional.first().unwrap_or_else(|| die(USAGE, 2));
            let nid = nid_arg(args.positional.get(1));
            let mut mb = open(file, salvage, quiet);
            // `--raw` dumps the node's data blocks as hex, which is the only way to look at
            // the handful of special nodes (search queues, templates) that are not heaps.
            if args.flag("--raw") {
                let node = *mb
                    .nodes()
                    .iter()
                    .find(|n| n.nid == nid)
                    .unwrap_or_else(|| die(&format!("no node 0x{nid:X} in this file"), 1));
                println!(
                    "node 0x{nid:X}: parent 0x{:X}, data block {}, subnode block {}",
                    node.nid_parent, node.bid_data, node.bid_sub
                );
                match mb.pst_mut().node_blocks(node.bid_data) {
                    Ok(blocks) => {
                        for (i, b) in blocks.iter().enumerate() {
                            println!("block {i}: {} bytes", b.len());
                            for (j, row) in b.chunks(16).enumerate() {
                                let hex: Vec<String> = row.iter().map(|x| format!("{x:02X}")).collect();
                                let asc: String = row.iter().map(|&x| if (0x20..0x7F).contains(&x) { x as char } else { '.' }).collect();
                                println!("  {:04X}  {:<48} {asc}", j * 16, hex.join(" "));
                            }
                        }
                    }
                    Err(e) => println!("  data unreadable: {e}"),
                }
                if node.bid_sub != 0 {
                    match mb.pst_mut().subnodes(node.bid_sub) {
                        Ok(subs) => {
                            let mut keys: Vec<_> = subs.keys().copied().collect();
                            keys.sort_unstable();
                            println!("subnodes: {}", keys.iter().map(|k| format!("0x{k:X}")).collect::<Vec<_>>().join(", "));
                        }
                        Err(e) => println!("  subnodes unreadable: {e}"),
                    }
                }
                return;
            }
            let props = mb.node_properties(nid).unwrap_or_else(|e| die(&e, 1));
            if json {
                print_json(&props);
                return;
            }
            for p in &props {
                println!(
                    "  0x{:04X}  {:<16} {:<40} {}",
                    p.tag,
                    p.type_name,
                    p.name.as_deref().unwrap_or(""),
                    trim(&p.value, 80)
                );
            }
        }
        "search" => {
            let file = args.positional.first().unwrap_or_else(|| die(USAGE, 2));
            let text = args.positional.get(1).cloned().unwrap_or_default();
            let mut mb = open(file, salvage, quiet);
            let any = [
                "--subject",
                "--sender",
                "--body",
                "--recipients",
                "--attachments",
            ]
            .iter()
            .any(|f| args.flag(f));
            let q = SearchQuery {
                text,
                in_subject: !any || args.flag("--subject"),
                in_sender: !any || args.flag("--sender"),
                in_body: !any || args.flag("--body"),
                in_recipients: !any || args.flag("--recipients"),
                in_attachment_names: args.flag("--attachments"),
                folder: args.value("--folder").map(|f| folder_arg(&mb, f)),
                limit: args.value("--limit").and_then(|v| v.parse().ok()),
                ..Default::default()
            };
            let mut on = |done: u64, total: u64| {
                if !quiet && done.is_multiple_of(500) {
                    eprint!("\r  searched {done} of {total}   ");
                }
                true
            };
            let r = mb.search(&q, &mut on);
            if !quiet {
                eprintln!();
            }
            if json {
                print_json(&r);
                return;
            }
            let names = mb.folder_names();
            for h in &r.hits {
                let m = &h.summary;
                println!(
                    "0x{:<8X}  {:<19}  {:<20}  {:<20}  {:<10}  {}",
                    m.nid,
                    date(m.date_ms),
                    trim(names.get(&m.folder).map(String::as_str).unwrap_or(""), 20),
                    trim(&m.sender_name, 20),
                    h.matched,
                    m.subject
                );
            }
            println!(
                "\n{} hit(s) in {} message(s), {} ms{}.",
                r.hits.len(),
                r.scanned,
                r.elapsed_ms,
                if r.truncated { " (limit reached)" } else { "" }
            );
        }
        "export" => {
            let file = args
                .positional
                .first()
                .unwrap_or_else(|| die(USAGE, 2))
                .clone();
            let dir = args
                .positional
                .get(1)
                .unwrap_or_else(|| die(USAGE, 2))
                .clone();
            let format = match args.value("--format") {
                Some(f) => ExportFormat::parse(f).unwrap_or_else(|| {
                    die(
                        "--format takes eml, msg, mbox, html, txt, csv, vcf, ics or json",
                        2,
                    )
                }),
                None => ExportFormat::Eml,
            };
            let folders = args.all("--folder");
            let messages = args.all("--message");
            let selection = if folders.is_empty() && messages.is_empty() {
                Selection {
                    entire_mailbox: true,
                    ..Default::default()
                }
            } else {
                let mb = open(&file, salvage, quiet);
                Selection {
                    entire_mailbox: false,
                    folders: folders.iter().map(|f| folder_arg(&mb, f)).collect(),
                    messages: messages
                        .iter()
                        .map(|m| {
                            u32::from_str_radix(m.trim_start_matches("0x"), 16)
                                .unwrap_or_else(|_| die("--message takes a node id in hex", 2))
                        })
                        .collect(),
                }
            };
            let opts = ExportOptions {
                source: file,
                destination: dir,
                format,
                selection,
                salvage,
                collision,
                extract_attachments: args.flag("--extract-attachments"),
            };
            let r = run_export("cli", &opts, &mut progress(quiet));
            finish_job(&args, r);
        }
        "convert" | "repair" => {
            let file = args
                .positional
                .first()
                .unwrap_or_else(|| die(USAGE, 2))
                .clone();
            let out = args
                .positional
                .get(1)
                .unwrap_or_else(|| die(USAGE, 2))
                .clone();
            let opts = ConvertOptions {
                source: file,
                destination: out,
                salvage,
                collision,
                validate: !args.flag("--no-validate"),
                write_allocation_maps: !args.flag("--no-maps"),
            };
            let kind = if args.command == "convert" {
                JobKind::Convert
            } else {
                JobKind::Repair
            };
            let r = run_convert("cli", kind, &opts, &mut progress(quiet));
            finish_job(&args, r);
        }
        "verify" => {
            let file = args
                .positional
                .first()
                .unwrap_or_else(|| die(USAGE, 2))
                .clone();
            let r = run_verify("cli", &file, &mut progress(quiet));
            finish_job(&args, r);
        }
        other => {
            // Upstream-style invocation, `ostpst file.pst --tree`, is accepted too.
            if Path::new(other).exists() {
                let p = PathBuf::from(other);
                die(
                    &format!(
                        "put the command first: ostpst info \"{}\"\n\n{USAGE}",
                        p.display()
                    ),
                    2,
                );
            }
            die(&format!("unknown command {other}\n\n{USAGE}"), 2);
        }
    }
}
