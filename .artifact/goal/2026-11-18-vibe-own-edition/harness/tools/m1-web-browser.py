"""Capture the real NSH Web app and verify its HTTP and WebSocket traffic."""

from functools import partial
from datetime import datetime, timezone
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import re
from threading import Thread
from urllib.parse import urlsplit
from urllib.request import urlopen

from playwright.sync_api import expect, sync_playwright


def capture(site, evidence, backend, port, workspace, workspace_id, process_id):
    class Site(SimpleHTTPRequestHandler):
        def do_GET(self):
            if self.path == "/api/workspaces" or self.path.startswith("/api/sessions?"):
                with urlopen(backend + self.path, timeout=10) as response:
                    body = response.read()
                    self.send_response(response.status)
                    self.send_header("Content-Type", response.headers["Content-Type"])
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
            else:
                super().do_GET()

    handler = partial(Site, directory=site)
    with ThreadingHTTPServer(("127.0.0.1", port), handler) as server:
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with sync_playwright() as playwright:
                with playwright.chromium.launch(channel="chrome") as browser:
                    page = browser.new_page(viewport={"width": 1080, "height": 760})
                    errors, sockets = [], set()
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    page.on("console", lambda message: print(f"Chrome {message.type}: {message.text}", flush=True)
                            if message.type in ("error", "warning") else None)
                    page.on("websocket", lambda socket: sockets.add(urlsplit(socket.url).path))
                    url = f"http://127.0.0.1:{port}"
                    with (page.expect_response(f"{url}/nsh-bindgen_bg.wasm") as wasm,
                          page.expect_response(f"{url}/api/workspaces") as workspaces):
                        page.goto(f"{url}/?backend_port={urlsplit(backend).port}")
                    assert wasm.value.ok and wasm.value.headers["content-type"] == "application/wasm"
                    assert workspaces.value.ok
                    page.wait_for_load_state("networkidle")
                    expect(page).to_have_title("vibe-own | NSH M1")
                    root = page.locator("#app")
                    assert not errors, errors
                    with page.expect_response(f"{url}/api/sessions?workspace_id={workspace_id}") as session:
                        root.get_by_text(workspace, exact=True).first.click(timeout=12000)
                    assert session.value.ok
                    try:
                        expect(root.get_by_text(re.compile(r"(?:^|/)review\.txt$")).first).to_be_visible(timeout=8000)
                    except AssertionError:
                        print(f"Chrome stream diagnostics: {sockets}; page errors: {errors}; view:\n{root.inner_text()}", flush=True)
                        raise
                    expect(root.get_by_text("log_transport=websocket", exact=False)).to_be_visible(timeout=12000)
                    expect(root.get_by_text("status=planned", exact=False)).to_be_visible()
                    count = root.get_by_text(re.compile(r"^[1-9]\d* entries$"))
                    expect(count).to_be_visible(timeout=12000)
                    before = count.inner_text()
                    page.screenshot(path=evidence / "m1-web-live-before.png")
                    expect(count).not_to_have_text(before, timeout=6000)
                    after = count.inner_text()
                    page.screenshot(path=evidence / "m1-web-live-after.png")
                    assert {
                        f"/api/workspaces/{workspace_id}/git/diff/ws",
                        "/api/execution-processes/stream/session/ws",
                        f"/api/execution-processes/{process_id}/raw-logs/ws",
                    } <= sockets
                    assert not errors, errors
                    sample = root.get_by_text(re.compile(r"^\d{4}\s+Scroll sample$")).first
                    first_row = sample.inner_text()
                    page.mouse.move(100, 430)
                    started = datetime.now(timezone.utc)
                    page.evaluate("""() => {
                        const samples = [];
                        window.m1Frames = samples;
                        const add = (time) => {
                            if (window.m1Frames !== samples) return;
                            samples.push(time);
                            requestAnimationFrame(add);
                        };
                        requestAnimationFrame(add);
                    }""")
                    for _ in range(60):
                        page.mouse.wheel(0, 120)
                        page.wait_for_timeout(50)
                    result = page.evaluate("""() => {
                        const times = window.m1Frames;
                        delete window.m1Frames;
                        return { times, scale: devicePixelRatio, width: innerWidth, height: innerHeight };
                    }""")
                    last_row = sample.inner_text()
                    assert first_row != last_row, "Browser benchmark did not scroll"
                    times = result["times"]
                    intervals = sorted(b - a for a, b in zip(times, times[1:]))
                    assert len(intervals) >= 60, "Insufficient rAF samples during scroll"
                    p95 = intervals[min(len(intervals) * 95 // 100, len(intervals) - 1)]
                    evidence.joinpath("m1-web-perf.md").write_text(
                        f"# Chrome 滚动实测\n\n```text\nChrome {browser.version}; Release WASM; {result['width']}x{result['height']}; DPR {result['scale']}\n"
                        f"UTC start: {started.isoformat()}\n"
                        f"Scroll: {first_row} -> {last_row}, duration {(times[-1] - times[0]) / 1000:.2f}s\n"
                        f"rAF interval p95: {p95:.2f} ms from {len(intervals)} intervals\n```\n",
                        encoding="utf-8", newline="\n",
                    )
                    print(f"Chrome scroll rAF interval p95: {p95:.2f} ms", flush=True)
                    print(f"Chrome {browser.version}: WASM HTTP 200. Workspace {workspace_id}. Logs {before} -> {after}.", flush=True)
        finally:
            server.shutdown()
            thread.join()
