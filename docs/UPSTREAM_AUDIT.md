# Upstream audit: pstfree 0.5.1

Audit of the `pstfree` codebase (MIT, https://github.com/sp00nznet/pstfree) that OST/PST
Studio is built on. Written before any new code, so that every reuse decision below is
grounded in what the upstream actually does rather than what its README says.

Reference copy audited: `F:\Projects\OSTtoPST\pstfree-og` (read-only). Nothing in that
directory was modified; the build and tests below were run on a copy in a scratch
directory.

## Baseline build and test results

| Step | Result |
|---|---|
| `cargo build --release` (Rust 1.99.0, MSVC) | OK, 5.4 s, two executables |
| `cargo test` (library) | 60 passed, 0 failed |
| `cargo test --test ansi` (synthetic ANSI PST) | 7 passed |
| `cargo test --test fuzz` (8 rounds per fixture) | 1 passed, no panics, no hangs |
| Fixtures | `dist-list.pst` (271 KB), `passworded.pst` (271 KB), `example-2013.ost` (16.8 MB) from hrbrmstr/freepst |

Dependencies: `miniz_oxide` (zlib inflate for 2013+ OST blocks) and `windows-sys`
(Win32 GUI, and `MultiByteToWideChar` for code pages). No build script.

## Code map (9,412 lines)

| Module | Lines | What it does | Reuse decision |
|---|---|---|---|
| `src/ndb.rs` | 1,562 | Header parsing (Unicode 512-byte page, Unicode 4K page OST, ANSI), CRC-32, node and block B-tree walks, block decode (permute, cyclic, zlib), XBLOCK/XXBLOCK chains, subnode trees, page sweep (`scan`), block carving (`carve`), header translation for rebuilds | **Reuse as-is** (core engine). Add a cancellation-aware progress hook and a `block_size_hint` for streaming readers. |
| `src/ltp.rs` | 1,271 | Heap-on-node, BTH, property contexts, table contexts, named property map, recipients, FILETIME formatting, HTML-body-to-text, substring search | **Reuse as-is.** Extend with more property ids (importance, flags, has-attach, contact and calendar fields) and a filter-based search. |
| `src/convert.rs` | 917 | OST (4K page) and ANSI PST to Unicode PST: re-lays every data stream, rebuilds subnode trees, rebuilds the message store with IPM subtree, wastebasket, finder entry ids and a zero password | **Reuse.** Writer needs allocation maps (see defects). Output path should be written via a temp file by the caller. |
| `src/repair.rs` | 665 | Rebuild of a 512-byte-page PST by copying surviving blocks byte for byte, building fresh NBT/BBT B-trees, streaming forward write, map-slot reservation | **Reuse.** Same AMap limitation as convert. |
| `src/export.rs` | 984 | EML (RFC 5322 + MIME), MBOX (mboxrd), MSG (CFBF with MAPI property streams), filename sanitisation, folder directory mirroring | **Reuse.** Needs selection-based export (list of message nids), HTML/text/PDF/CSV/VCF/ICS/JSON formats added beside it. |
| `src/cfbf.rs` | 474 | Compound-file writer for `.msg` | Reuse unchanged. |
| `src/crypt.rs` | 149 | The two keyless ciphers, table validated at test time | Reuse unchanged. |
| `src/html.rs` | 444 | HTML to text, entity decoding, code-page decoding via Win32 | Reuse. The GUI will additionally render sanitised HTML in a sandboxed WebView, which upstream deliberately did not do. |
| `src/bin/pstfree.rs` | 822 | CLI: summary, tree, list, props, find, export, rebuild/convert, verify, nodes, blocks, salvage | **Reuse the logic** (index fallback, verify report, folder naming) as the basis of the new CLI and the GUI backend. |
| `src/bin/pstfree-gui.rs` | 1,484 | Win32 window: tree, list, text pane, search box, jobs on a worker thread | **Not reused.** Replaced by Tauri 2 + React. The worker-thread pattern (reopen the file on the worker, post progress back) is carried over conceptually. |
| `tests/ansi.rs` | 470 | Builds a synthetic ANSI PST from the spec and reads/converts it | Reuse as a regression test. |
| `tests/fuzz.rs` | 147 | Sector-aligned random damage, panic and hang guard | Reuse as a regression test. |
| `tests/bakeoff.py` | | libpff head-to-head on intact and damaged fixtures | Reuse as the independent validation harness. |

## How it currently works

**Reading.** `Pst::open` reads the header, picks a page layout (512 / 4K / ANSI), checks
both header CRCs, truncation and the AMap-valid flag, and records warnings instead of
failing. `nodes()` and `blocks()` walk the two B-trees with loop and depth guards.
`block()` validates trailer id and CRC, decodes the keyless cipher for data blocks and
inflates zlib for 4K-page OST blocks. `node_blocks()` follows XBLOCK/XXBLOCK chains.
`subnodes()` flattens SLBLOCK/SIBLOCK trees.

**Properties.** `ltp::read_pc` reads a property context: heap, BTH, inline values,
heap values, subnode values. `read_tc` reads table contexts (hierarchy, contents,
recipients) with per-row cell-existence bitmaps. Named properties resolve through node
0x61. Narrow strings decode in the message's declared code page.

**Folder tree.** Built from NBT parent pointers (`nid_parent`), with hierarchy tables as
a cross-check in `--verify`. Display names and counts come from each folder's PC.

**Messages.** One PC read per message. Bodies: `PidTagBody`, else `PidTagHtml` with
markup stripped. Attachments enumerated from the subnode tree by NID type 0x05, by-value
attachments only.

**Repair.** `scan()` sweeps every page for NBT/BBT leaves that pass their CRC, settles
duplicates (block entries by on-disk validity, node entries by highest bid, then
bid_sub). `carve()` finds every block by trailer at aligned boundaries with no index.
`repair::rebuild` copies surviving blocks and writes fresh B-trees, marking AMaps
invalid.

**Convert.** `convert::convert` re-lays every node's data stream into 8176-byte blocks,
preserving block boundaries (heaps address allocations by block index), rebuilds
subnode trees, and rebuilds the message store node for OST input.

**Export.** Per message: EML (transport headers reused if present, else synthesised),
MBOX (mboxrd escaping), MSG (CFBF with Unicode property streams). One directory per
folder, names sanitised against traversal, reserved names and length.

**Search.** `ltp::find`: case-insensitive substring over subject, sender and body, one
PC read per message, no index.

**Progress.** `Progress = &mut dyn FnMut(u64, u64)`, called per unit. No cancellation.

## Known defects and limitations (verified, not assumed)

1. **Allocation maps are written invalid** (`fAMapValid = 0`, maps zeroed, slots
   reserved). Upstream states Outlook reports the file as "not closed properly" and
   requires SCANPST. This is the single biggest Outlook-compatibility gap. Confirmed by
   reading `repair.rs` line 354 and `convert.rs` line 616.
2. **DList page (0x4200) is not written.** The density list is optional per MS-PST but
   is part of a cleanly closed file.
3. **FMap and FPMap recurrence is unknown** to upstream; worked around by reserving all
   four slots per AMap section. Safe for data but means those pages are never valid.
4. **No cancellation.** Long jobs cannot be stopped.
5. **Output is written directly to the target path** with `File::create`. An interrupted
   convert leaves a truncated file that looks like a PST. (Repair deletes on error;
   convert does not.)
6. **Attachments only by value.** Embedded messages (`ATTACH_EMBEDDED_MSG`, method 5)
   are preserved structurally by convert/repair but are not exported or displayed.
   Attachment extraction has never been tested against a real attachment because none of
   the public fixtures has one.
7. **Message list requires one PC read per message.** For a 100k-message mailbox the
   contents table (which carries subject, sender, time, size, flags inline) is a much
   cheaper source for listings and should be preferred.
8. **ANSI PST** only tested against a self-written fixture.
9. **Cyclic encoding** only tested against a self-written fixture.
10. **OST sync-state nodes (types 0x14-0x19)** are carried into the PST. Harmless as far
    as known, but unverified in Outlook.
11. **`special_folders` requires English names** ("Root - Mailbox", "IPM_SUBTREE",
    "Finder"). The first two are Exchange constants and fine; "Deleted Items" falls back
    to the lowest-nid subtree child, which is a guess on localised mailboxes.
12. **`--rebuild` of a Unicode PST keeps the source encoding** (blocks copied raw), so
    the header crypt method is preserved. Fine. A converted OST is written `NDB_CRYPT_NONE`.
13. **Search is linear and unindexed**, re-reads every message body for every query.
14. **The .msg writer has never been opened in Outlook** (upstream had no Outlook).
15. **No contact / calendar specific handling**: properties are read generically, but
    nothing presents them or exports CSV/VCF/ICS.
16. **Exporting a selection** of messages is possible only by folder subtree or a
    single message; multi-select is a GUI-only concept.
17. **`MAX_WARNINGS = 200`** caps the report; fine, but the count is kept.
18. **`Pst` is `!Sync` and takes `&mut self`** everywhere, so concurrent browsing and a
    long job on the same mailbox need separate `Pst` handles (upstream GUI reopens the
    file on the worker thread).

## Functions and modules reused verbatim

All of `ndb.rs`, `ltp.rs`, `crypt.rs`, `cfbf.rs`, `html.rs`, `convert.rs`, `repair.rs`,
`export.rs` are carried into `crates/ostpst-core/src/` with the upstream header
attribution kept at the top of each file. Changes are additive and listed in
`docs/IMPLEMENTATION_STATUS.md`.

## Areas requiring architectural change

- A **high-level mailbox API** (`mailbox.rs`) above the raw layers: open with index
  fallback, folder tree, paged message listings from contents tables, message detail,
  attachment access, search with filters. This is what the GUI and CLI both call.
- **Cancellation-aware progress** (`Progress` returns `bool`; `false` cancels).
- **Safe output writes**: temp file plus rename, collision policy, disk-space check.
- **Validation layer**: reopen output, compare node and folder counts against the
  source, check essential nodes, independent check via libpff when available.
- **Reports**: serialisable job report (JSON, HTML, text).
- **Allocation maps**: write valid AMap/PMap/FMap/FPMap pages and set `fAMapValid`.
- **GUI**: Tauri 2 host, job manager with events, React frontend.

## Missing commercial-grade features (compared with Stellar Converter for OST)

See `docs/COMPETITOR_FEATURE_MATRIX.md`. Highlights: rendered HTML preview, advanced
multi-field search, batch queue, selective export by message, additional export formats
(HTML, TXT, PDF, CSV, VCF, ICS), PST splitting, saved scans, attachment management,
contacts and calendar views, reports.

## Recommended implementation sequence

1. Core crate with cancellation, mailbox API, validation, reports.
2. CLI on top of the mailbox API (keeps technician workflow).
3. Tauri host: open, folders, messages, preview, convert job with progress.
4. Frontend shell: navigation, dashboard, explorer, convert wizard, jobs.
5. Export selection and formats; search filters; batch queue.
6. Allocation maps and Outlook compatibility testing (Outlook is installed locally).
7. Contacts, calendar, extra exports, splitting, scan cache.
8. Packaging (NSIS/MSI via Tauri bundler), docs, test results.
