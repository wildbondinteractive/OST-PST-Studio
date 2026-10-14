r"""Drive the desktop app's WebView2 through the Chrome DevTools Protocol, for UI tests.

Launch the app with remote debugging enabled, then use this to evaluate JavaScript in
the page and capture screenshots:

    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
    .\target\debug\ostpst-studio.exe
    python scripts\cdp.py shot out.png
    python scripts\cdp.py eval "document.title"
    python scripts\cdp.py click "button[aria-label='Settings']"
    python scripts\cdp.py reload

Needs: pip install websocket-client
"""
import base64
import json
import sys
import time
import urllib.request

import websocket

import os
PORT = int(os.environ.get("CDP_PORT", "9222"))


def connect():
    targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json"))
    page = next(t for t in targets if t["type"] == "page")
    ws = websocket.create_connection(page["webSocketDebuggerUrl"], suppress_origin=True)
    return ws


class Cdp:
    def __init__(self):
        self.ws = connect()
        self.seq = 0

    def call(self, method, **params):
        self.seq += 1
        self.ws.send(json.dumps({"id": self.seq, "method": method, "params": params}))
        while True:
            msg = json.loads(self.ws.recv())
            if msg.get("id") == self.seq:
                if "error" in msg:
                    raise RuntimeError(msg["error"])
                return msg.get("result", {})

    def eval(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, awaitPromise=True, returnByValue=True)
        if "exceptionDetails" in r:
            raise RuntimeError(r["exceptionDetails"].get("text", str(r["exceptionDetails"])))
        return r.get("result", {}).get("value")

    def shot(self, path):
        r = self.call("Page.captureScreenshot", format="png")
        with open(path, "wb") as f:
            f.write(base64.b64decode(r["data"]))
        return path

    def click(self, selector):
        return self.eval(
            "(function(){const e=document.querySelector(%s);if(!e)return 'no element';"
            "e.scrollIntoView({block:'center'});e.click();return 'clicked';})()" % json.dumps(selector)
        )

    def type_into(self, selector, text):
        return self.eval(
            "(function(){const e=document.querySelector(%s);if(!e)return 'no element';"
            "const setter=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set;"
            "setter.call(e,%s);e.dispatchEvent(new Event('input',{bubbles:true}));return 'typed';})()"
            % (json.dumps(selector), json.dumps(text))
        )

    def text(self):
        return self.eval("document.body.innerText")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    c = Cdp()
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "shot":
        print(c.shot(args[0] if args else "shot.png"))
    elif cmd == "eval":
        print(json.dumps(c.eval(args[0]), indent=2))
    elif cmd == "click":
        print(c.click(args[0]))
    elif cmd == "type":
        print(c.type_into(args[0], args[1]))
    elif cmd == "text":
        print(c.text())
    elif cmd == "reload":
        c.call("Page.reload")
        time.sleep(2)
        print("reloaded")
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
