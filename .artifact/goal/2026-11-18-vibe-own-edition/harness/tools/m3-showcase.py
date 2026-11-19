"""Capture the shared NSH design showcase in a Win32 window and Chromium."""

import argparse
from collections import Counter
from contextlib import contextmanager
import ctypes
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import subprocess
from threading import Thread
from time import monotonic, sleep
import winreg

from PIL import Image

FIXTURES = (
    "light-comfortable", "light-compact", "dark-comfortable", "dark-compact",
    "amber-light", "amber-dark", "narrow", "empty-reduced",
)


def capture_win32(target, output):
    import win32gui
    import win32process
    import win32ui

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    user32.SetProcessDPIAware()
    user32.GetDpiForWindow.argtypes = [ctypes.c_void_p]
    user32.PrintWindow.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint]
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Accessibility") as key:
            text_scale, _ = winreg.QueryValueEx(key, "TextScaleFactor")
    except FileNotFoundError:
        text_scale = 100
    animations = ctypes.c_int()
    if not user32.SystemParametersInfoW(0x1042, 0, ctypes.byref(animations), 0):
        raise ctypes.WinError(ctypes.get_last_error())
    print(f"Win32 text scale {text_scale}%; system animations {'on' if animations.value else 'off'}", flush=True)
    for name in FIXTURES:
        title = f"vibe-own | M3 {name}"
        with (target / f"showcase-native-{name}.log").open("wb") as log:
            process = subprocess.Popen([target / "debug/vibe-own.exe", name], stderr=log)
            hwnd = 0
            try:
                deadline = monotonic() + 15
                while monotonic() < deadline:
                    hwnd = win32gui.FindWindow(None, title)
                    if hwnd and win32process.GetWindowThreadProcessId(hwnd)[1] == process.pid:
                        break
                    if process.poll() is not None:
                        raise RuntimeError(f"{name}: exit {process.returncode}; {log.name}")
                    sleep(0.05)
                else:
                    raise RuntimeError(f"Window not mounted: {title}; {log.name}")
                sleep(0.65)
                _, _, width, height = win32gui.GetClientRect(hwnd)
                dpi = user32.GetDpiForWindow(hwnd)
                hdc = win32gui.GetDC(hwnd)
                dc = win32ui.CreateDCFromHandle(hdc)
                memory = dc.CreateCompatibleDC()
                bitmap = win32ui.CreateBitmap()
                bitmap.CreateCompatibleBitmap(dc, width, height)
                memory.SelectObject(bitmap)
                try:
                    if not user32.PrintWindow(hwnd, memory.GetSafeHdc(), 3):
                        raise ctypes.WinError(ctypes.get_last_error())
                    image = Image.frombytes("RGB", (width, height), bitmap.GetBitmapBits(True), "raw", "BGRX")
                    path = output / f"win-{name}.png"
                    save_frame(image, path)
                    print(f"{path.name} {width}x{height} device pixels at {dpi} DPI", flush=True)
                finally:
                    memory.DeleteDC()
                    win32gui.DeleteObject(bitmap.GetHandle())
                    win32gui.ReleaseDC(hwnd, hdc)
            finally:
                if process.poll() is None and hwnd and win32gui.IsWindow(hwnd):
                    if win32process.GetWindowThreadProcessId(hwnd)[1] == process.pid:
                        win32gui.PostMessage(hwnd, 0x0010, 0, 0)  # WM_CLOSE
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=20)


@contextmanager
def static_site(directory):
    handler = partial(SimpleHTTPRequestHandler, directory=directory)
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


def save_frame(image, path):
    counts = Counter(image.getdata())
    if len(counts) < 100:
        raise RuntimeError(f"A rendered showcase is expected in {path.name}; got {len(counts)} colors")
    image.save(path)


def capture_web(target, output):
    from playwright.sync_api import expect, sync_playwright

    with static_site(target / "showcase-site") as origin, sync_playwright() as playwright:
        browser = playwright.chromium.launch(channel="chrome", headless=True)
        print(f"Chromium {browser.version}", flush=True)
        for name in FIXTURES:
            size = (900, 720) if name == "narrow" else (1280, 800)
            page = browser.new_page(
                viewport=dict(zip(("width", "height"), size)),
                reduced_motion="reduce" if name == "empty-reduced" else "no-preference",
                device_scale_factor=1,
            )
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(f"{origin}/?fixture={name}", wait_until="networkidle")
            expect(page.get_by_text("vibe-own", exact=True)).to_be_visible(timeout=15_000)
            if name.startswith("dark-"):
                expect(page.get_by_text("命令菜单", exact=True)).to_be_visible()
                expect(page.get_by_text("暂无远端配置", exact=True)).to_be_visible()
                expect(page.locator("input:focus")).to_have_count(1)
            elif name == "narrow":
                expect(page.get_by_text("保存失败。工作区当前为只读，输入已保留。", exact=True)).to_be_visible()
            elif name == "empty-reduced":
                expect(page.get_by_text("筛选无结果", exact=True)).to_be_visible()
                assert page.evaluate("matchMedia('(prefers-reduced-motion: reduce)').matches")
            else:
                expect(page.get_by_text("工程看板", exact=True)).to_be_visible()
                expect(page.get_by_text("修复任务重试后的状态漂移", exact=True).first).to_be_visible()
            if errors:
                raise RuntimeError(f"{name}: {errors}")
            page.evaluate("document.fonts.ready")
            path = output / f"web-{name}.png"
            page.screenshot(path=path)
            with Image.open(path) as frame:
                if len(Counter(frame.getdata())) < 100:
                    raise RuntimeError(f"Chromium rendered an empty frame in {path.name}")
            print(f"{path.name} {size[0]}x{size[1]} CSS pixels at DPR {page.evaluate('devicePixelRatio')}", flush=True)
            if name == "light-comfortable":
                check_browser_inputs(page)
            page.close()
        browser.close()


def check_browser_inputs(page):
    from playwright.sync_api import expect

    page.get_by_text("舒适", exact=True).click()
    expect(page.get_by_text("紧凑", exact=True)).to_be_visible()
    page.get_by_text("仅我负责", exact=True).last.click()
    expect(page.get_by_text("筛选无结果", exact=True)).to_be_visible()
    page.get_by_text("清除筛选", exact=True).click()
    page.keyboard.press("Control+k")
    expect(page.get_by_text("命令菜单", exact=True)).to_be_visible()
    search = page.locator("input:focus")
    search.fill("retry")
    expect(page.get_by_text("打开议题 VO-014", exact=True)).to_be_visible()
    search.press("Enter")
    expect(page.get_by_text("命令菜单", exact=True)).to_have_count(0)
    title_input = page.locator("input").first
    title_input.fill("保留的中文输入")
    page.get_by_text("保存标题", exact=True).click()
    expect(title_input).to_have_value("保留的中文输入")
    expect(page.get_by_text("保存失败。工作区当前为只读，输入已保留。", exact=True)).to_be_visible()
    print("Chromium checks passed: density, empty filter, Ctrl+K, query, Enter, and inline save error.", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("windows", "web", "both"))
    parser.add_argument("target", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.mode != "web":
        capture_win32(args.target, args.output)
    if args.mode != "windows":
        capture_web(args.target, args.output)


if __name__ == "__main__":
    main()
