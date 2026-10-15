# OST/PST Studio 0.1.3

Automatic updates, and a round of everyday improvements. Mailbox reading, conversion,
repair and export are unchanged from 0.1.2.

**Install this version by hand once.** 0.1.2 and earlier cannot update themselves. Run the
new setup program (or MSI) over your current installation; your settings and history are
kept. From here on, new versions arrive inside the application.

## New

- **Software updates.** OST/PST Studio checks the project's GitHub releases for new
  versions, shows what changed, downloads the update and verifies its signature before
  installing. Choose under Settings, Software updates:
  - **Notify me** (default): check once a day and tell you; nothing is downloaded until
    you choose to.
  - **Automatic**: also download in the background; the update installs when you close
    the application, or straight away if you choose "Restart to update".
  - **Off**: never check on its own; "Check for updates" still works.
- Updates **never interrupt work**: installing waits while a conversion, repair, export,
  verification or batch is running.
- An installation keeps its type: a setup-program install updates with the setup
  program, an MSI install with the MSI. The portable executable is told about new
  versions and links to the download page; it never installs anything.
- **Closing while a job runs** now asks first. If you close anyway, jobs stop at a safe
  point and remove their unfinished output; original mailboxes are never changed.

## Improved

- **Recent mailboxes** on the dashboard: reopen with one click, see which files have
  been moved or deleted, remove entries one by one or clear the list.
- Optional: **reopen last session** at launch, and **return to the last folder** you
  viewed in a mailbox (Settings, General).
- "Remember recent mailboxes" (Settings, Privacy) now really stops the list being
  recorded, and turning it off clears the list. In 0.1.2 the switch was not honoured.
- **Drag and drop**: the window says what a drop will do before you let go and rejects
  files that are not mailboxes. Dropping one file on Convert, Export or Repair makes it
  the source. Dropping several asks whether to open them or add them to a batch.
- **Keyboard**: F5 refreshes the current view (it used to reload the whole window), Ctrl+F
  focuses the search box when a mailbox is open, Esc clears the search, and Settings
  lists all shortcuts.
- **Settings** is organised into General, Output, Software updates, Privacy and About.
  Typing in a settings field no longer loses focus after each character.
- **About** shows the version, how the application was installed, the code-signing
  publisher, and links to the source, release notes, issue tracker and licences.
- Job notifications say where the output went, and that a cancelled job removed its
  unfinished output.

## Privacy

Update checks are the application's only network access of its own. A check asks GitHub
for one small file; a download fetches the installer from the same release. Neither
carries anything about your mailboxes, files or use of the application, and there is no
telemetry. Turn checks off under Settings, Software updates. Details: the
[privacy policy](https://github.com/wildbondinteractive/OST-PST-Studio/blob/main/docs/PRIVACY_POLICY.md).

## Code signing policy

The installers, the desktop executable and the command-line tool are signed with
Microsoft Azure Artifact Signing (Public Trust, Authenticode, SHA-256, timestamped). The
publisher shown by Windows is Jacob Adlon, the project's maintainer; Wildbond
Interactive is the project's name, not the certificate holder. In-app updates carry an
additional signature that the application checks before installing.

A valid signature does not by itself prevent Microsoft SmartScreen warnings while a
publisher is still building reputation.

To check a download: in PowerShell, `Get-AuthenticodeSignature <file>` must report
`Valid`, and `Get-FileHash <file> -Algorithm SHA256` must match the list below. Full
policy: [Code signing policy](https://github.com/wildbondinteractive/OST-PST-Studio/blob/main/docs/CODE_SIGNING_POLICY.md).

## Known limits

- A 1.57 GB real OST converts to a file that validates fully, but that output has not
  yet been opened in Outlook; `scripts/outlook-check.ps1` is the manual check.
- Exporting a selection to a new PST, PST splitting and PDF export are not in this
  release.

## Requirements

Windows 10 or 11, 64-bit. The WebView2 runtime (included in current Windows; the
installer fetches it if missing). The command-line tool has no dependencies.
