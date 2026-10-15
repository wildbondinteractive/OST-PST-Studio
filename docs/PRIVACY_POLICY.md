# Privacy policy

OST/PST Studio, version 0.1.0. Last updated 2026-10-14.

This policy describes what the application does with the data it handles. It is written
to match the code as shipped; where the code changes, this document is updated with it.
Anyone can check the claims against the source at
https://github.com/wildbondinteractive/OST-PST-Studio.

## Summary

- Mailboxes are processed entirely on your computer. Nothing from a mailbox is sent
  anywhere.
- The application collects no telemetry, no usage statistics and no crash reports, and
  contains no code that talks to a server.
- The only things written to disk are the output you ask for and a small amount of local
  state (preferences, a recent-files list, job reports, and an index cache), all on your
  machine and all deletable from the Settings page.

## What the application processes

OST/PST Studio reads Outlook OST and PST files you open, converts, repair, export or
verify. Those files contain mail, contacts, calendar items, attachments and the
properties attached to them. All reading and writing happens in the application process
on your computer. The original file is opened read-only and is never modified.

## Network

The application makes no network connections of its own. The source contains no HTTP
client, no socket code and no update check; the window's content security policy
permits connections only to the application's internal command channel. The "Check for
updates" switch in Settings is inactive and documented as such.

Two things outside the application itself can involve the network:

1. **Installing the WebView2 runtime.** The installer ships without the Microsoft Edge
   WebView2 runtime that the window needs. On a PC where it is missing (uncommon on
   Windows 10 and 11), the installer downloads Microsoft's bootstrapper from Microsoft at
   install time. No mailbox exists yet at that point and nothing of yours is sent. The
   runtime is a Microsoft component with its own update behaviour and privacy terms.
2. **Remote content in a message you choose to load.** HTML message bodies are shown in
   a sandbox with scripts disabled and remote images and other remote resources blocked.
   A button on a message lets you load its remote images for that message only. Doing so
   fetches them from whatever server the sender named, which can tell that server your IP
   address and that the message was opened, exactly as it would in a mail client. This
   never happens without your clicking that button, and the setting "Block remote content
   in previews" is on by default.

## What is stored on your computer

Everything below lives under your Windows user profile and is removed by deleting those
folders or by using the Clear buttons in Settings.

| Data | Where | Contents |
|---|---|---|
| Preferences | the application's config folder, `settings.json` | theme, default folders and formats, job options, the switches on the Settings page |
| History | the application's config folder, `history.json` | the paths, sizes and item counts of recently opened files, and the reports of past jobs (paths, counts, timings, validation results, and any warnings the engine raised, which can name folders and message ids but never message text) |
| Saved scans | the application's cache folder, `scans/` | the structural index of files you have opened (node and block ids and offsets), so large or damaged files reopen quickly; never message contents |
| Window state and search presets | the application's web view storage | pane widths, sidebar state, and saved search presets including the search text you typed |

The application writes no log files. Diagnostic text shown in the window (warnings,
validation results) describes file structure and never includes message bodies or
attachment contents.

## Output you ask for

Conversions, repairs and exports write files where you choose. Those files contain the
mailbox data you selected, in the format you selected, and are yours to keep, move or
delete. Reports saved from the Activity page contain the same information as the
history entry described above. The application never writes output anywhere you did
not name, and never overwrites an existing file unless you chose that option.

## Attachments and untrusted content

Mailbox contents are treated as untrusted. Attachment names are sanitised before they
touch the file system, attachments are never executed, HTML is rendered only inside a
sandbox with scripts disabled, and exported HTML carries the same restrictions.

## Third-party services

None. The application has no accounts, no sign-in, no cloud features and no analytics
provider. The optional developer tools in the repository (the libpff comparison script
and the Outlook compatibility script) are not part of the application and are run only
by a developer, by hand.

## Children

The application is a general-purpose utility and collects no personal information from
anyone, of any age.

## Changes

Changes to how the application handles data will be reflected in this document and in
the release notes of the version that makes them.

## Contact

Questions about this policy can be raised as an issue at
https://github.com/wildbondinteractive/OST-PST-Studio/issues.
