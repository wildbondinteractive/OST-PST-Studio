# Competitor feature matrix

Compared: **pstfree 0.5.1** (upstream), **OST/PST Studio** (this project), and **Stellar
Converter for OST** (Corporate $79, Technician $149, Toolkit $199, as advertised on
stellarinfo.com on 2026-10-14). Stellar's claims are taken from its product page and are
not independently verified here; our own statuses are graded honestly.

Status key for OST/PST Studio:

- **Tested**: implemented, covered by automated tests or the libpff comparison.
- **Implemented, not independently verified**: works in this engine; no external check.
- **Partial**: some of the capability.
- **Planned**: designed for, not built.
- **Unsupported**: out of scope.

| Capability | pstfree | OST/PST Studio | Stellar (edition) |
|---|---|---|---|
| OST to PST conversion (2013+ 4K OST) | Yes | **Tested** (libpff: identical id for id) | Yes |
| Pre-2013 OST to PST | Yes (untested, no sample) | Implemented, not independently verified (no sample) | Yes |
| ANSI PST to Unicode | Yes (synthetic fixture) | Tested against a synthetic fixture only | Yes |
| Orphaned OST (no Exchange) | Yes | **Tested** | Yes |
| "Encrypted" OST | Keyless encodings only | Keyless encodings only; WIP-protected files unsupported | Claimed |
| Password-marked PST | Opens, never asks | **Tested** | Password recovery (Toolkit) |
| Valid allocation maps (no SCANPST) | No | **Implemented, not independently verified in Outlook**; validated by engine, libpff reads result | Yes (implied) |
| PST repair / rebuild from damaged index | Yes | **Tested** (torn roots, truncation, fuzz) | Toolkit only |
| Block carving with no index | Yes | **Tested** | Unknown |
| Deleted item recovery | Via sweep (`--salvage`) | Partial: salvage mode surfaces swept nodes; not labelled per item yet | Toolkit only |
| Mailbox preview, three-pane | Win32 window, text only | **Implemented**: folders, virtualised list, sandboxed HTML, attachments, properties | Yes |
| Sorting by date, from, subject, size | Partial | **Implemented** | Yes (plus type, attachment, importance) |
| Search by text (subject, sender, body) | Yes | **Tested** | Yes |
| Search filters (date, size, type, attachments, importance, recipients) | No | **Implemented** | Yes |
| Saved search presets | No | Planned | Unknown |
| Export EML | Yes | **Tested** (upstream tests) | Yes |
| Export MSG | Yes (never opened in Outlook) | Implemented, not independently verified | Yes |
| Export MBOX | Yes | **Tested** | No |
| Export HTML | No | Implemented (sandboxed, CSP) | Yes |
| Export plain text | No | Implemented | RTF |
| Export PDF | No | Planned | Yes |
| Export CSV (contacts) | No | Implemented | Technician |
| Export vCard | No | Implemented (vCard 3.0) | Unknown |
| Export iCalendar | No | Implemented (UTC times, all-day events) | Unknown |
| Export JSON (technical) | No | Implemented | No |
| Export selection to a new PST | No | Planned (needs selective folder table rebuild) | Yes |
| Attachment preview, extraction, hashes | No | **Implemented** (images, text; SHA-256; bulk extract) | Preview |
| Embedded message attachments | Preserved in convert; not shown | Implemented in preview (properties and body) | Yes |
| Batch conversion | No | **Implemented** (queue, concurrency, per-file reports, retry) | Technician |
| PST splitting by size / date / folder | No | Planned | Corporate (auto-split) |
| Save and reload scan | No | Planned (cache keyed on file identity) | Yes (DAT) |
| Contacts browsing | Generic properties | Implemented (contact fields decoded) | Yes |
| Calendar browsing | Generic properties | Implemented (start, end, location, status) | Yes |
| Tasks, notes | Generic properties | Partial (tasks decoded; notes as text) | Yes |
| Validation of written PST | Reopen only | **Tested**: reopen, counts, essentials, every message read; libpff script | None advertised |
| Independent verification (libpff) | Bake-off script | `tests/libpff_compare.py`, run on every fixture | None |
| Outlook compatibility statement | SCANPST required | Reported per job; manual Outlook check script | "Supports Outlook 2021 and earlier" |
| Job reports (JSON/HTML/text) | Console text | **Implemented** | Log file |
| Progress and cancellation | Progress, no cancel | **Implemented** (phases, ETA, cancel at safe points) | Yes |
| Office 365 / Exchange export | No | Planned (Phase 4; supported Graph import only) | Technician |
| Auto-locate Outlook data files | No | Planned | Yes |
| Command line / automation | Yes | **Implemented** (`ostpst`, `--json`) | No |
| Dark mode, high DPI, keyboard navigation | Partial | Implemented | Unknown |
| Privacy: offline, no telemetry | Yes | Yes, by design | Activation online |
| Licence | MIT, free | MIT, free, source available | Proprietary, per-seat, trial saves nothing |

## Ease of use

Stellar's flow is select, scan, preview, save. OST/PST Studio's conversion wizard follows
the same four steps (source, analyse, options, run) and adds a validated report at the
end. The explorer is usable before, during and after a conversion, and every job can be
cancelled without leaving a half-written file behind.

## What Stellar does that this project does not yet

Selective export to PST, PST splitting, saved scan files, PDF export, Office 365 and
Exchange export, and auto-locating profile OST files. All are on the roadmap in
`docs/IMPLEMENTATION_STATUS.md`; none is blocked on anything but the work, except
selective PST export, which needs folder tables rebuilt for the subset.
