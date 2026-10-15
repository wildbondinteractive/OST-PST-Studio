# OST/PST Studio 0.1.2

The first release built by the signed release pipeline. The application itself is the
0.1.1 engine with no functional change; this release is about how it is delivered.

## Changed

- **Signed binaries.** The desktop executable, the command-line executable, the NSIS
  installer and the MSI are signed with Microsoft Azure Artifact Signing (Public Trust,
  Authenticode, SHA-256, RFC 3161 timestamp). The executables are signed before the
  installers are built from them, so the program inside each installer is signed, not
  only the installer. The release workflow verifies every file, including the executable
  unpacked from inside each installer, before anything is attached here; a file that
  fails verification cannot be published.
- **SHA-256 checksums** of every attached file are listed at the end of these notes and
  shipped as `SHA256SUMS.txt`.
- The command-line executable carries a version resource (product name, version, icon).

## Code signing policy

Releases from 0.1.2 onwards are signed through Microsoft Azure Artifact Signing. The
publisher shown by Windows is the verified identity of the project's maintainer, an
individual; Wildbond Interactive is the project's name, not the certificate subject.
Releases 0.1.0 and 0.1.1 are unsigned.

A valid signature does not by itself prevent Microsoft SmartScreen warnings while a
publisher is still building reputation; "Windows protected your PC" can still appear
on a correctly signed file from a new project.

To check a download: in PowerShell, `Get-AuthenticodeSignature <file>` must report
`Valid`, and `Get-FileHash <file> -Algorithm SHA256` must match the list below. The full
policy, including who signs, how, and how to run the complete verification yourself, is
the [Code signing policy](https://github.com/wildbondinteractive/OST-PST-Studio/blob/main/docs/CODE_SIGNING_POLICY.md).

## Known limits

- Outlook compatibility was confirmed on the two public fixtures (a converted OST and a
  repaired PST open directly in classic Outlook, no SCANPST). A 1.57 GB real OST
  converts to a file that validates fully and lists identically to its source, but that
  output has not yet been opened in Outlook; `scripts/outlook-check.ps1` is the manual
  check. See `docs/OUTLOOK_COMPATIBILITY.md`.
- Three internal nodes of that OST use undocumented heap types; they are copied as they
  are and named in the report.
- Exporting a selection to a new PST, PST splitting and PDF export are not in this
  release.

## Requirements

Windows 10 or 11, 64-bit. The WebView2 runtime (included in current Windows; the
installer fetches it if missing). The command-line tool has no dependencies.

## Provenance

Built, signed and verified by the `release` GitHub Actions workflow from the tagged
commit, on a GitHub-hosted Windows runner, with no secrets in the repository: the
workflow authenticates to Azure with OpenID Connect. The verification summary is in the
workflow run attached to this tag.
