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
| `OST-PST-Studio_<version>_x64-setup.exe` | Per-user installer, no administrator rights needed |
| `OST-PST-Studio_<version>_x64_en-US.msi` | MSI for managed deployment |
| `OST-PST-Studio_<version>_x64-portable.exe` | Runs from anywhere, nothing installed |
| `ostpst_<version>_x64.exe` | Command-line tool, no dependencies |

**Uninstall:** Settings, Apps, Installed apps, "OST-PST Studio", Uninstall (the MSI also
appears under Programs and Features). The portable build is removed by deleting the
file. Preferences, history and the scan cache can be cleared from the application's
Settings page or by deleting `%APPDATA%\com.wildbondinteractive.ostpststudio` and
`%LOCALAPPDATA%\com.wildbondinteractive.ostpststudio`.

**Signatures.** Releases from 0.1.2 onwards are signed through Microsoft Azure Artifact
Signing; 0.1.0 and 0.1.1 are unsigned. See "Verifying a download" below. Windows
SmartScreen can still warn on a correctly signed file from a project this new ("More
info", "Run anyway"); a valid signature identifies the publisher, it does not buy
reputation.

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
Releases are built, signed and verified by `.github/workflows/release.yml` from a
`v<version>` tag; see `docs/RELEASING.md`. Local builds are never signed.

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

Releases from 0.1.2 onwards are signed with Microsoft Azure Artifact Signing (Public
Trust, Authenticode, SHA-256, timestamped), by the GitHub Actions release workflow,
from the tagged commit, with no signing secrets anywhere in the project: the workflow
authenticates to Azure with OpenID Connect. The executables are signed before the
installers are built from them, and every file is verified, including the executable
inside each installer, before it can be published. The publisher Windows shows is the
verified identity of the project's maintainer, an individual; "Wildbond Interactive" is
the project's name, not the certificate subject. Releases 0.1.0 and 0.1.1 are unsigned.

The full policy is [docs/CODE_SIGNING_POLICY.md](docs/CODE_SIGNING_POLICY.md); the
operator's manual is [docs/AZURE_CODE_SIGNING.md](docs/AZURE_CODE_SIGNING.md).

### Verifying a download

Each release page lists the SHA-256 of every file and ships them as `SHA256SUMS.txt`.

```powershell
Get-FileHash ".\OST-PST Studio_0.1.2_x64-setup.exe" -Algorithm SHA256
Get-AuthenticodeSignature ".\OST-PST Studio_0.1.2_x64-setup.exe" | Format-List Status, SignerCertificate, TimeStamperCertificate
```

`Status` must be `Valid` with a timestamper present. In File Explorer: Properties,
Digital Signatures. For the complete check the release workflow itself runs, including
the executable inside an installer (needs the Windows SDK and 7-Zip):

```powershell
.\scripts\verify-signatures.ps1 -Path "$env:USERPROFILE\Downloads\OST-PST Studio_0.1.2_x64-setup.exe" -Embedded
```

## Reporting issues

Open an issue at https://github.com/wildbondinteractive/OST-PST-Studio/issues. Never
attach a real mailbox; the JSON report the application saves describes the file's
structure without message contents and is usually enough.

## Licence

MIT. See [LICENSE](LICENSE). The engine is derived from
[pstfree](https://github.com/sp00nznet/pstfree), copyright pstfree contributors, MIT.
Every open-source component compiled into the application, with its licence, is listed
in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and summarised on the application's
Settings page.
