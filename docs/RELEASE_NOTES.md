# OST/PST Studio 0.1.1

Fixes for real Outlook 2013+ OST files, found on a 1.57 GB Outlook 2016 cached mailbox
(26,417 messages, 1,088 folders). The 0.1.0 release had only been exercised on the two
small public fixtures.

## Fixed

- **Message lists showed "Unknown sender" and "(no subject)" for a real OST.** A
  4K-page OST packs heap ids differently from a PST (13 bits of block number and 14 of
  allocation index, not 16 and 11), so any folder whose contents table spans more than
  one block, or holds more than 2047 entries, listed as blank. The reader now uses the
  file's own layout.
- **Senders were still missing after that.** Outlook keeps two columns of every
  contents table in an OST (the sent-representing name and the conversation index) in
  auxiliary heaps hung off the table as subnodes 0x521 and 0x541, sharing the id space
  of the table's own heap. Nothing in the file says which column uses which heap, so
  the reader works it out by holding a few rows against the messages themselves.
- **OST to PST conversion produced a file in which most messages were unreadable**
  (25,288 of 26,417 on the mailbox above). Heaps were copied block for block; a PST
  cannot hold a 64 KB block, and once a heap block is divided every id inside it points
  at the wrong place. Every property context and table context from a 4K OST is now
  read out and laid out again in the PST's layout: blocks of at most 8,176 bytes,
  allocations of at most 3,580, larger values moved to subnodes, auxiliary heaps folded
  into the table's own, and row matrices re-packed for the smaller block size. The
  result validates fully (every folder and message readable) and its contents tables
  list the same rows as the source, cell for cell.
- The command-line executable carries a version resource (product name, version, icon).

## Still to confirm

- The converted file from the mailbox above has not yet been opened in Outlook. The
  fixtures converted with this release open directly in classic Outlook, as before;
  see `docs/OUTLOOK_COMPATIBILITY.md` and `scripts/outlook-check.ps1`.
- Three internal nodes in that OST use heap types the engine does not know (0x8C and
  0xCC, undocumented); they are copied as they are and named in the report.
- libpff cannot read that OST itself (it fails on one contents-table row), so the
  independent comparison could only be run on the public fixtures, where it reports the
  output identical to the source.

## Code signing policy

OST/PST Studio v0.1.1 is currently unsigned. Windows may display SmartScreen warnings
when installing or running this release.

For information about code signing, release verification, and our planned SignPath
Foundation integration, please see our
[Code signing policy](https://github.com/wildbondinteractive/OST-PST-Studio/blob/main/docs/CODE_SIGNING_POLICY.md).

## Requirements

Windows 10 or 11, 64-bit. The WebView2 runtime (included in current Windows; the
installer fetches it if missing). The command-line tool has no dependencies.

## Provenance

Built by the `release` GitHub Actions workflow from the tagged commit, on a GitHub-hosted
Windows runner. SHA-256 checksums of the attached files are listed at the end of these
notes by the workflow.
