# Test results

Last run 2026-10-14 on Windows 11 Pro 10.0.26300, Rust 1.99.0 (MSVC), Node 24.18,
WebView2 155. Fixtures: `dist-list.pst`, `passworded.pst`, `example-2013.ost` from
hrbrmstr/freepst (fetched by `scripts/fetch-fixtures.ps1`), plus a synthetic ANSI PST the
tests write themselves and damaged copies the tests make from the fixtures.

Statuses: **PASS**, **FAIL**, **UNTESTED** (not exercised), **MANUAL** (a person has to do
it; recorded when done).

## Automated: engine (`cargo test --workspace`)

| Suite | Count | Result |
|---|---|---|
| `ostpst-core` unit tests (upstream 60 + new 18) | 78 | PASS |
| `ostpst-core` ANSI PST fixture tests | 7 | PASS |
| `ostpst-core` fuzz (8 rounds per fixture, sector-aligned damage) | 1 | PASS, no panic, no hang |
| `cargo clippy --workspace --all-targets -- -D warnings` | | PASS (clean) |
| `cargo fmt --all --check` | | PASS |

New tests beyond upstream: `fsutil` (name sanitisation, collision policy, temp-file
commit, sizes), `mailbox` (open PST and OST, folder tree and kinds, listing from the
contents table, message detail, search by subject and body, filter-only search, search
cancellation, torn file opens as recovered, salvage mode labels swept deleted items as
recovered), `cache` (entry reused only for the identical file), `ostfix` (folder roles
resolve by class and name in the OST), `amap` (map recurrence arithmetic; a
rebuilt PST gets valid maps that cover every block and page, a valid DList, and reopens
with no warnings), `exporters` (CSV quoting, iCalendar times and folding, HTML export
sandboxing).

## Automated: independent verification (`tests/libpff_compare.py`)

| Case | libpff on source | libpff on output | Result |
|---|---|---|---|
| `example-2013.ost` converted with allocation maps (CLI) | 46 folders, 3 messages, 202 properties | 46 / 3 / 202 | PASS, identical id for id |
| `example-2013.ost` converted with allocation maps (GUI) | same | same | PASS |
| `dist-list.pst` repaired with allocation maps | 24 folders, 4 messages, 287 properties | 24 / 4 / 287 | PASS |

## Engine validation of written files (`validate.rs`, 11 checks)

Both conversions above and the repair: all 11 checks PASS (reopen, format, index
source, essential nodes, no structural warnings, allocation maps valid, node, folder and
message counts, every folder readable, every message readable).

## Integration (CLI)

| Scenario | Result |
|---|---|
| `ostpst info` / `tree` / `list` / `show` / `props` / `search` on all three fixtures | PASS |
| `ostpst convert example-2013.ost out.pst` with report JSON | PASS |
| `ostpst repair dist-list.pst out.pst` | PASS |
| `ostpst verify dist-list.pst` (checksums, sweep, carve, table cross-check) | PASS, "the two agree", 0 disagreements |
| Open a PST with both B-tree roots zeroed (torn) | PASS, opens as Recovered, 4 messages, 29 stale nodes listed |
| Open the OST in salvage mode | PASS, 9 messages (3 live, 6 swept) with the swept ones labelled recovered |
| Export whole mailbox as EML (CLI and GUI) | PASS, 3 files in mirrored folders |
| Export MSG / MBOX / HTML / TXT / CSV / JSON | PASS (files written; MSG not opened in Outlook) |
| Export VCF from dist-list.pst Contacts | PASS (1 contact written, distribution list skipped and counted) |
| Export ICS from dist-list.pst Calendar | PASS (1 event) |
| Cancellation mid-job | UNTESTED from the GUI on these fixtures (jobs finish in under a second); the cancel path is exercised by the search cancellation unit test and the `Progress` contract |
| Collision policies (fail, rename, overwrite) | PASS (unit test) |
| Interrupted write leaves only a `.partial` | PASS by construction (unit test of commit); not fault-injected |

## GUI (`scripts/cdp.py` driving the debug build; screenshots in `docs/screenshots`)

| Check | Result |
|---|---|
| Dashboard renders, quick actions navigate | PASS |
| Open OST via store, explorer shows 46 folders with kinds and counts | PASS |
| Message list (two-line rows, sort chips, virtualised) | PASS |
| Message preview: headers, sandboxed body, Properties tab count | PASS |
| Properties tab contents, attachment dialog | UNTESTED by harness (Radix tabs need pointer events; no fixture has a file attachment) |
| Search "test" across the mailbox: 3 hits with matched field | PASS |
| Search filters panel renders | PASS |
| Convert wizard: source probe, analyse, options, run, report with 11 checks, toast | PASS |
| Export page: whole mailbox to EML, report card | PASS |
| Repair page: verify job with notes | PASS |
| Batch page renders | PASS (no batch run from the harness) |
| Activity page lists session jobs | PASS |
| Settings page: theme switch to light and back persists | PASS |
| File health dialog on a healthy and a torn file | PASS |
| Light and dark themes | PASS (screenshots) |
| Drag and drop of files onto the window | UNTESTED (needs a real drag) |
| Native file dialogs | UNTESTED by harness (OS dialogs); paths were typed instead |
| Keyboard shortcuts (Ctrl+1..7, Ctrl+O, Ctrl+F) | UNTESTED by harness |
| High DPI | PASS (screenshots were taken at 150 percent scaling) |

## Packaging

| Artifact | Result |
|---|---|
| `target/release/ostpst-studio.exe` (release, embedded UI) | PASS, launches, loads `tauri://localhost` |
| `target/release/bundle/nsis/OST-PST Studio_0.1.0_x64-setup.exe` (2.6 MB) | Built. Install run: UNTESTED |
| `target/release/bundle/msi/OST-PST Studio_0.1.0_x64_en-US.msi` (3.8 MB) | Built. Install run: UNTESTED |
| `target/release/ostpst.exe` (CLI) | PASS |
| Code signing | Not signed. No certificate was used; Windows SmartScreen will warn. |

## Outlook

| Check | Result |
|---|---|
| Converted PST (first build) attached in classic Outlook | MANUAL, done: attached, folder never loaded. Cause found and fixed (foreign entry ids); see `docs/OUTLOOK_COMPATIBILITY.md` |
| Converted PST with re-pointed links opened in classic Outlook | MANUAL, not yet done. Files for the isolation test are in `fixtures/out/outlook-test/` |
| `.msg` export opened in Outlook | MANUAL, not yet done |

## Not covered by any fixture

Real file attachments, embedded messages, RTF-only bodies, non-English folder names,
files over 125 MB (FMap pages), files over 2 GB (FPMap pages), real ANSI PSTs, cyclic
encoding in a real file, WIP-encrypted files.
