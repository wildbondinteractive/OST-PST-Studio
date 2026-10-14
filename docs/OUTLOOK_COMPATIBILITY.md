# Outlook compatibility

What has been established about files this engine writes, and how. A file that this
engine can reopen is not thereby a file Outlook will open; the two are reported
separately everywhere in the application.

## Levels of evidence

1. **Completed**: the writer finished and the file was moved into place.
2. **Validated by this engine**: the output was reopened from its own header and index,
   every folder and every message was read back, node, folder and message counts match
   the source net of anything the writer reported dropping, and no structural warning
   was raised. Every checksum in the file was computed over bytes the writer laid down,
   so a single warning here means a writer bug.
3. **Verified by libpff**: `tests/libpff_compare.py` reads the source and the output with
   libpff (the reference implementation most other PST tools wrap, which shares no code
   with this engine) and compares folders, messages and every property, id for id.
4. **Opened in Outlook**: a person attached the file to a classic Outlook profile and
   Outlook read it without asking for the Inbox Repair Tool. This is the only level that
   is a statement about Outlook, and it is recorded here only when it has been done.

## Current standing (2026-10-14)

| Output | Engine validation | libpff | Outlook |
|---|---|---|---|
| `example-2013.ost` converted to PST, maps written | Passed, 11 checks | Identical: 46 folders, 3 messages, 202 properties | Not yet opened |
| `dist-list.pst` repaired to a new PST, maps written | Passed | Identical: 24 folders, 4 messages, 287 properties | Not yet opened |
| `dist-list.pst` with both B-tree roots zeroed, salvaged and repaired | Passed (upstream test) | libpff reads all 4 messages (upstream bake-off) | Not yet opened |

Upstream pstfree reports one real-world data point: a user converted a customer OST,
Outlook asked for SCANPST because the allocation maps were marked invalid, and opened the
file after SCANPST and after the message-store fixes that are now part of this engine.
This engine additionally writes valid AMap, PMap, FMap, FPMap and DList pages and marks
the header `VALID_AMAP2`, which removes the documented reason for the SCANPST prompt.
Whether it removes the prompt in practice has not been observed yet.

## How to check in Outlook

Classic Outlook is required (new Outlook does not mount PST files). The script
`scripts/outlook-check.ps1` copies the PST, attaches the copy to the running Outlook
profile with `Namespace.AddStoreEx`, walks and counts every folder, and detaches it. It
prompts before each step. It is never run by the tests or by the application, because
attaching a store changes the Outlook profile for the duration of the check.

Record the outcome in the table above, including the Outlook build number and whether
SCANPST was requested.

## What the writer does for Outlook specifically

- Message store rebuilt for OST input: `PidTagIpmSubTreeEntryId`, wastebasket and finder
  entry ids pointing at `Root - Mailbox\IPM_SUBTREE`, its Deleted Items and `Finder`;
  `PidTagValidFolderMask`; `PidTagPstPassword = 0`; a display name.
- Unicode 512-byte-page layout, `wVer = 23`, `wVerClient = 19`, `NDB_CRYPT_NONE` for
  converted output (blocks written plain); a repaired PST keeps its source encoding.
- Fresh NBT and BBT B-trees, every block trailer re-signed for its new offset.
- Allocation maps: see `crates/ostpst-core/src/amap.rs` for the derivation of FMap and
  FPMap placement, which the specification only draws.
- OST sync-state nodes (types 0x14 to 0x19) are carried across unchanged, as upstream
  does. They have not been shown to do harm; they have not been shown to be harmless
  in Outlook either.

## Known gaps

- No file over 125 MB written by Outlook has been examined, so FMap recurrence is
  derived, not observed. Outlook 2007 SP2 and later do not read FMaps.
- The `.msg` export has never been opened in Outlook (upstream had no Outlook; this
  project has not run the manual check yet).
- WIP-encrypted files (`bCryptMethod = 0x10`) are not supported and are reported as an
  unknown encoding.
