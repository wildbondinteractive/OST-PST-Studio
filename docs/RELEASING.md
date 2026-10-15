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

Published binaries are built, signed and verified by GitHub Actions, never on a
developer PC, so that anyone can trace a download back to a workflow run, a commit, and
a signature. The repository is https://github.com/wildbondinteractive/OST-PST-Studio.

1. Make sure the last `build` workflow run on `main` is green
   (`.github/workflows/ci.yml`).
2. Update `docs/RELEASE_NOTES.md`; it becomes the body of the release.
3. Tag and push:

   ```powershell
   git tag v0.1.2
   git push origin v0.1.2
   ```

   A release candidate is `v0.1.2-rc.1` with the manifests still at `0.1.2`; it is
   published as a pre-release.
4. `.github/workflows/release.yml` checks that the tag is a release tag and matches the
   version in `Cargo.toml`, `package.json` and `src-tauri\tauri.conf.json`, runs the
   tests, builds the executables, builds the installers with signing hooked into the
   bundler (the executable is signed as each installer packages it, then the installer
   is signed), signs the portable executable and the command-line tool, verifies every
   file (including the executable unpacked from each installer), writes SHA-256
   checksums and creates a **draft** release with the five files attached. Any signing
   or verification failure fails the run and nothing is attached.
5. Open the draft, read the verification summary in the run, install the NSIS build on
   a clean Windows user account, check Properties, Digital Signatures on it, then publish
   the draft from the Releases page.

The workflow never publishes on its own. If anything in the run looks wrong, delete the
draft and the tag and fix the source first.

## Signing

Releases are signed through Microsoft Azure Artifact Signing by the release workflow,
authenticated with OpenID Connect; no certificate, key or secret exists outside Azure.
The policy is `docs/CODE_SIGNING_POLICY.md`; the account, environment variables, role
assignment and troubleshooting are in `docs/AZURE_CODE_SIGNING.md`. Local builds are
unsigned and `npm run tauri build` needs no Azure access.

To prove the packaging sequence locally, without Azure (builds, bundles with a logging
stand-in for the sign command, and checks what the bundler hands it and what the
installers contain):

```powershell
pwsh -File scripts\dev\staged-packaging-check.ps1
```

To check a signed file, locally or on a download:

```powershell
.\scripts\verify-signatures.ps1 -Path "target\release\bundle\nsis\OST-PST Studio_0.1.2_x64-setup.exe" -Embedded
```

## Before tagging

1. `cargo clippy --workspace --all-targets -- -D warnings` and `npm run typecheck` clean
   (the `build` workflow checks the same).
0. Bump the version in `Cargo.toml`, `package.json` and `src-tauri\tauri.conf.json`
   together; the release workflow refuses a tag that does not match all three.
2. `cargo test --workspace` green with fixtures present.
3. `python tests\libpff_compare.py` on a converted and a repaired fixture: identical (the
   `build` workflow runs this too).
4. Install the NSIS build on a clean Windows user account and open a fixture.
5. Update `docs/TEST_RESULTS.md`, `docs/IMPLEMENTATION_STATUS.md` and
   `docs/RELEASE_NOTES.md`.
6. If a dependency changed, regenerate `THIRD_PARTY_NOTICES.md`:

   ```powershell
   cargo metadata --format-version 1 > $env:TEMP\meta.json
   python scripts\third-party-notices.py $env:TEMP\meta.json
   ```
