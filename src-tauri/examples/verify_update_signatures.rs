//! Verify a release's updater signatures, and its update manifest, the way installed
//! copies of OST/PST Studio will. Run by `.github/workflows/release.yml` after the
//! installers are signed; usable locally on any release folder.
//!
//! ```text
//! cargo run --release -p ostpst-studio --example verify_update_signatures -- \
//!     --config src-tauri/tauri.conf.json --version 0.1.3 \
//!     [--manifest release/latest.json --url-prefix https://github.com/OWNER/REPO/releases/download/v0.1.3/] \
//!     FILE...
//! ```
//!
//! For every FILE: `FILE.sig` exists, verifies against the public key compiled into the
//! application (`plugins.updater.pubkey` in the configuration) with minisign, exactly as the
//! updater verifies it, and its signed trusted comment binds `--version`.
//!
//! With `--manifest`: the manifest parses with the updater plugin's own parser, announces
//! `--version`, has exactly the `windows-x86_64-nsis` and `windows-x86_64-msi` entries, each
//! URL starts with `--url-prefix` and names one of the FILEs, and each entry's signature
//! verifies against that local file. So the manifest advertises only files that exist,
//! were verified, and will be published under that URL.
//!
//! Exit code 1 on any failure. Prints no key material.

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri_plugin_updater::{RemoteRelease, RemoteReleaseInner};

const TARGETS: [&str; 2] = ["windows-x86_64-nsis", "windows-x86_64-msi"];

fn b64(s: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| format!("not base64: {e}"))?;
    String::from_utf8(bytes).map_err(|_| "not UTF-8 after decoding".to_string())
}

/// Verify `data` against a base64 `.sig` and the base64 public key; return the version the
/// signature binds.
fn verify(data: &[u8], signature_b64: &str, pubkey_b64: &str) -> Result<String, String> {
    let pk = PublicKey::decode(&b64(pubkey_b64)?).map_err(|e| format!("public key: {e}"))?;
    let sig = Signature::decode(&b64(signature_b64)?).map_err(|e| format!("signature: {e}"))?;
    pk.verify(data, &sig, true)
        .map_err(|e| format!("does not verify: {e}"))?;
    sig.trusted_comment()
        .split('\t')
        .find_map(|f| f.strip_prefix("version:"))
        .map(str::to_string)
        .ok_or_else(|| "the signature binds no version".into())
}

struct Report {
    rows: Vec<(bool, String, String)>,
}

impl Report {
    fn check(&mut self, ok: bool, what: impl Into<String>, detail: impl Into<String>) {
        self.rows.push((ok, what.into(), detail.into()));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut config = None;
    let mut version = None;
    let mut manifest = None;
    let mut url_prefix = None;
    let mut files: Vec<PathBuf> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config = args.next(),
            "--version" => version = args.next(),
            "--manifest" => manifest = args.next(),
            "--url-prefix" => url_prefix = args.next(),
            _ => files.push(PathBuf::from(a)),
        }
    }
    let (Some(config), Some(version)) = (config, version) else {
        eprintln!("usage: verify_update_signatures --config CONF --version V [--manifest JSON --url-prefix URL] FILE...");
        std::process::exit(2);
    };
    if files.is_empty() {
        eprintln!("no files to verify");
        std::process::exit(2);
    }

    let conf: serde_json::Value = match std::fs::read_to_string(&config)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{config}: {e}");
            std::process::exit(1);
        }
    };
    let pubkey = conf["plugins"]["updater"]["pubkey"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let mut r = Report { rows: Vec::new() };
    match PublicKey::decode(&b64(&pubkey).unwrap_or_default()) {
        Ok(_) => r.check(true, "public key", format!("from {config}")),
        Err(e) => r.check(false, "public key", format!("{config}: {e}")),
    }

    // Each file against its .sig.
    let mut local: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for f in &files {
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let data = match std::fs::read(f) {
            Ok(d) => d,
            Err(e) => {
                r.check(false, format!("{name}: exists"), e.to_string());
                continue;
            }
        };
        let sig_path = PathBuf::from(format!("{}.sig", f.display()));
        match std::fs::read_to_string(&sig_path) {
            Err(e) => r.check(false, format!("{name}: .sig"), e.to_string()),
            Ok(sig) => match verify(&data, &sig, &pubkey) {
                Ok(v) if v == version => r.check(
                    true,
                    format!("{name}: signature"),
                    format!("verifies, binds version {v}"),
                ),
                Ok(v) => r.check(
                    false,
                    format!("{name}: signature"),
                    format!("binds version {v}, expected {version}"),
                ),
                Err(e) => r.check(false, format!("{name}: signature"), e),
            },
        }
        local.insert(name, data);
    }

    // The manifest, parsed as the application parses it.
    if let Some(m) = &manifest {
        let prefix = url_prefix.unwrap_or_default();
        let parsed: Result<RemoteRelease, String> = std::fs::read_to_string(m)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
        match parsed {
            Err(e) => r.check(false, "manifest parses", format!("{m}: {e}")),
            Ok(release) => {
                r.check(true, "manifest parses", Path::new(m).display().to_string());
                r.check(
                    release.version.to_string() == version,
                    "manifest version",
                    release.version.to_string(),
                );
                r.check(
                    release.version.pre.is_empty(),
                    "manifest is a stable version",
                    release.version.to_string(),
                );
                r.check(
                    release.pub_date.is_some(),
                    "manifest date",
                    format!("{:?}", release.pub_date),
                );
                match &release.data {
                    RemoteReleaseInner::Dynamic(_) => {
                        r.check(false, "manifest platforms", "not the static format")
                    }
                    RemoteReleaseInner::Static { platforms } => {
                        let mut keys: Vec<&String> = platforms.keys().collect();
                        keys.sort();
                        r.check(
                            keys.iter()
                                .map(|k| k.as_str())
                                .eq(["windows-x86_64-msi", "windows-x86_64-nsis"]),
                            "manifest platforms",
                            format!("{keys:?}"),
                        );
                        for t in TARGETS {
                            let Some(p) = platforms.get(t) else { continue };
                            let url = p.url.as_str();
                            let ok_prefix = !prefix.is_empty()
                                && url.starts_with(&prefix)
                                && p.url.scheme() == "https";
                            r.check(ok_prefix, format!("{t}: url"), url);
                            let name = url.rsplit('/').next().unwrap_or_default().to_string();
                            let expected_kind = if t.ends_with("msi") {
                                ".msi"
                            } else {
                                "-setup.exe"
                            };
                            r.check(
                                name.ends_with(expected_kind),
                                format!("{t}: installer type"),
                                &name,
                            );
                            match local.get(&name) {
                                None => r.check(
                                    false,
                                    format!("{t}: file"),
                                    format!("{name} is not among the verified files"),
                                ),
                                Some(data) => match verify(data, &p.signature, &pubkey) {
                                    Ok(v) if v == version => r.check(
                                        true,
                                        format!("{t}: signature"),
                                        format!("matches {name}, binds {v}"),
                                    ),
                                    Ok(v) => r.check(
                                        false,
                                        format!("{t}: signature"),
                                        format!("binds {v}, expected {version}"),
                                    ),
                                    Err(e) => r.check(false, format!("{t}: signature"), e),
                                },
                            }
                        }
                    }
                }
            }
        }
    }

    let failed = r.rows.iter().filter(|(ok, ..)| !ok).count();
    for (ok, what, detail) in &r.rows {
        println!("{}  {what:<42} {detail}", if *ok { "PASS" } else { "FAIL" });
    }
    println!(
        "\n{} checks: {} passed, {failed} failed",
        r.rows.len(),
        r.rows.len() - failed
    );
    if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        let mut md = format!("## Updater signature verification\n\n{} checks: {} passed, {failed} failed\n\n| Result | Check | Detail |\n|---|---|---|\n", r.rows.len(), r.rows.len() - failed);
        for (ok, what, detail) in &r.rows {
            md.push_str(&format!(
                "| {} | {what} | {} |\n",
                if *ok { "PASS" } else { "FAIL" },
                detail.replace('|', "/")
            ));
        }
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)
            .and_then(|mut f| std::io::Write::write_all(&mut f, md.as_bytes()));
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
