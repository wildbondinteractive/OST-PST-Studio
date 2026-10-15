# OST/PST Studio

Professional Windows desktop application for Outlook OST/PST recovery, conversion,
browsing and export. Rust engine (forked from pstfree, MIT) + Tauri 2 + React/TypeScript.

## Rules

- `F:\Projects\OSTtoPST\pstfree-og` is the upstream reference. Read-only. Never modify it.
- Never commit real mailboxes. `fixtures/data/` and every `*.pst`/`*.ost` are gitignored.
- The engine never modifies an input file. Output goes to `<target>.partial` and is
  renamed into place only after the writer finished and validation ran.
- Never report Outlook compatibility that has not been established. The report
  distinguishes completed, validated-by-this-engine, libpff-verified, and opened in
  Outlook manually.
- No em dashes anywhere in prose or code comments.
- No telemetry, no network calls, no remote content in previews unless the user allows
  it per message.
- Keep upstream attribution banners at the top of the carried-over engine files.
- Releases are built only by GitHub Actions (`docs/CODE_SIGNING_POLICY.md`). Never commit
  signing credentials; SignPath is wired through the secret `SIGNPATH_API_TOKEN` and the
  variable `SIGNPATH_ORGANIZATION_ID` only.

## Layout

```
Cargo.toml              workspace: crates/ostpst-core, crates/ostpst-cli, src-tauri
crates/ostpst-core      engine. ndb/ltp/convert/repair/export/crypt/cfbf/html are upstream;
                        mailbox/model/jobs/report/validate/amap/cache/ostfix/heapw/exporters/fsutil/props are new
crates/ostpst-cli       `ostpst` command line (technicians, automation, --json)
src-tauri               Tauri host: commands (lib.rs), job manager (jobs.rs), settings.rs
src                     React UI: lib/api.ts mirrors model.rs; pages/*, components/*
fixtures/               fetch script writes fixtures/data (gitignored)
tests/                  libpff_compare.py (independent validation), upstream bakeoff
docs/                   audit, status, test results, competitor matrix, Outlook notes
scripts/                fetch-fixtures.ps1, outlook-check.ps1 (manual), third-party-notices.py, dev helpers
.github/workflows/      ci.yml (build + test + installer artifacts), release.yml (tag to draft release,
                        SignPath signing job inactive until the secret and variable exist)
```

## Build and test

```powershell
.\scripts\fetch-fixtures.ps1        # once; tests skip without fixtures
cargo test --workspace              # engine + CLI tests (fixtures make them meaningful)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
npm install
npm run typecheck                   # tsc
npm run build                       # vite -> dist/
npm run tauri dev                   # desktop app with hot reload
npm run tauri build                 # release exe + NSIS + MSI in src-tauri/target/release/bundle
cargo build --release -p ostpst-cli # target/release/ostpst.exe
python tests/libpff_compare.py <source> <output>   # needs: pip install libpff-python
```

## Conventions

- Engine errors are `String`; `CANCELLED` is the cancellation sentinel. Progress callbacks
  return `bool` (`false` = stop at the next safe point).
- All types crossing to the UI live in `model.rs` with `camelCase` serde and are mirrored
  by hand in `src/lib/api.ts`. Change both.
- Long work never runs on the UI thread: Tauri commands use `spawn_blocking`; jobs run on
  their own threads with their own `Mailbox` handle and emit `job://progress`,
  `job://update`, `job://done`.
- Listings come from folder contents tables when present (cheap), falling back to one
  property-context read per message. Bodies and attachments are read on demand.
- HTML bodies render in a sandboxed iframe (`sandbox=""`, DOMPurify, CSP blocking
  remote fetches). Attachments are never executed.
- Keep `docs/IMPLEMENTATION_STATUS.md` and `docs/TEST_RESULTS.md` current at each
  milestone.
