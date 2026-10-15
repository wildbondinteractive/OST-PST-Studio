# Azure Artifact Signing: setup and maintenance

How OST/PST Studio releases get signed, what lives where, and what to do when it breaks.
The public-facing policy is [CODE_SIGNING_POLICY.md](CODE_SIGNING_POLICY.md); this is
the operator's manual.

## What is managed where

| Managed in this repository (source control) | Managed by hand in the Azure portal and GitHub settings |
|---|---|
| `.github/workflows/release.yml`: the signing workflow | The Azure Artifact Signing account and certificate profile |
| `scripts/azure-sign.ps1` and `src-tauri/tauri.release.conf.json`: the sign command the installer builder calls | |
| `scripts/verify-signatures.ps1`: the verification run before publishing | The Entra application registration and its federated credential |
| `docs/RELEASE_NOTES.md`: becomes the release body | The role assignment that lets that application sign |
| The version in `Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json` | The GitHub environment `release-signing` and its variables and protection rules |

Nothing in the repository can sign anything on its own. The workflow only works when
the hand-managed pieces exist and agree with each other.

## Azure resources

| Item | Value |
|---|---|
| Resource group | `rg-wildbond-signing` |
| Artifact Signing account | `wildbondsigning` |
| Region and endpoint | East US, `https://eus.codesigning.azure.net/` |
| Certificate profile | `WildbondPublicTrust`, type Public Trust |
| Entra application registration | `Wildbond-GitHub-Signing` |

The certificate profile's subject is the maintainer's verified individual identity, as
validated by Microsoft. The subscription is the maintainer's personal subscription.

### Role assignment

The Entra application needs exactly one role on the signing account (or on the
certificate profile, or on the resource group; the account is the usual scope):

> **Artifact Signing Certificate Profile Signer**

Portal: the `wildbondsigning` account, Access control (IAM), Add role assignment, role
"Artifact Signing Certificate Profile Signer", assign access to "User, group, or service
principal", select `Wildbond-GitHub-Signing`. Do not give the application Contributor,
Owner, or any other role; signing needs none of them.

### Federated credential (OIDC)

On the application registration `Wildbond-GitHub-Signing`: Certificates & secrets,
Federated credentials, Add credential, scenario "GitHub Actions deploying Azure
resources":

| Field | Value |
|---|---|
| Organization | `wildbondinteractive` |
| Repository | `OST-PST-Studio` |
| Entity type | Environment |
| Environment name | `release-signing` |
| Issuer | `https://token.actions.githubusercontent.com` |
| Subject identifier | `repo:wildbondinteractive@234866646/OST-PST-Studio@1419887626:environment:release-signing` |
| Audience | `api://AzureADTokenExchange` |

**The subject uses GitHub's immutable format.** Repositories created after 15 July 2026
(this one was created on 14 October 2026) identify themselves in OIDC tokens by numeric
owner and repository IDs, not by names: `repo:OWNER@OWNER-ID/REPO@REPO-ID:environment:NAME`.
The older name-only form, `repo:wildbondinteractive/OST-PST-Studio:environment:release-signing`,
does not match this repository's tokens and must not be used. The portal's "GitHub
Actions deploying Azure resources" scenario generates the name-only form, so the subject
has to be entered by hand (or the credential created with the "Other issuer" scenario).
The IDs are public and stable across renames; confirm them with:

```powershell
gh api repos/wildbondinteractive/OST-PST-Studio --jq '"owner \(.owner.id) repo \(.id)"'
gh api repos/wildbondinteractive/OST-PST-Studio/actions/oidc/customization/sub
```

The second command shows `use_immutable_subject: true` and the exact `sub_claim_prefix`
GitHub puts in front of `:environment:release-signing`.

The subject must match exactly, including the `environment:` part. A credential made for
a branch or a tag, or one using the name-only form, does not match a job that runs in
the environment, and the login step fails with `AADSTS70021` (no matching federated
identity record).

No client secret is created. If one exists on the application from earlier experiments,
delete it; the workflow never uses it, and the repository must never hold one.

## GitHub configuration

### Environment `release-signing`

Repository Settings, Environments, `release-signing`. Protection rules to set:

- **Deployment branches and tags:** "Selected branches and tags", add a tag rule `v*`
  (and nothing else). This is what stops a pull request or a branch build from ever
  reaching the environment, whatever the workflow file says.
- **Required reviewers:** configured. A run pauses at the signing job, before anything
  is signed, until a reviewer approves it on the run page ("Review deployments").

Environment variables (Settings, Environments, `release-signing`, Environment
variables; **variables**, not secrets, because none of them is secret):

| Variable | Value |
|---|---|
| `AZURE_CLIENT_ID` | Application (client) ID of `Wildbond-GitHub-Signing` |
| `AZURE_TENANT_ID` | Directory (tenant) ID |
| `AZURE_SUBSCRIPTION_ID` | The subscription holding `rg-wildbond-signing` |
| `AZURE_SIGNING_ENDPOINT` | `https://eus.codesigning.azure.net/` |
| `AZURE_SIGNING_ACCOUNT` | `wildbondsigning` |
| `AZURE_CERT_PROFILE` | `WildbondPublicTrust` |
| `RELEASE_SIGNER_SUBJECT` | Optional. Text the signer certificate's subject must contain, normally the CN as the certificate profile shows it. When set, verification fails if a different identity signed. When empty, the subject is reported but not checked. |

The workflow reads all of these with `${{ vars.NAME }}` at run time. There are no
secrets to add: OIDC replaces them.

### What the workflow asks GitHub for

The signing job declares `permissions: id-token: write, contents: read` and
`environment: release-signing`. The publishing job declares `contents: write` only. The
workflow-level default is `contents: read`. Actions are referenced by major version tag
(`azure/login@v3`, `azure/artifact-signing-action@v2`, `actions/*@v4`,
`softprops/action-gh-release@v2`); pin them to commit SHAs if the repository's policy
tightens later.

## How a release is produced

1. Bump the version in `Cargo.toml` (`[workspace.package]`), `package.json` and
   `src-tauri/tauri.conf.json` to the same value, update `docs/RELEASE_NOTES.md` (it
   becomes the release body), regenerate `THIRD_PARTY_NOTICES.md` if dependencies
   changed, commit, push, and wait for the `build` workflow to pass.
2. Tag and push:

   ```powershell
   git tag v0.1.2
   git push origin v0.1.2
   ```

   For a release candidate use `v0.1.2-rc.1`; the manifests still say `0.1.2`, and the
   release is marked as a pre-release.
3. The `release` workflow runs. In order: tag check, build, tests, Azure login over
   OIDC, `tauri bundle` with `src-tauri/tauri.release.conf.json` (the bundler calls
   `scripts/azure-sign.ps1` for the executable as it packages each installer, for the
   NSIS uninstaller stub, and for each finished installer), sign the portable executable
   and the command-line tool with `azure/artifact-signing-action`, verify everything
   including the executable inside each installer, write `SHA256SUMS.txt`, upload, and
   create a **draft** release with the five files attached.

### Why signing is hooked into the bundler

The Tauri bundler writes a bundle-type marker (`NSS`, `MSI`, or `UNK` for a plain
build) into the executable before packaging it, then restores the plain marker. An
executable signed before bundling would therefore no longer verify inside the
installer. Tauri's `bundle.windows.signCommand` exists for exactly this: the bundler
calls it on the executable right after writing the marker, on the uninstaller stub, and
on each installer when it is complete. `scripts/azure-sign.ps1` is that command. It uses
the ArtifactSigning PowerShell module, which is the component the official GitHub
action wraps (the workflow installs the same pinned version the action pins), with the
same digest, timestamp and credential settings, so the two signing paths produce the
same kind of signature. The bundler also offers its NSIS plug-in DLLs and WiX extension
DLLs for signing; the script declines those, as they are third-party tools.
`scripts/dev/staged-packaging-check.ps1` demonstrates the whole sequence locally with a
stand-in sign command that only logs.
4. Open the draft on the Releases page, read the verification summary in the workflow
   run, download one file and check it on another machine, then publish.

A tag can also be run again by hand: Actions, `release`, "Run workflow", enter the
existing tag. The same checks apply; a non-release tag or a branch is refused.

## Verifying a signed file

```powershell
.\scripts\verify-signatures.ps1 -Path "C:\Downloads\OST-PST Studio_0.1.2_x64-setup.exe" -Embedded
```

Checks: file exists, version resource (with `-ExpectedVersion`), signed, status Valid,
signer subject (optionally matched), code signing usage, chain root is "Microsoft
Identity Verification Root Certificate Authority 2020", RFC 3161 timestamp present,
`signtool verify /pa` succeeds, file digest SHA-256, and for installers the
`ostpst-studio.exe` unpacked from inside passes the same checks. Needs the Windows SDK
(signtool) and 7-Zip (to unpack the NSIS installer).

## Troubleshooting

### `azure/login` fails

| Message | Cause | Fix |
|---|---|---|
| `AADSTS70021: No matching federated identity record found` | The OIDC subject does not match the federated credential | The job must run with `environment: release-signing`, and the credential's subject must be the immutable form `repo:wildbondinteractive@234866646/OST-PST-Studio@1419887626:environment:release-signing`, not the name-only form |
| `AADSTS700016: Application ... was not found` | Wrong `AZURE_CLIENT_ID` or `AZURE_TENANT_ID` | Copy both from the application's Overview page |
| `No subscriptions found` | The application has no role in the subscription, or wrong `AZURE_SUBSCRIPTION_ID` | The signer role on the account is inside the subscription, so this normally means the ID is wrong |
| `Error: Unable to get ACTIONS_ID_TOKEN_REQUEST_URL` | The job lacks `id-token: write` | Keep the permissions block on the job |

### Signing fails

| Message | Cause | Fix |
|---|---|---|
| `403 Forbidden` / `AuthorizationFailed` from the signing endpoint | The application lacks the signer role, or has it on the wrong scope | Assign "Artifact Signing Certificate Profile Signer" on the `wildbondsigning` account |
| `404` naming the account or profile | `AZURE_SIGNING_ACCOUNT`, `AZURE_CERT_PROFILE` or the endpoint region is wrong | The endpoint must be the East US one for this account |
| Certificate profile not active | Identity validation lapsed or the profile was disabled | Azure portal, the account, Certificate profiles; renew the identity validation |
| Timestamp errors | `timestamp.acs.microsoft.com` unreachable from the runner | Re-run the job; the timestamp is mandatory, so the run must fail rather than sign without it |
| Only some files signed | A path in `files` did not exist at that point of the build | Check the preceding build step's output; the file names come from the build, not from the workflow |
| `azure-sign.ps1: AZURE_SIGNING_ENDPOINT is not set` during `tauri bundle` | The bundle step did not receive the environment variables | They are passed with `env:` on that step from `vars.*`; check the environment's variables exist |
| `Import-Module ArtifactSigning` fails | The module install step did not run or PSGallery was unreachable | Re-run; the version is pinned by `ARTIFACT_SIGNING_MODULE_VERSION` in the workflow |

### Verification fails

The verification step prints a table of every check. A `FAIL` on "signature valid" with
`HashMismatch` means a file was modified after signing (for example an installer
rebuilt after the executable was signed); on "trusted root" it means a different
certificate signed the file; on "timestamped" the signature would expire with the
certificate and the release must not be published. In every case, fix the cause and run
the workflow again for the same tag. Delete the failed draft if one was created.

### When a signing operation fails

Nothing has been published. The draft release is created only after verification
passes, so a failed run leaves no release behind. Fix the cause, then re-run the
workflow for the same tag from the Actions page. If the tag itself was wrong, delete it
(`git push --delete origin v0.1.2`, `git tag -d v0.1.2`) and tag again.

## Rotating or replacing the certificate profile

1. Create the new profile in the Azure portal under the same account (or a new account
   in the same region).
2. Make sure the Entra application has the signer role on the new account or profile.
3. Change `AZURE_CERT_PROFILE` (and `AZURE_SIGNING_ACCOUNT` or `AZURE_SIGNING_ENDPOINT`
   if those changed) on the `release-signing` environment. Change
   `RELEASE_SIGNER_SUBJECT` if the subject changed.
4. Run the workflow for a release candidate tag and verify the result before using the
   profile for a release. Record the change in `docs/CODE_SIGNING_POLICY.md` if the
   publisher identity changed.

Nothing in the repository needs to change for a profile change; only the environment
variables do.

## Local development

Local builds are never signed and never claim to be. `npm run tauri dev`,
`npm run tauri build` and `cargo build` need no Azure access. A locally built installer
shows `NotSigned` in `Get-AuthenticodeSignature`, and SmartScreen warns on it. Only the
release workflow signs.
