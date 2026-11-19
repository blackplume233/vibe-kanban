"""Verify the generated Windows runner and Web site against one real, isolated API.

Requires Chrome, Python playwright/psutil/pywin32/pywinauto/Pillow, Node and the local-web
Vite installation. Run with --help for paths. Output includes screenshots and
Windows application logs. The isolated fixture remains in the evidence directory.
"""

import argparse
from base64 import b64encode
from contextlib import closing, contextmanager
import ctypes
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
from tempfile import TemporaryDirectory
from time import monotonic, sleep
from urllib.error import URLError
from urllib.parse import urlsplit
from urllib.request import Request, urlopen
from uuid import UUID

import psutil
import win32gui
import win32process
import win32ui
from PIL import Image
from playwright.sync_api import expect, sync_playwright
from pywinauto import Desktop


WORKSPACE = "M7 packaged workspace"
START_PROXY = """
const {preview} = await import(process.env.M7_VITE);
const server = await preview({
    configFile: false,
    root: process.env.M7_SITE_PARENT,
    build: {outDir: process.env.M7_SITE_NAME},
    preview: {
        host: '127.0.0.1',
        port: 0,
        proxy: {'/api': {target: process.env.M7_BACKEND, ws: true}},
    },
});
console.log(`http://127.0.0.1:${server.httpServer.address().port}`);
"""


def until(check, description, timeout=30):
    deadline = monotonic() + timeout
    while monotonic() < deadline:
        result = check()
        if result:
            return result
        sleep(0.1)
    raise AssertionError(f"Timed out waiting for {description}")


def api(base, path, method="GET", payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    request = Request(base + "/api" + path, data, {"Content-Type": "application/json"}, method=method)
    with urlopen(request, timeout=25) as response:
        result = json.load(response)
        assert result["success"], result
        print(f"HTTP {response.status} {method} {path}", flush=True)
        return result.get("data")


def stop(process):
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=10)


@contextmanager
def runner(executable, profile, log_path):
    env = os.environ.copy()
    for key in ("VK_SHARED_API_BASE", "VK_SHARED_RELAY_API_BASE", "VK_ALLOWED_ORIGINS", "BACKEND_PORT"):
        env.pop(key, None)
    # NSH_APP_DIR points at the runner's own packaged assets and enables its
    # stderr log. The package, native window, and server remain unmodified.
    env.update(VK_ASSET_DIR=str(profile / "data"), TEMP=str(profile), TMP=str(profile),
               NSH_APP_DIR=str(executable.parent / "nsh"))
    with log_path.open("wb") as log:
        process = subprocess.Popen([str(executable)], env=env, cwd=profile,
                                   stdout=log, stderr=log)
        try:
            yield process
        finally:
            stop(process)


def capture_window(hwnd, target):
    left, top, right, bottom = win32gui.GetWindowRect(hwnd)
    size = (right - left, bottom - top)
    handle = win32gui.GetWindowDC(hwnd)
    window = win32ui.CreateDCFromHandle(handle)
    memory = window.CreateCompatibleDC()
    bitmap = win32ui.CreateBitmap()
    bitmap.CreateCompatibleBitmap(window, *size)
    memory.SelectObject(bitmap)
    try:
        capture = ctypes.windll.user32.PrintWindow
        capture.argtypes = (ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint)
        assert capture(hwnd, memory.GetSafeHdc(), 2), "PrintWindow failed"
        image = Image.frombytes('RGB', size, bitmap.GetBitmapBits(True), 'raw', 'BGRX', 0, 1)
        assert image.getbbox(), "Win32 window capture is empty"
        image.save(target)
    finally:
        memory.DeleteDC()
        win32gui.DeleteObject(bitmap.GetHandle())
        win32gui.ReleaseDC(hwnd, handle)


def live_runner(process):
    assert process.poll() is None, f"Packaged runner exited: {process.returncode}"
    windows = []

    def collect(hwnd, _):
        owner = win32process.GetWindowThreadProcessId(hwnd)[1]
        if owner == process.pid and win32gui.IsWindowVisible(hwnd) and win32gui.GetWindowText(hwnd) == "vibe-own":
            windows.append(hwnd)
        return True

    win32gui.EnumWindows(collect, None)
    if not windows:
        return None
    listeners = [item.laddr for item in psutil.Process(process.pid).net_connections(kind="tcp")
                 if item.status == psutil.CONN_LISTEN]
    assert all(address.ip in {"127.0.0.1", "::1"} for address in listeners), listeners
    for address in listeners:
        base = f"http://127.0.0.1:{address.port}"
        try:
            with urlopen(base + "/api/workspaces", timeout=1) as response:
                assert json.load(response)["success"]
                return windows[0], base
        except (URLError, TimeoutError, ValueError):
            pass
    return None


def open_native_changes(hwnd):
    window = Desktop(backend="uia").window(handle=hwnd)

    def click(title, control_type=None):
        control = window.child_window(title=title, control_type=control_type)
        control.wait("visible enabled", timeout=15)
        center = control.rectangle().mid_point()
        x, y = win32gui.ScreenToClient(hwnd, (center.x, center.y))
        point = ((y & 0xFFFF) << 16) | (x & 0xFFFF)
        win32gui.SendMessage(hwnd, 0x0201, 1, point)  # WM_LBUTTONDOWN
        win32gui.SendMessage(hwnd, 0x0202, 0, point)  # WM_LBUTTONUP

    click("工作区", "Button")
    click("工作区", "ComboBox")
    click(WORKSPACE, "ListItem")
    click("日志与变更", "Button")


def close_window(process, hwnd, base):
    win32gui.PostMessage(hwnd, 0x0010, 0, 0)  # WM_CLOSE
    assert process.wait(timeout=15) == 0, process.returncode
    try:
        urlopen(base + "/api/workspaces", timeout=1).close()
    except URLError:
        print(f"Win32 PID {process.pid} exited cleanly; {base} closed", flush=True)
    else:
        raise AssertionError(f"Server accepted a request after Win32 exit: {base}")


def fixture(base, folder):
    repo = folder / "fixture-repo"
    repo.mkdir()
    (repo / "review.txt").write_text("status=planned\n", encoding="ascii")
    for args in (("init", "--initial-branch=main"), ("add", "review.txt"),
                 ("-c", "user.name=M7 smoke", "-c", "user.email=m7@example.invalid", "commit", "-m", "Base")):
        subprocess.run(["git", "-C", str(repo), *args], check=True, stdout=subprocess.DEVNULL)
    repo_id = api(base, "/repos", "POST", {"path": str(repo), "display_name": "M7 fixture"})["id"]
    heartbeats = folder / "worker-tick.txt"
    script = f"""
Set-Content -Path review.txt -Value 'status=packaged' -Encoding ascii
for ($i=0; $i -lt 180; $i++) {{
    Set-Content -LiteralPath '{str(heartbeats).replace("'", "''")}' -Value $i
    Write-Output ('M7 live tick {{0:000}}' -f $i)
    Start-Sleep -Milliseconds 500
}}
"""
    encoded = b64encode(script.encode("utf-16-le")).decode("ascii")
    api(base, f"/repos/{repo_id}", "PUT", {
        "dev_server_script": f"powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}"
    })
    workspace = api(base, "/workspaces", "POST", {"name": WORKSPACE})["id"]
    api(base, f"/workspaces/{workspace}/repos", "POST", {"repo_id": repo_id, "target_branch": "main"})
    return workspace, heartbeats


@contextmanager
def proxy(site, vite, base):
    env = dict(os.environ, M7_SITE_PARENT=str(site.parent), M7_SITE_NAME=site.name,
               M7_VITE=vite.as_uri(), M7_BACKEND=base)
    process = subprocess.Popen(["node", "--input-type=module", "-e", START_PROXY], env=env,
                               stdout=subprocess.PIPE, text=True)
    try:
        line = process.stdout.readline().strip()
        assert line.startswith("http://127.0.0.1:"), line
        yield line
    finally:
        stop(process)


def browser_smoke(args, base, workspace, execution):
    with proxy(args.site, args.vite, base) as origin, sync_playwright() as playwright:
        with playwright.chromium.launch(channel="chrome", headless=True) as browser:
            page = browser.new_page(viewport={"width": 1080, "height": 760})
            errors, sockets, responses = [], set(), []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("websocket", lambda socket: sockets.add(socket.url))
            page.on("response", lambda response: responses.append(response))
            page.goto(origin)
            page.wait_for_load_state("networkidle")
            expect(page).to_have_title("vibe-own")
            page.get_by_role("button", name="工作区", exact=True).first.click(timeout=15_000)
            page.get_by_text("选择工作区", exact=True).click()
            page.get_by_text(WORKSPACE, exact=True).click()
            page.get_by_role("button", name="日志与变更", exact=True).click()
            expect(page.get_by_text(re.compile(r"(?:^|/)review\.txt$")).first).to_be_visible(timeout=15_000)
            expect(page.get_by_text("status=packaged", exact=False)).to_be_visible()
            expect(page.get_by_text(re.compile(r"M7 live tick \d{3}")).first).to_be_visible()
            ws = origin.replace("http", "ws", 1)
            required = {f"{ws}/api/workspaces/{workspace}/git/diff/ws",
                        f"{ws}/api/execution-processes/stream/session/ws?session_id=",
                        f"{ws}/api/execution-processes/{execution}/raw-logs/ws"}
            assert all(any(url.startswith(prefix) for url in sockets) for prefix in required), sockets
            assert all(urlsplit(url).netloc == urlsplit(origin).netloc for url in sockets), sockets
            assert any(response.ok and response.url.endswith(".wasm") for response in responses)
            for path in ("/api/workspaces", f"/api/sessions?workspace_id={workspace}"):
                assert any(response.ok and response.url == origin + path for response in responses), path
            assert not errors, errors
            page.screenshot(path=args.evidence / "m7-browser.png")
            print(f"Chrome {browser.version}: packaged WASM, HTTP and {len(sockets)} same-origin WebSockets on {origin}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("exe", "site", "vite", "evidence"):
        parser.add_argument(f"--{name}", type=lambda value: Path(value).resolve(), required=True)
    args = parser.parse_args()
    args.evidence.mkdir(parents=True, exist_ok=True)
    ctypes.windll.user32.SetProcessDPIAware()
    with TemporaryDirectory(prefix="vibe-own-m7-", dir=args.evidence, delete=False) as directory:
        profile = Path(directory)
        with runner(args.exe, profile, args.evidence / "m7-native-seed.log") as process:
            hwnd, base = until(lambda: live_runner(process), "first Win32 window and API")
            database = profile / "data/db.v2.sqlite"
            assert database.is_file(), f"Missing isolated database: {database}"
            assert api(base, "/workspaces") == [], "New profile contains existing workspaces"
            workspace, heartbeats = fixture(base, profile)
            close_window(process, hwnd, base)
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
            assert connection.execute("SELECT id FROM workspaces").fetchall() == [(UUID(workspace).bytes,)]
        paths = subprocess.check_output(
            ["git", "-C", str(profile / "fixture-repo"), "worktree", "list", "--porcelain"],
            text=True,
        ).splitlines()
        worktrees = [Path(path.removeprefix("worktree ")).resolve() for path in paths if path.startswith("worktree ")]
        assert len(worktrees) == 2 and all(path.is_relative_to(profile) for path in worktrees), worktrees
        print(f"Isolated SQLite: {database}; worktree: {worktrees[1]}", flush=True)

        log = args.evidence / "m7-native.log"
        with runner(args.exe, profile, log) as process:
            hwnd, base = until(lambda: live_runner(process), "Win32 window and in-process API")
            assert any(row["id"] == workspace for row in api(base, "/workspaces"))
            execution = api(base, f"/workspaces/{workspace}/execution/dev-server/start", "POST")[0]["id"]

            def logged(fragment):
                return fragment in log.read_text(encoding="utf-8", errors="replace")

            until(lambda: logged("HTTP 200 /api/workspaces"), "Win32 workspaces HTTP request")
            open_native_changes(hwnd)
            until(lambda: logged(f"diff {workspace}: 1 changed files"), "Win32 real diff stream")
            until(lambda: logged(f"execution {execution}: 1 live log entries"), "Win32 real log stream")
            try:
                capture_window(hwnd, args.evidence / "m7-native.png")
            except (AssertionError, OSError, win32ui.error) as error:
                print(f"Win32 window capture unavailable: {error}", flush=True)
            browser_smoke(args, base, workspace, execution)
            assert not logged(f"execution {execution}: raw log stream finished"), "Worker already finished"
            assert heartbeats.exists(), "Missing worker heartbeat"
            close_window(process, hwnd, base)
            pulse = heartbeats.read_bytes()
            sleep(1.25)
            assert heartbeats.read_bytes() == pulse, "Worker kept running after window closed"
            print(f"Win32 window closed with live execution {execution}; worker heartbeat stopped", flush=True)


if __name__ == "__main__":
    main()
