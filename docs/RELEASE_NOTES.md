# OST/PST Studio 0.1.0

First release. A Windows desktop application and command-line tool for Outlook OST and
PST files, built on the pstfree engine (MIT).

## What it does

- Opens Unicode PST, Outlook 2013+ OST, pre-2013 OST and ANSI PST without Outlook,
  Exchange or a password.
- Recovers files whose index is damaged by sweeping surviving pages and carving blocks,
  and labels items it could not confirm.
- Explorer with folder tree, virtualised message list, sandboxed HTML preview
  (scripts disabled, remote content blocked), attachments with previews and SHA-256,
  and every raw MAPI property.
- Search across the mailbox with field, date, size, type, importance and attachment
  filters, plus saved presets.
- Converts OST to a Unicode PST (and ANSI to Unicode), reopens and validates the result,
  and writes valid allocation maps so Outlook should open it without SCANPST.
- Repairs damaged PSTs to a new file. The original is never modified; output is written
  to a temporary name and moved into place only when complete.
- Exports to EML, MSG, MBOX, HTML, text, CSV, vCard, iCalendar and JSON, for the whole
  mailbox, chosen folders or selected messages, with attachment extraction.
- Batch queue with per-file progress, reports, cancellation and retry.
- Reports for every job as JSON, HTML or text.
- Saved scans so large or damaged files reopen quickly.
- Command line `ostpst` with `--json` output for scripts.

## Verification

- 86 automated tests (engine and fixtures), clippy clean.
- libpff (an independent reader) reads converted and repaired outputs and finds them
  identical to their sources, folder by folder and property by property.

## Known limits

- Outlook compatibility was confirmed on the two public fixtures (a converted OST and a
  repaired PST open directly in classic Outlook, no SCANPST). Larger real-world files
  have not been tried; `scripts/outlook-check.ps1` is the manual check. See
  `docs/OUTLOOK_COMPATIBILITY.md`.
- Exporting a selection to a new PST, PST splitting and PDF export are not in this
  release.
- The installers are not code-signed; Windows SmartScreen will warn on first run.

## Requirements

Windows 10 or 11, 64-bit. The WebView2 runtime (included in current Windows; the
installer fetches it if missing). The command-line tool has no dependencies.
