"""用本地 Chrome 检验 NSH 的静态 WASM 站点并截图。"""

from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sys
from threading import Thread

from playwright.sync_api import expect, sync_playwright


def main(site: Path, screenshot: Path) -> None:
    handler = partial(SimpleHTTPRequestHandler, directory=site)
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome")
                page = browser.new_page(viewport={"width": 760, "height": 420})
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))
                url = f"http://127.0.0.1:{server.server_port}"
                with page.expect_response(f"{url}/nsh.wasm") as wasm:
                    page.goto(url)
                response = wasm.value
                assert response.ok, f"WASM 响应失败：HTTP {response.status}"
                assert response.headers["content-type"] == "application/wasm"
                page.wait_for_load_state("networkidle")
                expect(page).to_have_title("vibe-own | NSH M1")
                root_text = page.locator("#app").get_by_text("NSH 原生窗口已启动", exact=True)
                expect(root_text).to_be_visible()
                assert not errors, f"页面异常：{errors}"
                page.screenshot(path=screenshot)
                print(
                    f"Web 评测通过：Chrome {browser.version}，WASM HTTP {response.status}，"
                    "根组件可见，无页面异常。"
                )
                browser.close()
        finally:
            server.shutdown()
            thread.join()


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
