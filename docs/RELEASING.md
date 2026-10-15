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

Published binaries are built by GitHub Actions, not on a developer PC, so that anyone can
trace a download back to a workflow run and a commit. The repository is
https://github.com/wildbondinteractive/OST-PST-Studio.

1. Make sure the last `build` workflow run on `main` is green
   (`.github/workflows/ci.yml`). Its artifacts are the same installers a release will
   contain.
2. Update `docs/RELEASE_NOTES.md`; it becomes the body of the release.
3. Tag and push:

   ```powershell
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. `.github/workflows/release.yml` checks that the tag matches the version in
   `Cargo.toml`, `package.json` and `src-tauri\tauri.conf.json`, runs the tests, builds the
   four files above, computes SHA-256 checksums and creates a **draft** release with the
   notes and the files attached.
5. Download the artifacts from the draft, install the NSIS build on a clean Windows user
   account, open a fixture, then publish the draft from the Releases page.

The workflow never publishes on its own. If anything in the run looks wrong, delete the
draft and the tag and fix the source first.

## Signing

Unsigned builds work, but Windows SmartScreen warns on first run and some mail-admin
environments block them. The policy that governs signing, including who may approve it,
is `docs/CODE_SIGNING_POLICY.md`. Read that first; the rest of this section is the
mechanics.

### SignPath Foundation (the intended route)

The project is applying to SignPath Foundation's free code-signing programme for
open-source projects. When the application is accepted:

1. In SignPath, create the project `OST-PST-Studio`, an artifact configuration
   `windows-installers` that matches the files in the `unsigned-artifacts` workflow
   artifact (NSIS installer, MSI, portable exe, CLI exe), and the signing policies
   `test-signing` and `release-signing` as SignPath prescribes.
2. In the GitHub repository settings add the secret `SIGNPATH_API_TOKEN` (the token of the
   SignPath CI user) and the repository variable `SIGNPATH_ORGANIZATION_ID`.
3. Nothing else changes. The `sign` job in `.github/workflows/release.yml` is skipped
   while the variable is absent; once it exists, every tagged release submits its unsigned
   artifact to SignPath, waits for a signing approver to approve it in SignPath, and the
   draft release is assembled from the signed files instead. The release notes then carry
   the line "Free code signing provided by SignPath.io, certificate by SignPath
   Foundation." and only then.

No credential for SignPath exists in the repository and none should ever be committed.

### Signing on a developer PC with your own certificate

This path exists for anyone who forks the project and holds a certificate of their own.
Builds signed this way are not published as releases of this project (see the policy).
Once the certificate is on the build PC, one command signs the application, the CLI, the
NSIS installer and the MSI.

### 1. Get a certificate

| Option | Cost | Who it suits |
|---|---|---|
| Azure Trusted Signing (Microsoft) | about 10 USD/month, pay as you go | Individuals and companies; identity check takes days, no hardware token; signatures get SmartScreen reputation quickly |
| OV certificate from DigiCert, Sectigo, SSL.com, GlobalSign | 200 to 500 USD/year, delivered on a hardware token or in a cloud HSM | Established companies; SmartScreen reputation builds over time |
| EV certificate | 300 to 700 USD/year | Immediate SmartScreen reputation; stricter identity checks |
| Self-signed test certificate | free | Only for testing the pipeline; Windows will still warn |

Either way the private key never leaves the vendor's hardware or cloud HSM; signtool
uses it in place.

### 2. Install it on the build PC

- Hardware token or cloud HSM certificate: install the vendor's client (for example
  SafeNet Authentication Client or the vendor's KSP) and plug in or sign in. The
  certificate appears in `Cert:\CurrentUser\My`.
- Azure Trusted Signing: install the Trusted Signing client (`Azure.CodeSigning.Dlib`),
  sign in with `az login`, write the small metadata JSON Microsoft documents (endpoint,
  account name, certificate profile name), and point the script at both:

  ```powershell
  $env:OSTPST_SIGN_DLIB = "C:\Tools\TrustedSigning\bin\x64\Azure.CodeSigning.Dlib.dll"
  $env:OSTPST_SIGN_METADATA = "C:\Tools\TrustedSigning\metadata.json"
  ```

  With those set, `scripts/sign.ps1` signs through the cloud key instead of a local
  certificate.

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
powershell -File scripts\sign.ps1 target\release\ostpst.exe
```

### 4. Verify before publishing

```powershell
Get-AuthenticodeSignature "target\release\bundle\nsis\OST-PST Studio_0.1.0_x64-setup.exe" | Format-List Status, SignerCertificate, TimeStamperCertificate
```

`Status` must be `Valid` and the timestamper must be present. Then, in File Explorer,
right-click the installer, Properties, Digital Signatures, and confirm the publisher
name is the one you expect.

Only after this do the release notes get to say the build is signed. A self-signed
certificate does not count.

## Before tagging

1. `cargo clippy --workspace --all-targets -- -D warnings` and `npm run typecheck` clean
   (the `build` workflow checks the same).
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
