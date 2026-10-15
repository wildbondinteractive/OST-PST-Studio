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

Unsigned builds work, but Windows SmartScreen warns on first run and some mail-admin
environments block them. Signing needs an Authenticode code-signing certificate; the
project is set up so that once the certificate is on the build PC, one command signs the
application, the CLI, the NSIS installer and the MSI.

### 1. Get a certificate

| Option | Cost | Who it suits |
|---|---|---|
| Azure Trusted Signing (Microsoft) | about 10 USD/month, pay as you go | Individuals and companies; identity check takes days, no hardware token; signatures get SmartScreen reputation quickly |
| OV certificate from DigiCert, Sectigo, SSL.com, GlobalSign | 200 to 500 USD/year, delivered on a hardware token or in a cloud HSM | Established companies; SmartScreen reputation builds over time |
| EV certificate | 300 to 700 USD/year | Immediate SmartScreen reputation; stricter identity checks |
| SignPath Foundation | free for open-source projects that qualify | OSS builds through their CI integration |
| Self-signed test certificate | free | Only for testing the pipeline; Windows will still warn |

For a public MIT project the realistic choices are Azure Trusted Signing or an OV
certificate. Either way the private key never leaves the vendor's hardware or cloud
HSM; signtool uses it in place.

### 2. Install it on the build PC

- Hardware token or cloud HSM certificate: install the vendor's client (for example
  SafeNet Authentication Client or the vendor's KSP) and plug in or sign in. The
  certificate appears in `Cert:\CurrentUser\My`.
- Azure Trusted Signing: install the Trusted Signing dlib and client, sign in with
  `az login`, and set `OSTPST_SIGN_MODE=azure` (see `scripts/sign.ps1` for the exact
  signtool arguments it then uses, or adapt them to your account and profile names).

Check that Windows can see a code-signing certificate:

```powershell
Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Select-Object Subject, Thumbprint, NotAfter
```

### 3. Build signed

```powershell
npm run build:signed
```

That runs `tauri build` with `src-tauri/tauri.signed.conf.json`, which tells the bundler
to call `scripts/sign.ps1` for every artifact. The script finds the certificate (or the
one named by `OSTPST_SIGN_THUMBPRINT` when several are installed), signs with SHA-256,
applies an RFC 3161 timestamp from `http://timestamp.digicert.com` (override with
`OSTPST_TIMESTAMP_URL`), and verifies the result. Sign the CLI the same way:

```powershell
cargo build --release -p ostpst-cli
powershell -File scripts\sign.ps1 targetelease\ostpst.exe
```

### 4. Verify before publishing

```powershell
Get-AuthenticodeSignature "targeteleaseundle
sis\OST-PST Studio_0.1.0_x64-setup.exe" | Format-List Status, SignerCertificate, TimeStamperCertificate
```

`Status` must be `Valid` and the timestamper must be present. Then, in File Explorer,
right-click the installer, Properties, Digital Signatures, and confirm the publisher
name is the one you expect.

Only after this do the release notes get to say the build is signed. A self-signed
certificate does not count.

## Before tagging

1. `cargo clippy --workspace --all-targets -- -D warnings` and `npm run typecheck` clean.
2. `cargo test --workspace` green with fixtures present.
3. `python tests\libpff_compare.py` on a converted and a repaired fixture: identical.
4. Install the NSIS build on a clean Windows user account and open a fixture.
5. Update `docs/TEST_RESULTS.md` and `docs/IMPLEMENTATION_STATUS.md`.
