# Releasing

How a build becomes something other people can download.

## Build

```powershell
npm install
cargo test --workspace
npm run tauri build                   # release exe + NSIS + MSI
cargo build --release -p ostpst-cli   # the command line
```

Outputs:

| File | What it is |
|---|---|
| `target\release\bundle\nsis\OST-PST Studio_<version>_x64-setup.exe` | Per-user installer (no admin rights). Fetches the WebView2 runtime if missing. |
| `target\release\bundle\msi\OST-PST Studio_<version>_x64_en-US.msi` | MSI for managed deployment. |
| `target\release\ostpst-studio.exe` | Portable application. Needs the WebView2 runtime, which every current Windows 10/11 has. |
| `target\release\ostpst.exe` | Command-line tool, fully standalone. |

Bump the version in `Cargo.toml` (`workspace.package.version`), `package.json` and
`src-tauri\tauri.conf.json` together.

## Publish on GitHub

The repository is https://github.com/wildbondgames/OST-PST-Studio. Create a release
from the web UI (Releases, "Draft a new release"), tag it `v<version>`, and attach the
four files above. Suggested release notes: the "What it does" and "Honest limits"
sections of `README.md`, plus the current `docs/TEST_RESULTS.md` summary line.

With the GitHub CLI installed this is one command:

```powershell
gh release create v0.1.0 "target\release\bundle\nsis\OST-PST Studio_0.1.0_x64-setup.exe" "target\release\bundle\msi\OST-PST Studio_0.1.0_x64_en-US.msi" target\release\ostpst-studio.exe target\release\ostpst.exe --title "OST/PST Studio 0.1.0" --notes-file docs\RELEASE_NOTES.md
```

## Signing

The builds are not code-signed. Windows SmartScreen will warn on first run of an
unsigned installer. To sign, obtain an Authenticode certificate and set
`bundle.windows.certificateThumbprint` (and `digestAlgorithm`, `timestampUrl`) in
`src-tauri\tauri.conf.json`; Tauri signs the executable and installers during
`tauri build`. Never claim a build is signed unless this was done.

## Before tagging

1. `cargo clippy --workspace --all-targets -- -D warnings` and `npm run typecheck` clean.
2. `cargo test --workspace` green with fixtures present.
3. `python tests\libpff_compare.py` on a converted and a repaired fixture: identical.
4. Install the NSIS build on a clean Windows user account and open a fixture.
5. Update `docs/TEST_RESULTS.md` and `docs/IMPLEMENTATION_STATUS.md`.
