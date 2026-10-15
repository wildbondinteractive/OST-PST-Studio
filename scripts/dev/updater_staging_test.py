r"""End-to-end test of the in-app updater with real installers, isolated from any real
installation of OST/PST Studio.

What it does:

  1. Generates a throwaway updater key pair in a temporary folder (never the production key).
  2. Builds two *staging* versions of the application, 0.1.3 and 0.1.4 by default, with a
     different product name ("OST-PST Studio Staging") and identifier, so they install,
     store settings and appear in Windows' app list separately from the real application.
     Their update endpoint is a local HTTP server; nothing touches GitHub.
  3. Signs the newer version's NSIS and MSI installers with the throwaway key, writes a
     latest.json for them, and serves both on 127.0.0.1.
  4. Installs the older version with its NSIS installer (per user, silent), starts it with
     WebView2 remote debugging, and through the app's own commands: checks (expects the
     newer version), downloads (expects a verified package), installs. The app exits, the
     installer runs and starts the new version, which must report the new version and find
     no further update.
  5. Runs the older version's executable taken out of its MSI (bundle marker MSI) and checks
     that it downloads the MSI, not the NSIS installer. (It does not install the MSI: that
     needs an administrator prompt.)
  6. Runs the bare executable (no bundle marker, i.e. portable) and checks that it is told
     about the update but refuses to download or install it.
  7. Uninstalls the staging application and removes its data, whatever happened.

Usage, from the repository root (needs Rust, Node, Python with websocket-client):

    python scripts\dev\updater_staging_test.py [--old 0.1.3] [--new 0.1.4] [--keep]

Prints a PASS/FAIL line per check and exits 1 on any failure. Screenshots of the update
dialog go to the work folder.
"""
import argparse
import glob
import http.server
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time
import urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import cdp  # noqa: E402

PRODUCT = "OST-PST Studio Staging"
IDENTIFIER = "com.wildbondinteractive.ostpststudio.staging"
NPX = shutil.which("npx") or shutil.which("npx.cmd")
RESULTS = []


def check(ok, what, detail=""):
    RESULTS.append((bool(ok), what, detail))
    print(f"{'PASS' if ok else 'FAIL'}  {what:<58} {detail}", flush=True)
    return ok


def run(cmd, **kw):
    print("> " + " ".join(cmd), flush=True)
    return subprocess.run(cmd, cwd=ROOT, check=True, **kw)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def file_version(path):
    out = subprocess.run(
        ["powershell", "-NoProfile", "-Command", f"(Get-Item -LiteralPath '{path}').VersionInfo.ProductVersion"],
        capture_output=True, text=True,
    )
    return out.stdout.strip()


class Server(http.server.ThreadingHTTPServer):
    pass


def serve(directory, port, log):
    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **k):
            super().__init__(*a, directory=directory, **k)

        def log_message(self, fmt, *args):
            log.append(self.path)

    srv = Server(("127.0.0.1", port), Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def build(version, work, pubkey, port):
    conf = {
        "productName": PRODUCT,
        "identifier": IDENTIFIER,
        "version": version,
        "plugins": {
            "updater": {
                "pubkey": pubkey,
                "endpoints": [f"http://127.0.0.1:{port}/latest.json"],
                "dangerousInsecureTransportProtocol": True,
                "requireSignedVersion": True,
                "windows": {"installMode": "passive"},
            }
        },
    }
    path = os.path.join(work, f"staging-{version}.json")
    with open(path, "w") as f:
        json.dump(conf, f)
    run([NPX, "tauri", "build", "--ci", "--config", path, "--bundles", "nsis", "msi"])
    out = os.path.join(work, f"v{version}")
    os.makedirs(out, exist_ok=True)
    nsis = glob.glob(os.path.join(ROOT, "target", "release", "bundle", "nsis", f"{PRODUCT}_{version}_x64-setup.exe"))[0]
    msi = glob.glob(os.path.join(ROOT, "target", "release", "bundle", "msi", f"{PRODUCT}_{version}_x64_en-US.msi"))[0]
    files = {
        "nsis": shutil.copy(nsis, os.path.join(out, f"staging-{version}-setup.exe")),
        "msi": shutil.copy(msi, os.path.join(out, f"staging-{version}.msi")),
        # After bundling, the executable carries the plain (portable) marker again.
        "portable": shutil.copy(os.path.join(ROOT, "target", "release", "ostpst-studio.exe"), os.path.join(out, f"staging-{version}-portable.exe")),
    }
    return files


class App:
    """A running copy of the app with WebView2 remote debugging on its own port."""

    def __init__(self, exe):
        self.port = free_port()
        env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f"--remote-debugging-port={self.port}")
        self.proc = subprocess.Popen([exe], env=env)
        self.cdp = self.connect()

    def connect(self, timeout=60):
        cdp.PORT = self.port
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json", timeout=2)
                c = cdp.Cdp()
                if c.eval("typeof window.__TAURI_INTERNALS__ !== 'undefined' && document.readyState === 'complete'"):
                    return c
            except Exception:
                pass
            time.sleep(1)
        raise RuntimeError(f"the app did not start with remote debugging on port {self.port}")

    def invoke(self, cmd, args=None):
        js = "window.__TAURI_INTERNALS__.invoke(%s, %s).then(v => ({ok: v ?? null}), e => ({err: String(e)}))" % (
            json.dumps(cmd), json.dumps(args or {}))
        return self.cdp.eval(js)

    def status_settled(self, timeout=60):
        deadline = time.time() + timeout
        while time.time() < deadline:
            s = self.invoke("update_status")["ok"]
            if s["state"]["phase"] not in ("checking", "downloading", "verifying"):
                return s
            time.sleep(0.5)
        return s

    def click_text(self, text):
        return self.cdp.eval(
            "(()=>{const b=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===%s);if(!b)return false;b.click();return true})()"
            % json.dumps(text))

    def kill(self):
        if self.proc.poll() is None:
            subprocess.run(["taskkill", "/PID", str(self.proc.pid), "/T", "/F"], capture_output=True)


def installed_exe():
    for base in (os.environ.get("LOCALAPPDATA", ""), os.environ.get("ProgramFiles", "")):
        p = os.path.join(base, PRODUCT, "ostpst-studio.exe")
        if os.path.exists(p):
            return p
    return None


def ps(command):
    return subprocess.run(["powershell", "-NoProfile", "-Command", command], capture_output=True, text=True).stdout.strip()


def kill_staging_processes():
    """Stop only copies running from the staging install or the work folder, never a real
    installation of OST/PST Studio."""
    ps(rf"Get-Process ostpst-studio -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -like '*\{PRODUCT}\*' -or $_.Path -like '*ostpst-updater-staging*' }} | Stop-Process -Force")


def other_copies_running():
    return ps(rf"@(Get-Process ostpst-studio -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -notlike '*\{PRODUCT}\*' -and $_.Path -notlike '*ostpst-updater-staging*' }}).Count")


def cleanup():
    kill_staging_processes()
    exe = installed_exe()
    if exe:
        un = os.path.join(os.path.dirname(exe), "uninstall.exe")
        if os.path.exists(un):
            subprocess.run([un, "/S"], capture_output=True)
            time.sleep(3)
    for base in (os.environ.get("APPDATA", ""), os.environ.get("LOCALAPPDATA", "")):
        shutil.rmtree(os.path.join(base, IDENTIFIER), ignore_errors=True)
    shutil.rmtree(os.path.join(os.environ.get("LOCALAPPDATA", ""), PRODUCT), ignore_errors=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--old", default="0.1.3")
    ap.add_argument("--new", default="0.1.4")
    ap.add_argument("--work", default=os.path.join(os.environ.get("TEMP", "."), "ostpst-updater-staging"))
    ap.add_argument("--keep", action="store_true", help="leave the staging install in place")
    a = ap.parse_args()

    work = a.work
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(os.path.join(work, "key"))
    os.makedirs(os.path.join(work, "serve"))
    port = free_port()
    log = []

    # The NSIS installer closes running copies of the application by executable name, so a
    # real copy that is open would be closed too. Refuse to start rather than risk that.
    if other_copies_running() not in ("", "0"):
        print("OST/PST Studio is running. Close it first: the staging installer would close it.")
        return 2
    if installed_exe():
        print("A staging install is already present; removing it first.")
        cleanup()

    key = os.path.join(work, "key", "staging.key")
    run([NPX, "tauri", "signer", "generate", "--ci", "-w", key, "-f"], capture_output=True)
    pubkey = open(key + ".pub").read().strip()

    try:
        new = build(a.new, work, pubkey, port)
        old = build(a.old, work, pubkey, port)

        # The update: the newer installers, signed for the updater, and their manifest.
        platforms = {}
        for kind, target in (("nsis", "windows-x86_64-nsis"), ("msi", "windows-x86_64-msi")):
            name = os.path.basename(new[kind])
            served = shutil.copy(new[kind], os.path.join(work, "serve", name))
            run([NPX, "tauri", "signer", "sign", "-f", key, "-p", "", "--app-version", a.new, served], capture_output=True)
            platforms[target] = {"url": f"http://127.0.0.1:{port}/{name}", "signature": open(served + ".sig").read().strip()}
        with open(os.path.join(work, "serve", "latest.json"), "w", encoding="utf-8") as f:
            json.dump({"version": a.new, "notes": f"# Staging {a.new}\n\n- A test release served from this computer.",
                       "pub_date": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "platforms": platforms}, f)
        srv = serve(os.path.join(work, "serve"), port, log)

        # ---- NSIS: install the old version, update it in place --------------------------
        subprocess.run([old["nsis"], "/S"], check=True, timeout=300)
        exe = installed_exe()
        check(exe is not None, "old version installed with its NSIS installer", exe or "not found")
        check(file_version(exe) == a.old, "installed executable is the old version", file_version(exe))

        app = App(exe)
        time.sleep(2)
        check(app.cdp.eval("document.getElementById('root').innerHTML.length > 0 && !!document.querySelector('nav')"),
              "the installed copy's window rendered")
        s = app.invoke("update_status")["ok"]
        check(s["installKind"] == "nsis", "installed copy reports an NSIS installation", s["installKind"])
        check(s["currentVersion"] == a.old, "installed copy reports the old version", s["currentVersion"])
        s = app.invoke("update_check")["ok"]
        s = app.status_settled()
        info = s["state"].get("info") or {}
        check(s["state"]["phase"] == "available" and info.get("version") == a.new, "check finds the new version", f"{s['state']['phase']} {info.get('version')}")
        time.sleep(1.5)
        shown = app.click_text("Details")
        check(shown, "the update notice offers its details")
        time.sleep(1)
        app.cdp.shot(os.path.join(work, "update-available.png"))
        check("Download and install" in app.cdp.eval("document.body.innerText"), "the update dialog offers Download and install")
        r = app.invoke("update_download")
        s = app.status_settled()
        check(s["state"]["phase"] == "ready", "download verified and ready", s["state"]["phase"] + (" " + s["state"].get("message", "") if s["state"]["phase"] == "failed" else ""))
        check(log.count("/" + os.path.basename(new["nsis"])) == 1 and log.count("/" + os.path.basename(new["msi"])) == 0,
              "an NSIS install downloaded the NSIS installer only", ", ".join(p for p in log if p != "/latest.json"))
        time.sleep(1)
        app.cdp.shot(os.path.join(work, "update-ready.png"))
        check("Restart to update" in app.cdp.eval("document.body.innerText"), "the dialog offers Restart to update once verified")

        # Install: the app exits, the installer runs (passive) and starts the new version.
        app.cdp.eval("window.__TAURI_INTERNALS__.invoke('update_install'); 'sent'")
        try:
            app.proc.wait(timeout=60)
            check(True, "the app exited to let the installer run", f"exit code {app.proc.returncode}")
        except subprocess.TimeoutExpired:
            check(False, "the app exited to let the installer run", "still running after 60 s")
        deadline = time.time() + 180
        while time.time() < deadline and file_version(exe) != a.new:
            time.sleep(2)
        check(file_version(exe) == a.new, "the installed executable is now the new version", file_version(exe))
        # The installer restarts the app, which inherits the debugging port.
        relaunched = None
        cdp.PORT = app.port
        deadline = time.time() + 90
        while time.time() < deadline and relaunched is None:
            try:
                c = cdp.Cdp()
                if c.eval("typeof window.__TAURI_INTERNALS__ !== 'undefined' && document.readyState === 'complete'"):
                    relaunched = c
            except Exception:
                time.sleep(2)
        check(relaunched is not None, "the installer started the new version")
        if relaunched:
            app.cdp = relaunched
            time.sleep(2)
            check(app.cdp.eval("document.getElementById('root').innerHTML.length > 0"), "the updated copy's window rendered")
            app.cdp.shot(os.path.join(work, "after-update.png"))
            s = app.invoke("update_status")["ok"]
            check(s["currentVersion"] == a.new and s["installKind"] == "nsis", "the new version reports itself, still an NSIS install", f"{s['currentVersion']} {s['installKind']}")
            app.invoke("update_check")
            s = app.status_settled()
            check(s["state"]["phase"] == "upToDate", "the new version finds no further update", s["state"]["phase"])
            kill_staging_processes()
            time.sleep(2)
        uninstall_entries = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             f"@(Get-ChildItem HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall, HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall -ErrorAction SilentlyContinue | Get-ItemProperty | Where-Object DisplayName -like '{PRODUCT}*').Count"],
            capture_output=True, text=True).stdout.strip()
        check(uninstall_entries == "1", "exactly one entry in Installed apps after the update", uninstall_entries)

        # ---- MSI: the executable from the MSI downloads the MSI --------------------------
        extract = os.path.join(work, "msi-extract")
        subprocess.run(["msiexec", "/a", old["msi"], "/qn", f"TARGETDIR={extract}"], check=True, timeout=300)
        msi_exe = glob.glob(os.path.join(extract, "**", "ostpst-studio.exe"), recursive=True)[0]
        log.clear()
        app = App(msi_exe)
        s = app.invoke("update_status")["ok"]
        check(s["installKind"] == "msi", "the MSI's executable reports an MSI installation", s["installKind"])
        app.invoke("update_check")
        app.status_settled()
        app.invoke("update_download")
        s = app.status_settled()
        check(s["state"]["phase"] == "ready", "an MSI install downloads and verifies its update", s["state"]["phase"])
        check(log.count("/" + os.path.basename(new["msi"])) == 1 and log.count("/" + os.path.basename(new["nsis"])) == 0,
              "an MSI install downloaded the MSI only", ", ".join(p for p in log if p != "/latest.json"))
        app.kill()
        time.sleep(2)

        # ---- Portable: told, never downloads ------------------------------------------------
        log.clear()
        app = App(old["portable"])
        s = app.invoke("update_status")["ok"]
        check(s["installKind"] == "portable", "the bare executable reports a portable copy", s["installKind"])
        app.invoke("update_check")
        s = app.status_settled()
        check(s["state"]["phase"] == "available", "a portable copy is told about the new version", s["state"]["phase"])
        r = app.invoke("update_download")
        check("err" in r and "portable" in r["err"], "a portable copy refuses to download it", r.get("err", "no error"))
        check(not any(p.endswith((".exe", ".msi")) for p in log), "a portable copy requested no installer", ", ".join(log))
        app.kill()
        srv.shutdown()
    finally:
        if not a.keep:
            time.sleep(2)
            cleanup()
            check(installed_exe() is None, "staging installation removed")

    failed = [r for r in RESULTS if not r[0]]
    print(f"\n{len(RESULTS)} checks: {len(RESULTS) - len(failed)} passed, {len(failed)} failed. Work folder: {work}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
