//! Export a selection of messages in any supported format.
//!
//! EML, MSG and MBOX go through the upstream exporter, which already handles the MIME
//! assembly, the compound file and the mboxrd escaping. The formats added here (HTML,
//! plain text, CSV, vCard, iCalendar, JSON) are built from the same message detail the
//! window shows, so what is exported is exactly what was previewed.
//!
//! The rules the upstream exporter holds to apply here too: nothing is invented (no
//! timezone the file does not record, no character set guessed), and nothing stops (one
//! unreadable message is counted and named, and the rest are written).

use crate::export::{self, Format};
use crate::fsutil::{safe_name, unique_in};
use crate::ltp::{filetime_parts, read_node_pc, Value};
use crate::mailbox::{multi_string, Mailbox};
use crate::model::*;
use crate::props::*;
use crate::report::esc;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct ExportStats {
    pub messages: usize,
    pub attachments: usize,
    pub folders: usize,
    pub files: usize,
    pub failed: usize,
    pub bytes: u64,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

/// Write `messages` from `mb` under `dir` in `opts.format`.
pub fn export_messages(
    mb: &mut Mailbox,
    messages: &[u32],
    dir: &Path,
    opts: &ExportOptions,
    on: crate::Progress,
) -> Result<ExportStats, String> {
    match opts.format {
        ExportFormat::Eml | ExportFormat::Msg | ExportFormat::Mbox => {
            let format = match opts.format {
                ExportFormat::Eml => Format::Eml,
                ExportFormat::Msg => Format::Msg,
                _ => Format::Mbox,
            };
            let nodes = mb.nodes_for(messages);
            let names = mb.folder_names();
            let folders_touched: HashSet<u32> = nodes
                .iter()
                .filter(|n| n.nid_type() == 0x04)
                .map(|n| n.nid_parent)
                .collect();
            let st = export::export(mb.pst_mut(), &nodes, &names, dir, format, on);
            Ok(ExportStats {
                messages: st.messages,
                attachments: st.attachments,
                folders: folders_touched.len(),
                files: if format == Format::Mbox {
                    folders_touched.len()
                } else {
                    st.messages
                },
                failed: st.failed,
                bytes: dir_size(dir),
                errors: st.errors,
                cancelled: st.cancelled,
            })
        }
        ExportFormat::Pst => Err(
            "Exporting a selection to a new PST is not supported yet. Convert or repair writes the whole mailbox; use EML or MSG for a selection."
                .into(),
        ),
        ExportFormat::Csv => export_csv(mb, messages, dir, on),
        ExportFormat::Vcf => export_vcf(mb, messages, dir, on),
        ExportFormat::Ics => export_ics(mb, messages, dir, on),
        ExportFormat::Html | ExportFormat::Txt | ExportFormat::Json => {
            export_per_message(mb, messages, dir, opts, on)
        }
    }
}

fn dir_size(dir: &Path) -> u64 {
    fn walk(p: &Path, acc: &mut u64) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(&path, acc);
                } else if let Ok(m) = e.metadata() {
                    *acc += m.len();
                }
            }
        }
    }
    let mut n = 0;
    walk(dir, &mut n);
    n
}

/// The directory for a folder, mirroring the mailbox tree under `root`.
fn folder_dir(mb: &Mailbox, folder: u32, root: &Path) -> PathBuf {
    let mut chain = Vec::new();
    let mut cur = folder;
    let mut seen = HashSet::new();
    while seen.insert(cur) {
        let Some(f) = mb.folder(cur) else { break };
        if cur != crate::ltp::NID_ROOT_FOLDER {
            chain.push(safe_name(&f.name, cur));
        }
        if f.parent == cur {
            break;
        }
        cur = f.parent;
    }
    chain.reverse();
    chain.iter().fold(root.to_path_buf(), |p, c| p.join(c))
}

fn date_string(ms: Option<i64>) -> String {
    ms.map(crate::report::iso).unwrap_or_default()
}

/// HTML, TXT and JSON: one file per message in a directory per folder, attachments
/// beside it when asked for.
fn export_per_message(
    mb: &mut Mailbox,
    messages: &[u32],
    dir: &Path,
    opts: &ExportOptions,
    on: crate::Progress,
) -> Result<ExportStats, String> {
    let mut st = ExportStats {
        messages: 0,
        attachments: 0,
        folders: 0,
        files: 0,
        failed: 0,
        bytes: 0,
        errors: Vec::new(),
        cancelled: false,
    };
    let mut folders = HashSet::new();
    let total = messages.len() as u64;
    for (i, nid) in messages.iter().enumerate() {
        if !on(i as u64 + 1, total) {
            st.cancelled = true;
            break;
        }
        let detail = match mb.message(*nid) {
            Ok(d) => d,
            Err(e) => {
                st.failed += 1;
                st.errors.push(format!("message 0x{nid:X}: {e}"));
                continue;
            }
        };
        let fdir = folder_dir(mb, detail.summary.folder, dir);
        if let Err(e) = std::fs::create_dir_all(&fdir) {
            st.failed += 1;
            st.errors.push(format!("{}: {e}", fdir.display()));
            continue;
        }
        folders.insert(detail.summary.folder);
        let base = safe_name(&detail.summary.subject, *nid);
        let path = unique_in(&fdir, &base, opts.format.extension());

        // Attachments go in a directory named after the message file, so two messages
        // with the same subject keep their attachments apart.
        let mut attachment_paths: Vec<(String, String)> = Vec::new();
        if opts.extract_attachments && !detail.attachments.is_empty() {
            let adir = path.with_extension("attachments");
            for a in &detail.attachments {
                if a.is_embedded_message {
                    continue;
                }
                match mb.attachment_data(*nid, a.nid) {
                    Ok((info, bytes)) => {
                        if std::fs::create_dir_all(&adir).is_ok() {
                            let (stem, ext) = split_name(&info.name);
                            let ap = unique_in(&adir, &stem, &ext);
                            match std::fs::write(&ap, &bytes) {
                                Ok(()) => {
                                    st.attachments += 1;
                                    st.files += 1;
                                    let rel = format!(
                                        "{}/{}",
                                        adir.file_name().unwrap_or_default().to_string_lossy(),
                                        ap.file_name().unwrap_or_default().to_string_lossy()
                                    );
                                    attachment_paths.push((info.name.clone(), rel));
                                }
                                Err(e) => st.errors.push(format!("{}: {e}", ap.display())),
                            }
                        }
                    }
                    Err(e) => st
                        .errors
                        .push(format!("message 0x{nid:X}, attachment {}: {e}", a.name)),
                }
            }
        }

        let bytes = match opts.format {
            ExportFormat::Html => render_html(&detail, &attachment_paths).into_bytes(),
            ExportFormat::Txt => render_text(&detail).into_bytes(),
            _ => serde_json::to_vec_pretty(&detail).map_err(|e| e.to_string())?,
        };
        match std::fs::write(&path, &bytes) {
            Ok(()) => {
                st.messages += 1;
                st.files += 1;
                st.bytes += bytes.len() as u64;
            }
            Err(e) => {
                st.failed += 1;
                st.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }
    st.folders = folders.len();
    Ok(st)
}

fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && e.len() <= 10 => (s.to_string(), e.to_string()),
        _ => (name.to_string(), "bin".to_string()),
    }
}

fn render_text(d: &MessageDetail) -> String {
    let mut s = String::new();
    s.push_str(&format!("Subject: {}\n", d.summary.subject));
    s.push_str(&format!(
        "From: {}{}\n",
        d.summary.sender_name,
        if d.summary.sender_email.is_empty() {
            String::new()
        } else {
            format!(" <{}>", d.summary.sender_email)
        }
    ));
    for (kind, label) in [(1, "To"), (2, "Cc"), (3, "Bcc")] {
        let list: Vec<String> = d
            .recipients
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| {
                if r.email.is_empty() {
                    r.name.clone()
                } else if r.name.is_empty() {
                    r.email.clone()
                } else {
                    format!("{} <{}>", r.name, r.email)
                }
            })
            .collect();
        if !list.is_empty() {
            s.push_str(&format!("{label}: {}\n", list.join(", ")));
        } else if kind == 1 && !d.summary.display_to.is_empty() {
            s.push_str(&format!("To: {}\n", d.summary.display_to));
        }
    }
    if let Some(t) = d.sent_ms {
        s.push_str(&format!("Sent: {} UTC\n", crate::report::iso(t)));
    }
    if let Some(t) = d.received_ms {
        s.push_str(&format!("Received: {} UTC\n", crate::report::iso(t)));
    }
    if !d.attachments.is_empty() {
        s.push_str(&format!(
            "Attachments: {}\n",
            d.attachments
                .iter()
                .map(|a| format!("{} ({})", a.name, crate::fsutil::human_bytes(a.size)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for (k, v) in &d.extra {
        s.push_str(&format!("{k}: {v}\n"));
    }
    s.push('\n');
    if let Some(b) = &d.body_text {
        s.push_str(b);
    } else if let Some(h) = &d.body_html {
        s.push_str(&crate::html::to_text(h));
    }
    s.push('\n');
    s
}

/// A self-contained HTML page. The message body, when it is HTML, is embedded in a
/// sandboxed iframe via `srcdoc` with scripts and remote content blocked by a CSP, so
/// opening the export in a browser is as safe as previewing it in the application.
fn render_html(d: &MessageDetail, attachments: &[(String, String)]) -> String {
    let mut h = String::new();
    h.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    h.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; frame-src data: about:;\">");
    h.push_str(&format!("<title>{}</title>", esc(&d.summary.subject)));
    h.push_str("<style>body{font:14px/1.5 Segoe UI,system-ui,sans-serif;color:#1b1b1b;background:#fff;max-width:960px;margin:1.5rem auto;padding:0 1rem}h1{font-size:1.3rem;margin:0 0 .5rem}table.h{border-collapse:collapse;margin-bottom:1rem}table.h th{text-align:left;color:#666;font-weight:600;padding:.15rem 1rem .15rem 0;vertical-align:top;white-space:nowrap}table.h td{padding:.15rem 0}iframe{width:100%;min-height:60vh;border:1px solid #ddd;border-radius:6px;background:#fff}pre{white-space:pre-wrap;font:inherit}.att a{display:inline-block;margin:.2rem .5rem .2rem 0;padding:.25rem .5rem;border:1px solid #ddd;border-radius:6px;color:#1b1b1b;text-decoration:none}</style></head><body>");
    h.push_str(&format!(
        "<h1>{}</h1><table class=\"h\">",
        esc(&d.summary.subject)
    ));
    let row = |h: &mut String, k: &str, v: String| {
        if !v.is_empty() {
            h.push_str(&format!("<tr><th>{}</th><td>{}</td></tr>", esc(k), esc(&v)));
        }
    };
    row(
        &mut h,
        "From",
        if d.summary.sender_email.is_empty() {
            d.summary.sender_name.clone()
        } else {
            format!("{} <{}>", d.summary.sender_name, d.summary.sender_email)
        },
    );
    for (kind, label) in [(1, "To"), (2, "Cc"), (3, "Bcc")] {
        let list: Vec<String> = d
            .recipients
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| {
                if r.email.is_empty() {
                    r.name.clone()
                } else {
                    format!("{} <{}>", r.name, r.email)
                }
            })
            .collect();
        let v = if list.is_empty() && kind == 1 {
            d.summary.display_to.clone()
        } else {
            list.join(", ")
        };
        row(&mut h, label, v);
    }
    row(&mut h, "Sent", date_string(d.sent_ms));
    row(&mut h, "Received", date_string(d.received_ms));
    for (k, v) in &d.extra {
        row(&mut h, k, v.clone());
    }
    h.push_str("</table>");
    if !d.attachments.is_empty() {
        h.push_str("<div class=\"att\">");
        for a in &d.attachments {
            let label = format!("{} ({})", a.name, crate::fsutil::human_bytes(a.size));
            match attachments.iter().find(|(n, _)| n == &a.name) {
                Some((_, rel)) => {
                    h.push_str(&format!("<a href=\"{}\">{}</a>", esc(rel), esc(&label)))
                }
                None => h.push_str(&format!("<span>{}</span> ", esc(&label))),
            }
        }
        h.push_str("</div>");
    }
    match (&d.body_html, &d.body_text) {
        (Some(html), _) => {
            // srcdoc with a sandbox: no scripts, no forms, no navigation, and the CSP
            // above stops the frame loading anything remote.
            h.push_str(&format!(
                "<iframe sandbox=\"\" referrerpolicy=\"no-referrer\" srcdoc=\"{}\"></iframe>",
                esc(&format!(
                    "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data: cid:;\">{html}"
                ))
            ));
        }
        (None, Some(text)) => h.push_str(&format!("<pre>{}</pre>", esc(text))),
        _ => h.push_str("<p><em>(no body)</em></p>"),
    }
    h.push_str("</body></html>");
    h
}

/// Every selected item as one CSV row: the listing columns plus contact fields when the
/// item is a contact. Written UTF-8 with a byte-order mark so Excel reads it as such.
fn export_csv(
    mb: &mut Mailbox,
    messages: &[u32],
    dir: &Path,
    on: crate::Progress,
) -> Result<ExportStats, String> {
    let path = unique_in(dir, "export", "csv");
    let mut f = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut st = empty_stats();
    f.write_all(b"\xEF\xBB\xBF").map_err(|e| e.to_string())?;
    let header = [
        "Folder",
        "Type",
        "Subject",
        "From",
        "From Email",
        "To",
        "Date (UTC)",
        "Size",
        "Attachments",
        "Full Name",
        "First Name",
        "Last Name",
        "Company",
        "Job Title",
        "E-mail",
        "E-mail 2",
        "E-mail 3",
        "Business Phone",
        "Home Phone",
        "Mobile",
        "Business Address",
        "Home Address",
    ];
    writeln!(
        f,
        "{}",
        header
            .iter()
            .map(|h| csv_field(h))
            .collect::<Vec<_>>()
            .join(",")
    )
    .map_err(|e| e.to_string())?;
    let total = messages.len() as u64;
    let mut folders = HashSet::new();
    for (i, nid) in messages.iter().enumerate() {
        if !on(i as u64 + 1, total) {
            st.cancelled = true;
            break;
        }
        let d = match mb.message(*nid) {
            Ok(d) => d,
            Err(e) => {
                st.failed += 1;
                st.errors.push(format!("message 0x{nid:X}: {e}"));
                continue;
            }
        };
        folders.insert(d.summary.folder);
        let extra = |k: &str| {
            d.extra
                .iter()
                .find(|(a, _)| a == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let to = if d.recipients.is_empty() {
            d.summary.display_to.clone()
        } else {
            d.recipients
                .iter()
                .filter(|r| r.kind == 1)
                .map(|r| {
                    if r.email.is_empty() {
                        r.name.clone()
                    } else {
                        r.email.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        let fields = [
            mb.folder(d.summary.folder)
                .map(|f| f.path.clone())
                .unwrap_or_default(),
            format!("{:?}", d.summary.item_type),
            d.summary.subject.clone(),
            d.summary.sender_name.clone(),
            d.summary.sender_email.clone(),
            to,
            date_string(d.summary.date_ms),
            d.summary.size.to_string(),
            d.attachments.len().to_string(),
            extra("Full name"),
            extra("First name"),
            extra("Last name"),
            extra("Company"),
            extra("Job title"),
            extra("E-mail"),
            extra("E-mail 2"),
            extra("E-mail 3"),
            extra("Business phone"),
            extra("Home phone"),
            extra("Mobile"),
            extra("Business address"),
            extra("Home address"),
        ];
        writeln!(
            f,
            "{}",
            fields
                .iter()
                .map(|x| csv_field(x))
                .collect::<Vec<_>>()
                .join(",")
        )
        .map_err(|e| e.to_string())?;
        st.messages += 1;
    }
    f.flush().map_err(|e| e.to_string())?;
    st.files = 1;
    st.folders = folders.len();
    st.bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Ok(st)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn empty_stats() -> ExportStats {
    ExportStats {
        messages: 0,
        attachments: 0,
        folders: 0,
        files: 0,
        failed: 0,
        bytes: 0,
        errors: Vec::new(),
        cancelled: false,
    }
}

/// vCard 3.0, one file holding every contact in the selection. Items that are not
/// contacts are skipped and counted rather than written as empty cards.
fn export_vcf(
    mb: &mut Mailbox,
    messages: &[u32],
    dir: &Path,
    on: crate::Progress,
) -> Result<ExportStats, String> {
    let path = unique_in(dir, "contacts", "vcf");
    let mut out = String::new();
    let mut st = empty_stats();
    let total = messages.len() as u64;
    let mut skipped = 0;
    for (i, nid) in messages.iter().enumerate() {
        if !on(i as u64 + 1, total) {
            st.cancelled = true;
            break;
        }
        let d = match mb.message(*nid) {
            Ok(d) => d,
            Err(e) => {
                st.failed += 1;
                st.errors.push(format!("message 0x{nid:X}: {e}"));
                continue;
            }
        };
        if d.summary.item_type != ItemType::Contact {
            skipped += 1;
            continue;
        }
        let extra = |k: &str| {
            d.extra
                .iter()
                .find(|(a, _)| a == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        out.push_str("BEGIN:VCARD\r\nVERSION:3.0\r\n");
        let full = extra("Full name");
        out.push_str(&format!(
            "FN:{}\r\n",
            vcf_esc(if full.is_empty() {
                &d.summary.subject
            } else {
                &full
            })
        ));
        out.push_str(&format!(
            "N:{};{};{};;\r\n",
            vcf_esc(&extra("Last name")),
            vcf_esc(&extra("First name")),
            vcf_esc(&extra("Middle name"))
        ));
        for (k, line) in [
            ("Company", "ORG"),
            ("Job title", "TITLE"),
            ("Nickname", "NICKNAME"),
            ("Web page", "URL"),
        ] {
            let v = extra(k);
            if !v.is_empty() {
                out.push_str(&format!("{line}:{}\r\n", vcf_esc(&v)));
            }
        }
        for (k, kind) in [
            ("E-mail", "INTERNET,PREF"),
            ("E-mail 2", "INTERNET"),
            ("E-mail 3", "INTERNET"),
        ] {
            let v = extra(k);
            if !v.is_empty() {
                out.push_str(&format!("EMAIL;TYPE={kind}:{}\r\n", vcf_esc(&v)));
            }
        }
        for (k, kind) in [
            ("Business phone", "WORK,VOICE"),
            ("Home phone", "HOME,VOICE"),
            ("Mobile", "CELL"),
            ("Business fax", "WORK,FAX"),
        ] {
            let v = extra(k);
            if !v.is_empty() {
                out.push_str(&format!("TEL;TYPE={kind}:{}\r\n", vcf_esc(&v)));
            }
        }
        for (k, kind) in [("Business address", "WORK"), ("Home address", "HOME")] {
            let v = extra(k);
            if !v.is_empty() {
                out.push_str(&format!("ADR;TYPE={kind}:;;{}\r\n", vcf_esc(&v)));
            }
        }
        let bday = extra("Birthday");
        if bday.len() >= 10 {
            out.push_str(&format!("BDAY:{}\r\n", bday[..10].replace('-', "")));
        }
        if let Some(t) = &d.body_text {
            if !t.trim().is_empty() {
                out.push_str(&format!("NOTE:{}\r\n", vcf_esc(t.trim())));
            }
        }
        out.push_str("END:VCARD\r\n");
        st.messages += 1;
    }
    if skipped > 0 {
        st.errors.push(format!(
            "{skipped} selected item(s) are not contacts and were not written"
        ));
    }
    std::fs::write(&path, out.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
    st.files = 1;
    st.bytes = out.len() as u64;
    Ok(st)
}

fn vcf_esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// iCalendar, one file holding every appointment in the selection.
///
/// Times are written as UTC (`Z`), because `PidLidAppointmentStartWhole` and
/// `PidLidAppointmentEndWhole` are stored in UTC. No time zone the file does not record
/// is claimed. All-day events use `VALUE=DATE`.
fn export_ics(
    mb: &mut Mailbox,
    messages: &[u32],
    dir: &Path,
    on: crate::Progress,
) -> Result<ExportStats, String> {
    let path = unique_in(dir, "calendar", "ics");
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//OST/PST Studio//EN\r\nCALSCALE:GREGORIAN\r\n",
    );
    let mut st = empty_stats();
    let total = messages.len() as u64;
    let mut skipped = 0;
    let names = mb.names();
    // Resolve the appointment ids once; they are per file.
    let find = |set: &str, lid: u32| -> Option<u16> {
        let want = format!("{set} 0x{lid:04X}");
        (0x8000u32..=0xFFFF)
            .map(|i| i as u16)
            .find(|id| names.get(*id) == Some(want.as_str()))
    };
    let (id_start, id_end, id_loc, id_allday) = (
        find("PSETID_Appointment", LID_APPT_START_WHOLE),
        find("PSETID_Appointment", LID_APPT_END_WHOLE),
        find("PSETID_Appointment", LID_LOCATION),
        find("PSETID_Appointment", LID_APPT_ALL_DAY),
    );
    let id_cats = (0x8000u32..=0xFFFF)
        .map(|i| i as u16)
        .find(|id| names.get(*id) == Some("PS_PUBLIC_STRINGS \"Keywords\""));

    for (i, nid) in messages.iter().enumerate() {
        if !on(i as u64 + 1, total) {
            st.cancelled = true;
            break;
        }
        let node = match mb.nodes().iter().find(|n| n.nid == *nid) {
            Some(n) => *n,
            None => continue,
        };
        let pc = match read_node_pc(mb.pst_mut(), &node) {
            Ok(pc) => pc,
            Err(e) => {
                st.failed += 1;
                st.errors.push(format!("message 0x{nid:X}: {e}"));
                continue;
            }
        };
        let class = pc.str(PID_MESSAGE_CLASS).unwrap_or("");
        if !matches!(
            crate::mailbox::item_type(class),
            ItemType::Appointment | ItemType::MeetingRequest
        ) {
            skipped += 1;
            continue;
        }
        let time = |id: Option<u16>| id.and_then(|i| pc.time(i)).filter(|t| *t != 0);
        let (Some(start), Some(end)) = (time(id_start), time(id_end)) else {
            skipped += 1;
            st.errors.push(format!(
                "appointment 0x{nid:X} has no start or end time and was not written"
            ));
            continue;
        };
        let all_day = id_allday
            .and_then(|i| pc.int(i))
            .map(|v| v != 0)
            .unwrap_or(false);
        out.push_str("BEGIN:VEVENT\r\n");
        let uid = pc
            .str(crate::ltp::PID_INTERNET_MSG_ID)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{nid:X}@ostpst.local"));
        out.push_str(&format!("UID:{}\r\n", ics_esc(&uid)));
        out.push_str(&format!(
            "DTSTAMP:{}\r\n",
            ics_utc(crate::model::ms_to_filetime(now_ms()))
        ));
        if all_day {
            out.push_str(&format!("DTSTART;VALUE=DATE:{}\r\n", ics_date(start)));
            out.push_str(&format!("DTEND;VALUE=DATE:{}\r\n", ics_date(end)));
        } else {
            out.push_str(&format!("DTSTART:{}\r\n", ics_utc(start)));
            out.push_str(&format!("DTEND:{}\r\n", ics_utc(end)));
        }
        out.push_str(&format!(
            "SUMMARY:{}\r\n",
            ics_esc(crate::ltp::clean_subject(
                pc.str(crate::ltp::PID_SUBJECT).unwrap_or("")
            ))
        ));
        if let Some(loc) = id_loc
            .and_then(|i| pc.str(i))
            .filter(|s| !s.trim().is_empty())
        {
            out.push_str(&format!("LOCATION:{}\r\n", ics_esc(loc)));
        }
        if let Some(Value::Raw {
            ptype: 0x101F,
            bytes,
        }) = id_cats.and_then(|i| pc.props.get(&i))
        {
            let list = multi_string(bytes, true);
            if !list.is_empty() {
                out.push_str(&format!(
                    "CATEGORIES:{}\r\n",
                    list.iter()
                        .map(|c| ics_esc(c))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        }
        if let Some(body) = crate::ltp::body_text(&pc) {
            if !body.trim().is_empty() {
                out.push_str(&fold(&format!("DESCRIPTION:{}", ics_esc(body.trim()))));
            }
        }
        if let Some(t) = pc.time(PID_CREATION_TIME).filter(|t| *t != 0) {
            out.push_str(&format!("CREATED:{}\r\n", ics_utc(t)));
        }
        if let Some(t) = pc.time(PID_LAST_MODIFICATION_TIME).filter(|t| *t != 0) {
            out.push_str(&format!("LAST-MODIFIED:{}\r\n", ics_utc(t)));
        }
        out.push_str("END:VEVENT\r\n");
        st.messages += 1;
    }
    out.push_str("END:VCALENDAR\r\n");
    if skipped > 0 {
        st.errors.push(format!(
            "{skipped} selected item(s) are not appointments (or have no times) and were not written"
        ));
    }
    std::fs::write(&path, out.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
    st.files = 1;
    st.bytes = out.len() as u64;
    Ok(st)
}

fn ics_utc(ft: u64) -> String {
    let (y, m, d, hh, mm, ss) = filetime_parts(ft);
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

fn ics_date(ft: u64) -> String {
    let (y, m, d, ..) = filetime_parts(ft);
    format!("{y:04}{m:02}{d:02}")
}

fn ics_esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// RFC 5545 line folding at 75 octets.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut count = 0;
    for c in line.chars() {
        let w = c.len_utf8();
        if count + w > 74 {
            out.push_str("\r\n ");
            count = 1;
        }
        out.push(c);
        count += w;
    }
    out.push_str("\r\n");
    out
}

/// A JSON document describing the whole mailbox structure, for technical analysis:
/// the file info, every folder, and a summary of every message.
pub fn mailbox_json(mb: &mut Mailbox, on: crate::Progress) -> Result<String, String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        file: FileInfo,
        folders: Vec<FolderInfo>,
        messages: Vec<MessageSummary>,
    }
    let folders = mb.folders();
    let total = folders.len() as u64;
    let mut messages = Vec::new();
    for (i, f) in folders.iter().enumerate() {
        if !on(i as u64 + 1, total) {
            return Err(crate::CANCELLED.into());
        }
        messages.extend(mb.all_summaries(f.nid));
    }
    let doc = Doc {
        file: mb.info().clone(),
        folders,
        messages,
    };
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

#[allow(dead_code)]
fn unused(_: BTreeMap<u16, Value>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_fields_are_quoted_when_needed() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn ics_times_are_utc_and_folded() {
        // 2024-01-01T00:00:00Z
        assert_eq!(ics_utc(133_485_408_000_000_000), "20240101T000000Z");
        assert_eq!(ics_date(133_485_408_000_000_000), "20240101");
        let long = fold(&format!("DESCRIPTION:{}", "x".repeat(200)));
        for l in long.lines() {
            assert!(l.len() <= 75, "{}", l.len());
        }
        assert_eq!(ics_esc("a,b;c\nd"), "a\\,b\\;c\\nd");
    }

    #[test]
    fn html_export_sandboxes_the_body() {
        let d = MessageDetail {
            summary: MessageSummary {
                nid: 1,
                folder: 2,
                subject: "<b>s</b>".into(),
                sender_name: "a".into(),
                sender_email: "a@b".into(),
                display_to: String::new(),
                date_ms: None,
                size: 0,
                has_attachments: false,
                importance: 1,
                read: true,
                item_type: ItemType::Mail,
                message_class: "IPM.Note".into(),
                from_table: false,
            },
            recipients: vec![],
            sender_representing_name: String::new(),
            sender_representing_email: String::new(),
            body_text: None,
            body_html: Some("<script>alert(1)</script><p>hi</p>".into()),
            transport_headers: None,
            internet_message_id: None,
            conversation_topic: None,
            created_ms: None,
            modified_ms: None,
            sent_ms: None,
            received_ms: None,
            creator_name: None,
            last_modifier_name: None,
            sensitivity: 0,
            attachments: vec![],
            properties: vec![],
            extra: vec![],
            problems: vec![],
        };
        let h = render_html(&d, &[]);
        assert!(h.contains("sandbox=\"\""));
        assert!(h.contains("&lt;b&gt;s&lt;/b&gt;"), "subject not escaped");
        assert!(!h.contains("<script>"), "raw script leaked outside srcdoc");
        assert!(h.contains("Content-Security-Policy"));
    }
}
