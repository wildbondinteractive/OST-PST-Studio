# Test results

Last run 2026-10-15 on Windows 11 Pro 10.0.26300, Rust 1.99.0 (MSVC), Node 24.18,
WebView2 155. Fixtures: `dist-list.pst`, `passworded.pst`, `example-2013.ost` from
hrbrmstr/freepst (fetched by `scripts/fetch-fixtures.ps1`), plus a synthetic ANSI PST the
tests write themselves and damaged copies the tests make from the fixtures.

Statuses: **PASS**, **FAIL**, **UNTESTED** (not exercised), **MANUAL** (a person has to do
it; recorded when done).

## Automated: engine (`cargo test --workspace`)

| Suite | Count | Result |
|---|---|---|
| `ostpst-core` unit tests (upstream 60 + new 24) | 84 | PASS |
| `ostpst-core` ANSI PST fixture tests | 7 | PASS |
| `ostpst-core` fuzz (8 rounds per fixture, sector-aligned damage) | 1 | PASS, no panic, no hang |
| `ostpst-studio` host tests: updater rules, settings and history (0.1.3) | 12 | PASS |
| `ostpst-studio` updater against a local server, real plugin in a mock app (0.1.3) | 22 | PASS |
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
sandboxing), `heapw` (heap blocks fill and read back, the bitmap-header block keeps its
map, B-trees with an index level round-trip and sort by key, a property context with a
spilled value round-trips, fill levels), `ltp` (4K heap ids split differently).

## Automated: independent verification (`tests/libpff_compare.py`)

| Case | libpff on source | libpff on output | Result |
|---|---|---|---|
| `example-2013.ost` converted with allocation maps (CLI) | 46 folders, 3 messages, 202 properties | 46 / 3 / 202 | PASS, identical id for id |
| `example-2013.ost` converted with allocation maps (GUI) | same | same | PASS |
| `dist-list.pst` repaired with allocation maps | 24 folders, 4 messages, 287 properties | 24 / 4 / 287 | PASS |

## In-app updater (0.1.3)

### Automated, on every CI run (`cargo test -p ostpst-studio`)

The real `tauri-plugin-updater` 2.14 runs inside a mock Tauri app against an HTTP server
on 127.0.0.1, with fixtures signed once by a throwaway key (only its public key is in the
repository). Every case PASS:

| Case | Expected and observed |
|---|---|
| Newer stable release | Found, with version, notes and date; downloaded; verified; ready |
| Same version, older version | Up to date; nothing offered |
| Prerelease (`0.2.0-rc.1`) | Not offered |
| No network (closed port) | Failure kind `network`; attempt recorded, no success recorded |
| Endpoint 404 and 500 | Failure kind `unavailable` |
| Malformed JSON, missing version, missing platforms, invalid version | Failure kind `metadata` |
| Manifest without this installer type | Failure kind `metadata`; not installed from the other type |
| Tampered package | Failure kind `signature`; nothing kept |
| Package signed by another key | Failure kind `signature` |
| Genuine signature for an older version (replay) | Failure kind `signature` (`requireSignedVersion`) |
| Signature that is not base64 | Failure kind `signature` |
| Download returns 503 | Failure kind `network`; the update stays known for a retry |
| Updates off, manual check | No request until asked; then found |
| Background check fails after an update was found | Update still shown; a manual check reports the failure |
| NSIS install / MSI install | Each downloads only its own installer type |
| Portable copy | Told about the update; download refused; no installer requested |
| Queued job, then a file operation | Install refused with the reason; still ready; install-on-close withheld until idle |
| Notify mode | Never installs on close |
| Install before download | Refused |
| Remind me later | Snoozes that version; still visible in Settings |
| Shipped configuration | Stable HTTPS endpoint only, `requireSignedVersion`, passive mode, no insecure flags, a valid minisign public key, no updater artifacts in ordinary builds |
| Settings and history | Old settings files load with the new defaults; update mode survives a restart; recent list capped at ten, keeps the last folder; session paths kept once; missing files detected |

### Real install and update on Windows (`scripts/dev/updater_staging_test.py`)

Run on the development PC on 2026-10-15. Two release builds of the application under a
separate name and identifier ("OST-PST Studio Staging"), a throwaway key, a local server
standing in for GitHub. 26 of 26 checks PASS:

| Step | Result |
|---|---|
| 0.1.3 installed per user by its NSIS installer; window renders; reports NSIS and 0.1.3 | PASS |
| Check finds 0.1.4; the notice and the dialog offer Download and install | PASS |
| Download verified; the NSIS installer only was fetched; dialog offers Restart to update | PASS |
| Install: the app exits, the passive installer replaces it, the new version starts by itself | PASS |
| Installed executable is 0.1.4; the window renders; reports NSIS and 0.1.4; no further update | PASS |
| Exactly one entry in Installed apps afterwards | PASS |
| The executable taken out of the 0.1.3 MSI reports MSI, downloads and verifies the MSI only | PASS (not installed: an MSI install needs an elevation prompt, unattended here) |
| The bare executable reports portable, is told about 0.1.4, refuses to download, requests no installer | PASS |
| Staging installation and its data removed afterwards | PASS |

The first run of this test passed every functional check but its screenshots showed an
empty window: a store selector in the close guard returned a new array on every read,
which React rejects (error 185) in a release build. Fixed; the test now also checks that
the window renders before and after the update.

### Release pipeline additions

| Check | Result |
|---|---|
| `examples/verify_update_signatures.rs` on a set where one signature binds the wrong version | Reports it and exits 1 |
| `release.yml` parses; updater steps: sign after Authenticode verification, verify with the app's public key, manifest for stable tags only, `latest.json` counted in the published set | PASS (YAML); first live run is the v0.1.3 tag |
| Staged packaging with the release configuration after the 0.1.3 changes: bundler order unchanged (executable NSS, uninstaller stub, NSIS installer, executable MSI, MSI) | PASS |
| Release executable: version 0.1.3, Common Controls manifest embedded by the linker, `asInvoker` | PASS |
| `scripts/verify-signatures.ps1` on the published, signed 0.1.2 files | 65 of 65 PASS |

### User interface (release build under a test identifier, `ui_smoke` via CDP)

Dashboard with Recent mailboxes; a fixture mailbox opens and appears in the list; every
Settings tab opens; About names the publisher and the project; the three update modes
and the installed version are shown; F5 refreshes without reloading the window; a
Settings field keeps focus while typing (it lost focus after each character in 0.1.2); no
uncaught errors. 13 of 13 PASS.

### The v0.1.3 release (run 38803667481, commit 319730c)

| Check | Result |
|---|---|
| Release workflow, every step: Azure signing, Authenticode verification (65 of 65), updater signing with the production key, updater verification with the app's key (14 of 14: both signatures bind 0.1.3, `latest.json` parses with the updater's own parser, both URLs name the matching installer), checksums, draft | PASS |
| The draft's files downloaded and verified on the developer PC: Authenticode 65 of 65 with signer CN=Jacob Adlon; updater signatures and manifest 14 of 14; `sha256sum -c SHA256SUMS.txt` by file name | PASS |
| Private key material in the run log | None |
| Before publishing: `releases/latest/download/latest.json` | 404, `latest` still v0.1.2: the draft was not visible to installed copies |
| MANUAL, by the maintainer on a second PC: 0.1.3 installed over 0.1.2 with the setup program; Settings, Software updates shows 0.1.3, "Installed with the setup program (per user)", Notify me; Check for updates after publishing reports "You're running the latest version of OST/PST Studio." | PASS |
| After publishing (2026-10-16): v0.1.3 is Latest; the live `latest.json` announces 0.1.3; both installers it names download (HTTP 200, expected sizes) | PASS |

### Not yet tested

| Item | Status |
|---|---|
| A real update delivered from a published GitHub release | UNTESTED until 0.1.4 is published: 0.1.3 is the first version that can be updated |
| MSI in-place install | UNTESTED end to end (download and verification PASS; the install needs elevation) |

## Real mailbox: 1.57 GB Outlook 2016 OST (not committed, not a fixture)

A cached Exchange mailbox from the developer's own PC: 26,417 messages, 1,088 folders,
33,871 nodes, 343,824 blocks, Unicode 4K layout. Used to find and fix the two 0.1.0
defects below; it is private and is never added to the repository.

| Check | Before (0.1.0) | After (0.1.1) |
|---|---|---|
| Open, index from header | PASS | PASS |
| Inbox listing from the contents table (25,191 rows) | 25,191 rows, 0 with a sender, most with no subject | 25,191 rows, 25,191 with a sender, 25,187 with a subject, 0.44 s |
| Convert to PST (2.8 GB written) | 14.5 s; validation FAIL: 25,288 of 26,417 messages unreadable, 25,803 skipped items | 26 s; all 11 checks PASS: 1,088 of 1,088 folders and 26,417 of 26,417 messages readable, 12,430 attachments; 3 internal nodes of unknown heap type reported |
| Converted PST's Inbox listed from its own contents table | not applicable | 25,191 rows, identical to the source row for row in subject, sender, date and size |
| libpff on the source | libpff fails on the source itself (`unable to retrieve sub message: 25182`), so it cannot judge the output | same |
| Opened in Outlook | MANUAL, not done | MANUAL, not yet done |

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

## Release pipeline (signing)

| Check | Result |
|---|---|
| `.github/workflows/release.yml` and `ci.yml` parse as YAML | PASS |
| Staged packaging (`scripts/dev/staged-packaging-check.ps1`): `tauri build --no-bundle`, then `tauri bundle --bundles nsis,msi` with a logging stand-in as the sign command. The bundler rebuilt nothing (executable hash identical before and after), called the sign command in the order executable (marker NSS), NSIS uninstaller stub, NSIS installer, executable (marker MSI), MSI, and the executable unpacked from each installer carries that installer's marker | PASS, locally, 0.1.2 |
| The bundler also offers its NSIS plug-in DLLs and WiX extension DLLs to the sign command; `scripts/azure-sign.ps1` declines files that are not .exe, .msi or the uninstaller stub | PASS (by inspection of the stand-in's log) |
| `scripts/verify-signatures.ps1` on unsigned local files reports every file as `NotSigned` and exits 1; `-Embedded` unpacks the NSIS installer (7-Zip) and the MSI (`msiexec /a`) and finds `ostpst-studio.exe`; `-ExpectedVersion` reads the version resource | PASS, locally |
| `Invoke-ArtifactSigning` (module ArtifactSigning 0.1.20, the one the official action pins) exposes every parameter `scripts/azure-sign.ps1` passes | PASS, locally (module installed, parameters listed; no signing attempted) |
| Azure Artifact Signing, first live run on tag `v0.1.2` (run 38774878740, commit 221c464): OIDC login over the immutable subject, module install, bundler sign hook (executable with NSS marker, uninstaller stub, NSIS installer, executable with MSI marker, MSI; seven third-party DLLs declined), portable and CLI signed by the official action, 65 of 65 verification checks on the runner | PASS |
| The same five files downloaded from the run on a developer PC: `scripts/verify-signatures.ps1 -Embedded -ExpectedVersion 0.1.2` 65 of 65; signer `CN=Jacob Adlon`, issuer Microsoft ID Verified CS AOC CA 03, root Microsoft Identity Verification Root CA 2020, timestamper Microsoft Public RSA Time Stamping Authority; SHA256SUMS.txt matches every file | PASS |
| Asset names: GitHub replaces spaces with dots, so the two installer names in the 0.1.2 `SHA256SUMS.txt` differ from the downloaded names (hashes still match by content). Installers are named without spaces from the next release | noted, fixed for 0.1.3 |

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
| Converted OST (re-pointed links, maps written) opened in classic Outlook | MANUAL, done: PASS. Attached directly, 24 folders, messages readable, special folders resolve, no SCANPST |
| Repaired PST (maps written) opened in classic Outlook | MANUAL, done: PASS. Attached directly, correct item counts, no SCANPST |
| Converted OST with maps marked invalid | MANUAL, done: Outlook refuses the file ("failed to load for this session"), which is the upstream behaviour the map writer removes |
| `.msg` export opened in Outlook | MANUAL, not yet done |

## Not covered by any fixture

Real file attachments, embedded messages, RTF-only bodies, non-English folder names,
files over 125 MB (FMap pages), files over 2 GB (FPMap pages), real ANSI PSTs, cyclic
encoding in a real file, WIP-encrypted files.
