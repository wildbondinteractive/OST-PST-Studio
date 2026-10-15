# Privacy policy

OST/PST Studio, version 0.1.3. Last updated 2026-10-15.

This policy describes what the application does with the data it handles. It is written
to match the code as shipped; where the code changes, this document is updated with it.
Anyone can check the claims against the source at
https://github.com/wildbondinteractive/OST-PST-Studio.

## Summary

- Mailboxes are processed entirely on your computer. Nothing from a mailbox is sent
  anywhere.
- The application collects no telemetry, no usage statistics and no crash reports, and
  includes no analytics or tracking service.
- Its only network access of its own is checking GitHub for new versions of the
  application, which you control in Settings and can turn off. Those requests carry
  nothing about your mailboxes, your files or how you use the application.
- The only things written to disk are the output you ask for and a small amount of local
  state (preferences, a recent-mailboxes list, job reports, an index cache and update
  bookkeeping), all on your machine and all deletable.

## What the application processes

OST/PST Studio reads Outlook OST and PST files you open, converts, repair, export or
verify. Those files contain mail, contacts, calendar items, attachments and the
properties attached to them. All reading and writing happens in the application process
on your computer. The original file is opened read-only and is never modified.

## Network

Browsing, searching, converting, repairing and exporting work entirely offline. The
window's content security policy permits connections only to the application's internal
command channel; the update check below is made by the application itself, not by the
window.

### Update checks (version 0.1.3 and later)

Under Settings, Software updates, you choose how the application looks for new versions:

| Setting | What is contacted, and when |
|---|---|
| Notify me (the default) | github.com, at most once a day while the application runs, to read one small file describing the latest release. Nothing is downloaded unless you choose to update. |
| Automatic | The same daily check, and when a new version exists, its installer is downloaded from the same GitHub release in the background. |
| Off | Nothing, ever, unless you press "Check for updates". |

"Check for updates" makes one check whatever the setting.

A check is an HTTPS request for one fixed address,
`github.com/wildbondinteractive/OST-PST-Studio/releases/latest/download/latest.json`, and a
download is an HTTPS request for the installer that file names, on the same project's
GitHub release page. The requests contain no mailbox content, no attachment, no file name
or path, no setting and no identifier for you or your computer. Like any web request,
they reveal your IP address and the program's user agent to GitHub, which handles them
under GitHub's privacy statement. Nothing else is sent, and nothing is sent to the
project's maintainers.

A downloaded update is installed only after its signature has been verified against a key
built into the application, and never while a conversion, repair, export or other job is
running.

### Other network activity

1. **Installing the WebView2 runtime.** The installer ships without the Microsoft Edge
   WebView2 runtime that the window needs. On a PC where it is missing (uncommon on
   Windows 10 and 11), the installer downloads Microsoft's bootstrapper from Microsoft at
   install time. No mailbox exists yet at that point and nothing of yours is sent. The
   runtime is a Microsoft component with its own update behaviour and privacy terms.
2. **Links you click** in Settings or the update dialog (the project's GitHub pages) open
   in your web browser.
3. **Remote content in a message you choose to load.** HTML message bodies are shown in
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
| Recent mailboxes and session | the application's config folder, `history.json` | the paths of mailboxes you opened, the folder you last viewed in each (when "Return to the last folder" is on), and the mailboxes open at the last exit (only when "Reopen last session" is on). Turning "Remember recent mailboxes" off deletes the list. |
| Update bookkeeping | the application's config folder, `updater.json` | when updates were last checked, and a "remind me later" choice. A downloaded update is held in memory, not on disk, until it is installed. |
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

The only service the application contacts is GitHub, for update checks and downloads as
described above, and only as your update setting allows. The application has no
accounts, no sign-in, no cloud features and no analytics provider. The optional developer tools in the repository (the libpff comparison script
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
