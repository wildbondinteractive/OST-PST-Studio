"""Regenerate THIRD_PARTY_NOTICES.md from Cargo metadata and node_modules.

    cargo metadata --format-version 1 > meta.json
    python scripts/third-party-notices.py meta.json

Run from the repository root after `npm install`. Lists every crate that is compiled
into a workspace member (normal and build dependencies, not dev-only) and every npm
package in "dependencies" (the ones bundled into the user interface).
"""
import collections
import json
import os
import sys

HEAD = """# Third-party notices

OST/PST Studio is MIT licensed (see LICENSE). It is built from the components below,
every one of which is distributed under an OSI-approved open-source licence. Nothing
proprietary and no pre-built third-party binary is included in the application; every
component is compiled from source by the build described in the README.

This file is generated from the lock files (`Cargo.lock`, `package-lock.json`) by
`scripts/third-party-notices.py`; regenerate it when dependencies change.

## Upstream engine

The PST/OST parser, repair, conversion and export code in `crates/ostpst-core` is derived
from pstfree 0.5.1 by the pstfree contributors, MIT licensed,
https://github.com/sp00nznet/pstfree. Each carried-over file keeps an attribution banner.
The `mpbbCrypt` table in `crypt.rs` is transcribed from the published MS-PST specification
(Microsoft Open Specifications, usable under the Open Specifications Promise).

## Runtime

The application window is hosted by the Microsoft Edge WebView2 runtime, a Microsoft
component installed separately on Windows (the installer fetches Microsoft's bootstrapper
if it is missing). It is not redistributed in this repository and Microsoft's licence and
privacy terms apply to it.
"""


def rust_packages(meta):
    workspace = set(meta["workspace_members"])
    by_id = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen, stack = set(), list(workspace)
    while stack:
        pkg = stack.pop()
        if pkg in seen:
            continue
        seen.add(pkg)
        for dep in nodes[pkg]["deps"]:
            kinds = {k.get("kind") for k in dep["dep_kinds"]}
            if kinds <= {"dev"}:
                continue
            stack.append(dep["pkg"])
    rows = []
    for pkg in seen:
        if pkg in workspace:
            continue
        p = by_id[pkg]
        rows.append((p["name"], p["version"], p.get("license") or "?", p.get("repository") or ""))
    return sorted(rows)


def npm_packages():
    pkg = json.load(open("package.json", encoding="utf-8"))
    rows = []
    for name in sorted(pkg["dependencies"]):
        p = json.load(open(os.path.join("node_modules", name, "package.json"), encoding="utf-8"))
        repo = p.get("repository")
        repo = repo.get("url") if isinstance(repo, dict) else repo
        rows.append((name, p.get("version"), p.get("license"), repo or ""))
    return rows


def main():
    meta = json.load(open(sys.argv[1], encoding="utf-8"))
    rust = rust_packages(meta)
    npm = npm_packages()
    out = [HEAD]
    out.append("## npm packages bundled into the user interface (%d)" % len(npm))
    out.append("")
    out.append("| Package | Version | Licence | Source |")
    out.append("|---|---|---|---|")
    out += ["| %s | %s | %s | %s |" % r for r in npm]
    out.append("")
    out.append("Development-only packages (Vite, TypeScript, Tailwind, the Tauri CLI, type definitions)")
    out.append("are not part of the shipped application and are listed in `package.json`.")
    out.append("")
    out.append("## Rust crates compiled into the engine, command line and application (%d)" % len(rust))
    out.append("")
    out.append("Licence expressions as declared by each crate. Where a crate offers a choice, this")
    out.append("project uses it under the MIT terms when MIT is offered, otherwise Apache-2.0.")
    out.append("")
    out.append("| Licence expression | Crates |")
    out.append("|---|---|")
    counts = collections.Counter(r[2] for r in rust)
    for lic, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])):
        out.append("| %s | %d |" % (lic, n))
    out.append("")
    out.append("| Crate | Version | Licence | Source |")
    out.append("|---|---|---|---|")
    out += ["| %s | %s | %s | %s |" % r for r in rust]
    out.append("")
    with open("THIRD_PARTY_NOTICES.md", "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(out))
    print("rust crates: %d, npm packages: %d" % (len(rust), len(npm)))


if __name__ == "__main__":
    main()
