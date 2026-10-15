# Code signing policy

This document is the code signing policy for OST/PST Studio (published under the file
and product name "OST-PST Studio", because a slash cannot appear in a Windows file
name). It is linked from the README and from every release page.

## Status

**No release of OST/PST Studio is code-signed yet.** An application to the SignPath
Foundation free code-signing programme for open-source projects is being prepared.
Until a release page says otherwise, every installer and executable published for this
project is unsigned, Windows SmartScreen will warn when it is run, and nothing in the
project's documentation, release notes or application claims a signature that does not
exist.

Once the application is accepted and a release has been signed through it, the
releases in question will carry this statement, and only those releases:

> Free code signing provided by SignPath.io, certificate by SignPath Foundation.

The certificate belongs to SignPath Foundation, which is the legal publisher named in
the signature. The project does not own the certificate and does not describe it as its
own.

## What a signature means

A signed release is one whose binaries were built automatically by the GitHub Actions
workflow in this repository (`.github/workflows/release.yml`) from a tagged commit on
the `main` branch, then signed from that workflow's artifact after manual approval by a
signing approver named below. The signature therefore attests that the binary came from
this repository's source at that tag, built by the public workflow; it does not attest
to anything beyond that.

Builds made on a developer's machine are never signed.

## Team and roles

The project is maintained by one person at present.

| Role | Who | Responsibility |
|---|---|---|
| Author (committer) | Jacob Adlon, GitHub [@wildbondinteractive](https://github.com/wildbondinteractive), repository owner | May change source code and build scripts; owns the repository |
| Reviewer | Jacob Adlon | Reviews every change from anyone who is not a committer before it is merged. Changes to `.github/workflows/`, `scripts/`, `src-tauri/tauri*.json` and `Cargo.toml` files are treated as source and reviewed with the same care as code |
| Signing approver | Jacob Adlon | Approves every signing request by hand. No release is signed automatically |

The repository's owner and collaborator lists are the authoritative record of
membership: https://github.com/wildbondinteractive/OST-PST-Studio. The roles above are
updated here when membership changes. All team members use multi-factor authentication
on GitHub and, once provisioned, on SignPath.

Changes to any of these roles require a commit to this file by the repository owner.

## Build and release process

1. All code lives in this repository. The engine is derived from pstfree (MIT), carried
   in `crates/ostpst-core` as a visible fork with upstream attribution banners on every
   carried-over file and a change log in `docs/IMPLEMENTATION_STATUS.md`. No upstream
   binary is included; everything is compiled from source here.
2. The continuous-integration workflow (`.github/workflows/ci.yml`) runs on every push
   and pull request on a GitHub-hosted Windows runner: formatting, clippy with warnings
   denied, the full test suite against the public fixtures, the TypeScript check, the
   production front-end build, the Tauri bundle, and an independent read of the
   conversion output with libpff. It uploads the installers and executables as workflow
   artifacts.
3. A release is cut by tagging `main` with `v<version>`. The release workflow builds the
   same artifacts from that tag and attaches them to a **draft** GitHub Release. A
   maintainer publishes the draft by hand after checking the artifacts.
4. When SignPath signing is active, the release workflow submits the unsigned artifact
   to SignPath, a signing approver approves the request in SignPath by hand, and the
   signed artifacts replace the unsigned ones on the release before it is published.
   This step is present in the workflow but inactive until the project is accepted
   (see "Preparing for SignPath" below).
5. Binaries carry Windows version metadata: the product name is "OST-PST Studio" and
   the product version equals the tag's version, set in one place for the engine, the
   command-line tool, the application and the installers (`Cargo.toml`,
   `package.json`, `src-tauri/tauri.conf.json`).

## What the software does with your data

OST/PST Studio processes mailboxes entirely on the user's computer. It makes no network
connections, collects no telemetry and contains no update check. The one network action
is the installer fetching Microsoft's WebView2 runtime when it is absent from the PC.
The full statement, including what is stored locally and the one user-initiated case in
which remote images in a message can be loaded, is in
[docs/PRIVACY_POLICY.md](PRIVACY_POLICY.md).

Third-party components that run on the user's machine: the Microsoft Edge WebView2
runtime (Microsoft's privacy terms apply to it) and the open-source libraries listed in
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md), none of which performs network
access in this application.

## Installation and removal

The NSIS installer installs for the current user and registers an entry under Settings,
Apps, Installed apps, from which it is removed; the MSI does the same through Programs
and Features. The portable executable needs no installation and is removed by deleting
it. Both leave behind the preferences and history files described in the privacy
policy, which can be deleted from the application's Settings page or by removing the
application's folder under `%APPDATA%` and `%LOCALAPPDATA%`.

## Preparing for SignPath

Nothing SignPath-specific is configured yet, and no credentials exist in the repository
or its secrets. When the application is approved:

1. Create the SignPath organisation, project (`OST-PST-Studio`) and signing policies
   (`test-signing`, `release-signing`) as SignPath prescribes.
2. Add the SignPath API token as the repository secret `SIGNPATH_API_TOKEN` and the
   organisation id as the repository variable `SIGNPATH_ORGANIZATION_ID`.
3. The release workflow's signing job is already written against those names and is
   skipped while they are absent; once they exist it runs on tagged releases and waits
   for manual approval in SignPath.
4. Update the Status section of this document and the release notes template, and only
   then describe a release as signed.
