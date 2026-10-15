# Implementation status

Last updated 2026-10-15. This is the hand-off document: what exists, how it is wired,
what is verified, and what is next. Pair it with `docs/TEST_RESULTS.md`.

## Phase 1: architecture and functional foundation (done)

| Item | State | Where |
|---|---|---|
| Upstream audit | Done | `docs/UPSTREAM_AUDIT.md` |
| Independent project, MIT with attribution | Done | `LICENSE`, banners on carried-over files |
| Engine integrated, cancellation added | Done | `crates/ostpst-core/src/{ndb,ltp,convert,repair,export,...}.rs`; `Progress` returns `bool` |
| 4K OST heaps read with their own id layout (13-bit block, 14-bit index) and auxiliary table heaps (subnodes 0x521/0x541, client 0xA5) resolved per column | Done, verified on a 1.57 GB Outlook 2016 OST | `ltp.rs`: `Heap::with_layout`, `node_heap`, `table_heaps`, `column_homes` |
| OST to PST conversion rebuilds every heap for the PST layout instead of copying blocks | Done: PCs re-serialised, TCs re-pointed and row matrices re-packed, values over 3,580 bytes spilled to subnodes, auxiliary heaps folded in | `heapw.rs`; `convert.rs` `Out::node` |
| High-level mailbox API | Done | `mailbox.rs`: open with sweep/carve fallback, folder tree, paged listings from contents tables, message detail, attachments, embedded messages, search with filters |
| Serialisable model | Done | `model.rs`, mirrored in `src/lib/api.ts` |
| Jobs: convert, repair, export, verify | Done | `jobs.rs`; temp file + rename, disk-space check, collision policy |
| Validation of written PST | Done | `validate.rs`, 11 checks |
| Reports (JSON, HTML, text) | Done | `report.rs` |
| Allocation maps (AMap, PMap, FMap, FPMap, DList, header) | Done | `amap.rs`; FMap/FPMap placement derived, see module doc |
| Export formats: EML, MSG, MBOX (upstream) + HTML, TXT, CSV, VCF, ICS, JSON | Done | `exporters.rs` |
| CLI | Done | `crates/ostpst-cli`, `ostpst <command> <file> ...`, `--json` |
| Tauri 2 host | Done | `src-tauri/src/lib.rs` (30 commands), `jobs.rs` (queue, concurrency, cancel, retry, events), `settings.rs` |
| React UI | Done | Dashboard, Explorer (3 panes, virtualised list, sandboxed preview, properties, attachments, search filters, context menu), Convert wizard, Export, Repair (verify + repair), Batch, Activity, Settings |
| Theme: system/light/dark, high DPI, keyboard shortcuts | Done | `index.css`, `App.tsx` |
| Drag and drop of files onto the window | Done | `App.tsx` |
| Packaging: NSIS + MSI via Tauri bundler, app icon, metadata | Done (see TEST_RESULTS for the build outcome) | `src-tauri/tauri.conf.json`, `src-tauri/icons` |
| Independent verification with libpff | Done | `tests/libpff_compare.py` |
| UI test harness | Done | `scripts/cdp.py` drives the running app over the DevTools protocol |

## Phase 2: reliability and professional experience (mostly done)

| Item | State |
|---|---|
| Valid allocation maps so Outlook does not ask for SCANPST | Done and **confirmed in Outlook**: a repaired PST and a converted OST open directly; the same conversion with maps marked invalid is refused |
| OST store and folder links re-pointed at the PST (`ostfix.rs`) | Done and **confirmed in Outlook**: folders load, special folders resolve, messages read |
| Upstream defects addressed | Cancellation, safe writes, TC-based listings, embedded message access, sender fallbacks |
| Advanced search (text in subject/sender/recipients/body/attachment names; date, size, type, importance, attachments, unread) | Done |
| Batch processing with concurrency, per-file reports, retry | Done |
| Export selection (entire, folders with subtree, selected messages) | Done |
| Rich preview (sandboxed HTML, remote content blocked with per-message override, images and text attachments, SHA-256) | Done |
| Activity history and reports | Done |
| Repair and recovery workflows (verify, salvage mode, repair to new file) | Done |
| Regression tests | Upstream suite carried over (68) plus new tests; 86 total |
| Saved search presets | Done (local storage, per machine) |
| Keyboard navigation of the message list (arrows, Home/End, Ctrl+A, Enter) | Done |

## Phase 3: advanced capabilities

| Item | State | Notes |
|---|---|---|
| HTML, TXT, CSV, VCF, ICS, JSON exports | Done | PDF not started (needs a PDF writer; planned as HTML-to-PDF through WebView2 print or a small PDF generator) |
| Contacts and calendar handling | Partial | Fields decoded for preview, CSV, VCF, ICS; no dedicated contacts/calendar grid views yet |
| PST splitting | Not started | Needs a selective writer: folder tables (hierarchy and contents TCs) and counts must be rebuilt for the subset; splitting at node level alone leaves TC rows pointing at missing nids |
| Export selection to a new PST | Not started | Same prerequisite as splitting |
| Save and resume scans | Done | `cache.rs`: nodes, blocks and recovery info as JSON in the app cache dir, keyed on path, size, mtime and both header CRCs; salvage is part of the key; settings to disable, relocate and clear. Restart of a partial PST write is deliberately not offered |
| Cancellation and restart recovery | Cancellation done at every phase; restart of a partial PST write is deliberately not offered (writer has no journal) |
| Forensic features | Partial | Raw properties with names, creator and modifier names, sent-on-behalf-of, transport headers, stale-revision listing. Per-item "recovered" label for messages the index holds but the folder table does not list (salvage mode surfaces deleted items this way). Not done: orphaned-node browsing |
| Large-file performance | Partial | Streaming writes, bounded memory, contents-table listings. Not measured above the 16 MB fixture |
| Installer and portable packaging | Done via Tauri bundler; portable is the bare `ostpst-studio.exe` plus WebView2 runtime | |
| Continuous integration and release builds | Done: `.github/workflows/ci.yml` builds, tests and uploads the installers on a GitHub-hosted Windows runner; `release.yml` turns a `v*` tag into a draft release with checksums | `.github/workflows/` |
| Code signing | Not yet: releases are unsigned. SignPath Foundation application prepared; the workflow's signing job is inactive until the secret and variable exist | `docs/CODE_SIGNING_POLICY.md` |
| Third-party notices | Done: generated from the lock files | `THIRD_PARTY_NOTICES.md`, `scripts/third-party-notices.py` |

## Phase 4: stretch

Microsoft 365 / Graph import, auto-locating profile OST files, PST merge. Not started.
Note for whoever picks this up: Graph has no PST import API; the supported route is the
Office 365 Import Service (network upload or drive shipping), which is an admin workflow,
not something an app should fake with per-message Graph writes.

## Known concerns

1. Outlook compatibility has been confirmed on the two public fixtures only (one OST,
   one PST). A 1.57 GB real OST now converts to a file that validates fully and lists
   identically, but that output has not been opened in Outlook yet. Use
   `scripts/outlook-check.ps1` with a throwaway profile and record results in
   `docs/OUTLOOK_COMPATIBILITY.md`.
8. Which heap a contents-table column lives in (the table's own or an auxiliary one) is
   not stated anywhere in an OST; it is inferred from the data, by checking a sample of
   rows against their messages. In every file seen so far the auxiliary heaps hold
   exactly two columns, 0x0042 and 0x3013. Heaps of client types 0x8C and 0xCC (seen on
   three internal nodes of the real OST) are not understood and are copied as they are.
2. FMap/FPMap placement beyond 125 MB / 2 GB is derived from the specification's
   figures; no Outlook-written file of that size has been examined.
3. OST sync-state nodes (0x14 to 0x19) are carried into converted PSTs as upstream does.
4. ANSI PST is tested against a synthetic fixture only.
5. Attachment extraction has not met a real attachment: none of the public fixtures has
   one. The code paths are unit-tested with synthetic data.
6. The `.msg` writer has never been opened in Outlook.
7. The debug build loads the UI from the Vite dev server (`devUrl`); run `npm run dev`
   or `npm run tauri dev`. The release build embeds `dist/`.

## How to continue

```powershell
.\scripts\fetch-fixtures.ps1
cargo test --workspace
npm install; npm run tauri dev
```

For UI checks without a mouse: launch the app with
`$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"` and use
`python scripts\cdp.py shot out.png` / `eval "..."`. In dev builds `window.__ostpst`
is the zustand store.

Suggested next steps, in order: (1) run the manual Outlook check and record it;
(2) selective PST writer (unlocks splitting and PST export of selections); (3) contacts
and calendar grid views; (4) PDF export; (5) orphaned-node browsing.
