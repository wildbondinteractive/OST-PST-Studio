# In-app updater: design and maintenance

How installed copies of OST/PST Studio find, verify and install new versions, and what a
maintainer has to keep in order. Read this before changing `src-tauri/src/updater.rs`,
the updater part of `.github/workflows/release.yml`, or the `plugins.updater` block of
`src-tauri/tauri.conf.json`.

## The pieces

| Piece | Where | What it does |
|---|---|---|
| Tauri updater plugin 2.14 | `src-tauri/Cargo.toml` | Fetches the manifest, compares versions, downloads, verifies the minisign signature, starts the installer |
| Update logic | `src-tauri/src/updater.rs` | Modes, schedule, stable-channel rule, installer-type selection, the "nothing running" rule, state shown to the window |
| Commands | `src-tauri/src/lib.rs` | `update_status`, `update_check`, `update_download`, `update_install`, `update_remind_later`, `close_app` |
| Configuration | `src-tauri/tauri.conf.json`, `plugins.updater` | Public key, the manifest URL, `requireSignedVersion`, passive Windows install |
| User interface | `src/components/updates/` | Settings, the notice, the dialog, release notes |
| Release workflow | `.github/workflows/release.yml` | Signs the installers for the updater, writes and verifies `latest.json` |
| Verifier | `src-tauri/examples/verify_update_signatures.rs` | Checks signatures and the manifest with the same library the app uses |
| Tests | `src-tauri/src/updater.rs`, `src-tauri/src/updater_tests.rs`, `scripts/dev/updater_staging_test.py` | Rules, the real plugin against a local server, and a real install-and-update |

The window never talks to the plugin directly (its `updater:*` permissions are not
granted). Every action goes through the host commands, which enforce the rules below.

## Two different signatures

| | Authenticode (Azure Artifact Signing) | Updater signature (Tauri, minisign) |
|---|---|---|
| Proves | To Windows: who published the file | To the installed app: this update was produced by the project's release workflow |
| Checked by | Windows, SmartScreen, `Get-AuthenticodeSignature` | The updater, before anything is run |
| Key | Microsoft-held certificate, short-lived, via OIDC | An Ed25519 key pair the maintainer holds |
| Where the private part lives | Azure, never exported | GitHub secret on `release-signing`, plus the maintainer's offline backup |
| Public part | In the signature | In `tauri.conf.json` (`plugins.updater.pubkey`), compiled into every copy |

The updater signature is made *after* all Authenticode signing, over the final installer
bytes, so it covers exactly what users download. Neither replaces the other.

## Rules the code enforces

- **Stable channel only.** The endpoint is
  `https://github.com/wildbondinteractive/OST-PST-Studio/releases/latest/download/latest.json`.
  GitHub resolves `releases/latest` to the newest *published, non-prerelease* release, so
  drafts and prereleases are never served. The workflow writes no `latest.json` for an
  `-rc.N` tag. The app also rejects any manifest version with a prerelease part or not
  newer than itself (`is_stable_update`).
- **Signed, version-bound packages.** `requireSignedVersion: true`: the signature's trusted
  comment must name the version the manifest announces, so an old signed release cannot be
  replayed as a new one. A package that fails verification is discarded.
- **Same installer type.** The bundler writes a marker into the executable (`NSS`, `MSI`, or
  nothing for a bare executable). The updater reads `windows-x86_64-nsis` or
  `windows-x86_64-msi` accordingly. There is deliberately no plain `windows-x86_64` entry,
  so an MSI install is never offered the NSIS installer (which would leave two entries in
  Installed apps and change the install scope), and a portable copy is never given an
  installer to run.
- **Portable copies are notified, never updated.** They read the NSIS entry only to learn
  the version, and offer "Download from GitHub".
- **Never during work.** Installing exits the application, so it is refused while a job is
  queued or running or a file operation is in progress. Downloading is allowed meanwhile.
- **Modes.** `notify` (default): check at most daily, ask before downloading. `automatic`:
  also download in the background; install when the user closes the app (no relaunch), or
  on "Restart to update" (relaunch). `off`: no request unless the user presses "Check for
  updates". Background checks: 10 seconds after launch, then hourly ticks that act only when
  24 hours have passed since a successful check, or 6 hours since a failed one.
- **Nothing about the user is sent.** A check is one GET for a fixed URL; a download is one
  GET for the installer named in the manifest. No mailbox path, content or identifier is in
  either request.

## Windows installer behaviour

| Installed with | Update installs with | Elevation | Notes |
|---|---|---|---|
| NSIS setup (`*-setup.exe`) | NSIS setup, `/P /UPDATE` (passive) | None (per-user install) | `/R` restarts the app afterwards when the user chose "Restart to update" |
| MSI (`*_en-US.msi`) | `msiexec /i ... /passive /promptrestart` | Windows asks, as at first install (per-machine) | `AUTOLAUNCHAPP=True` restarts the app when asked to |
| Portable executable | Not installed; the user downloads from GitHub | None | |
| Command-line tool | Not covered by the updater | | Download new versions from GitHub |

The app exits as the installer starts (a Windows installer cannot replace a running
program). The product name, identifier (`com.wildbondinteractive.ostpststudio`), install
folder and Installed apps entry stay the same across updates; never change the identifier
or `productName` without planning a migration, because an update would install a second
application next to the first.

Version 0.1.2 and earlier have no updater: users install 0.1.3 once by hand, and every
later version can reach them in the app.

## Keys

### Generating the production key pair (once)

On the maintainer's own PC, outside the repository, with a password:

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.tauri" | Out-Null
npx tauri signer generate -w "$env:USERPROFILE\.tauri\ostpst-studio-updater.key"
```

It asks for a password (twice). It writes the private key to that file and the public key
to the same name with `.pub`. Then:

1. **Back up the private key and its password offline**, separately (for example the key
   file on an encrypted USB drive kept away from the PC, the password in a password
   manager). Losing either means installed copies can never be updated again: they only
   trust this one public key.
2. Put the **public** key into `src-tauri/tauri.conf.json` as `plugins.updater.pubkey` (the
   whole content of the `.pub` file, one line). It is not secret.
3. Add the **private** key and its password to the `release-signing` environment as
   **secrets** (Settings, Environments, `release-signing`, Environment secrets):

   ```powershell
   gh secret set TAURI_SIGNING_PRIVATE_KEY --env release-signing --repo wildbondinteractive/OST-PST-Studio < "$env:USERPROFILE\.tauri\ostpst-studio-updater.key"
   gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --env release-signing --repo wildbondinteractive/OST-PST-Studio
   ```

   The second command prompts for the value, so the password never appears in the shell
   history.

The key never enters the repository. `.gitignore` excludes `*.key`.

### What the workflow does with it

The step "Sign the installers for the in-app updater" runs
`npx tauri signer sign --app-version <version>` on the two installers, reading the key and
password from the secrets as environment variables. It prints signatures (public), never
the key. The next step verifies each signature against the public key in
`tauri.conf.json`; if the configured public key does not match the private key in the
secret, the release fails there.

### Rotating the key

Installed copies trust exactly one public key, compiled in. To move to a new key:

1. Generate the new pair as above.
2. Release a version N signed with the **old** key whose `tauri.conf.json` carries the
   **new** public key. Installed copies accept N (old key) and from then on trust the new key.
3. Switch the secrets to the new private key for version N+1 onward.

Users who skip version N must install N+1 by hand. If the old private key is compromised,
publish a manual-download notice: the old key can no longer be trusted to carry the switch.

## Releasing 0.1.4 and later

1. Bump the version in `Cargo.toml`, `package.json` and `src-tauri/tauri.conf.json`, and
   write `docs/RELEASE_NOTES.md`: its text becomes both the GitHub release body and the
   notes shown inside the app.
2. Push, wait for the `build` workflow, tag `vX.Y.Z`, push the tag, approve the
   `release-signing` deployment.
3. The workflow builds, Authenticode-signs, verifies, signs for the updater, writes
   `latest.json`, verifies it, and drafts the release with `latest.json` attached.
4. While the release is a draft, installed copies keep seeing the previous release's
   manifest. **Publishing the draft is what releases the update to every installed copy
   within a day** (sooner for anyone who presses "Check for updates").
5. Before publishing: download the setup program from the draft and check it as in
   `docs/AZURE_CODE_SIGNING.md`. For a significant change, run the staging test below.

Never edit or replace assets of a published release: `latest.json` names the exact files
and their signatures. To fix a bad release, publish a new version.

## Testing

| Test | Command | Covers |
|---|---|---|
| Rules | `cargo test -p ostpst-studio --lib updater::` | Stable channel, schedule, snooze, busy gate, error kinds |
| Plugin against a local server | `cargo test -p ostpst-studio --lib updater_tests::` | Newer, same, older, prerelease; no network, 404 and 500, malformed and incomplete manifests; missing installer entry; tampered package, other key, version replay, bad signature; failed download; manual check with updates off; NSIS and MSI selection; portable; jobs and file operations blocking the install |
| Release verification | `cargo run --release -p ostpst-studio --example verify_update_signatures -- ...` | Signatures and manifest of a release folder, as the app reads them |
| Real update on this PC | `python scripts\dev\updater_staging_test.py` | Builds staging 0.1.3 and 0.1.4 under a different name and identifier, installs 0.1.3 per user, updates it in place to 0.1.4 through the app's own commands, checks the MSI and portable variants, uninstalls |

The staging test uses a throwaway key and a server on 127.0.0.1, and refuses to start while
a real copy of OST/PST Studio is running (the NSIS installer closes running copies by name).

## When an update fails

| What the user sees | Cause | What to do |
|---|---|---|
| "Could not reach GitHub" | No network, proxy, firewall | Retry later; the app retries on its own after 6 hours |
| "GitHub has no update information" | The latest published release has no `latest.json` (for example a release published without the workflow) | Publish a release made by the workflow |
| "Update information could not be read" | Malformed `latest.json`, or no entry for this installer type | Fix and publish a new version; never edit a published one |
| "Update rejected" (signature) | Package altered, wrong key, or signature for another version | Investigate before anything else: this is what tampering looks like. Users can install by hand from GitHub |
| "The installer could not be started" | Blocked by policy or security software | Users install by hand from GitHub |
| Install refused, "N jobs still in progress" | Work running | Expected; it installs when the work is done |

A failed update never leaves a broken installation: nothing is run unless the package
verified, and the installers themselves replace the program atomically or roll back.
