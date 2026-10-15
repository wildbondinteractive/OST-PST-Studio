# OST/PST Studio

Open, browse, search, convert, repair and export Outlook **OST** and **PST** files on
Windows. No Outlook, no Exchange connection, no upload, no licence. MIT.

Built on the [pstfree](https://github.com/sp00nznet/pstfree) engine (MIT), with a new
desktop application (Tauri 2, React, TypeScript), a job system with real progress and
cancellation, validated output with allocation maps, independent verification through
libpff, and a technician command line.

![OST/PST Studio](docs/screenshots/explorer.png)

## What it does

- **Opens** Unicode PST, Outlook 2013+ OST (4K pages, compressed blocks), pre-2013 OST
  and ANSI PST (Outlook 97-2002). "Password-protected" PSTs open without a password,
  because a PST password is a checksum, not a key.
- **Recovers** files whose index is gone: sweeps surviving B-tree pages, carves blocks
  with no index at all, and names what it could not confirm.
- **Browses** a three-pane explorer: folder tree, virtualised message list, sandboxed
  HTML preview, attachments with previews and hashes, every raw MAPI property.
- **Searches** across the mailbox by text in subject, sender, recipients, body and
  attachment names, with date, size, type, importance and attachment filters.
- **Converts** OST to a Unicode PST, and ANSI PST to Unicode, then reopens and
  validates the result and writes valid allocation maps so Outlook opens it directly.
- **Repairs** a damaged PST into a new, clean file. The original is never modified.
- **Exports** whole mailboxes, folders or selected messages as EML, MSG, MBOX, HTML,
  text, CSV, vCard, iCalendar or JSON, with attachment extraction.
- **Batches** many files with per-file progress and reports, bounded concurrency,
  cancellation and retry.
- **Reports** every job as JSON, HTML or text: counts, corruption, recovery actions,
  validation checks, and exactly how far Outlook compatibility was established.

## Install

Download the installer or portable build from the
[Releases page](https://github.com/wildbondinteractive/OST-PST-Studio/releases), or
build it yourself (below). Requirements: Windows 10/11 x64 and the WebView2 runtime
(present on all current Windows installations; the installer fetches it if missing).

| File | Use |
|---|---|
| `OST-PST Studio_<version>_x64-setup.exe` | Per-user installer, no administrator rights needed |
| `OST-PST Studio_<version>_x64_en-US.msi` | MSI for managed deployment |
| `OST-PST-Studio_<version>_x64-portable.exe` | Runs from anywhere, nothing installed |
| `ostpst_<version>_x64.exe` | Command-line tool, no dependencies |

**Uninstall:** Settings, Apps, Installed apps, "OST-PST Studio", Uninstall (the MSI also
appears under Programs and Features). The portable build is removed by deleting the
file. Preferences, history and the scan cache can be cleared from the application's
Settings page or by deleting `%APPDATA%\com.wildbondinteractive.ostpststudio` and
`%LOCALAPPDATA%\com.wildbondinteractive.ostpststudio`.

**Releases are not code-signed yet.** Windows SmartScreen warns on first run; "More
info", "Run anyway". See the code signing policy below.

## Build

```powershell
# Rust (stable, MSVC), Node 20+, and the Visual Studio C++ build tools.
npm install
.\scripts\fetch-fixtures.ps1   # public test fixtures, for the tests
cargo test --workspace
npm run tauri build            # installer + exe in target\release\bundle
cargo build --release -p ostpst-cli   # target\release\ostpst.exe
```

The same steps run on a GitHub-hosted Windows runner for every push
(`.github/workflows/ci.yml`), and the installers it produces are attached to each run as
artifacts, so what is published can be compared with what the committed source builds.
Releases are built by `.github/workflows/release.yml` from a `v<version>` tag. See
`docs/RELEASING.md`.

## Command line

```
ostpst info mailbox.ost
ostpst tree mailbox.ost
ostpst list mailbox.ost --folder Inbox --limit 50
ostpst search mailbox.ost invoice --body
ostpst convert mailbox.ost out.pst --report out-report.json
ostpst repair broken.pst fixed.pst --salvage
ostpst export mailbox.ost .\mail --format eml --folder Inbox
ostpst verify mailbox.pst
```

Add `--json` to any command for machine-readable output.

## Honest limits

- Outlook compatibility has been confirmed by opening converted and repaired fixtures in
  classic Outlook (they open directly, no SCANPST), but only on the two public fixtures.
  `scripts/outlook-check.ps1` is the manual check for your own files. See
  `docs/OUTLOOK_COMPATIBILITY.md`.
- Where FMap and FPMap pages recur in files over 125 MB and 2 GB is derived from the
  specification's figures, not observed in a file Outlook wrote.
- ANSI PST support has only been tested against a fixture written from the specification.
- Exporting a *selection* to a new PST is planned; convert and repair write the whole
  mailbox.

Full status: `docs/IMPLEMENTATION_STATUS.md`. Test results: `docs/TEST_RESULTS.md`.

## Privacy

Mailboxes are processed entirely on your computer; the application makes no network
connections and collects no telemetry. See [docs/PRIVACY_POLICY.md](docs/PRIVACY_POLICY.md).

## Code signing policy

No release is code-signed yet. An application to the
[SignPath Foundation](https://signpath.org) free code-signing programme for open-source
projects is pending; until a release page says it was signed through that programme,
every published binary is unsigned and is described as such. Releases are built only by
the GitHub Actions workflow in this repository from a tagged commit, and signing, once
available, will be approved by hand for each release.

The full policy, including who may commit, review and approve signing, is
[docs/CODE_SIGNING_POLICY.md](docs/CODE_SIGNING_POLICY.md).

## Licence

MIT. See [LICENSE](LICENSE). The engine is derived from
[pstfree](https://github.com/sp00nznet/pstfree), copyright pstfree contributors, MIT.
Every open-source component compiled into the application, with its licence, is listed
in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and summarised on the application's
Settings page.
