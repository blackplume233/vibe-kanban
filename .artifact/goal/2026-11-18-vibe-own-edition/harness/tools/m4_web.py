"""使用 Playwright 操作 M4 WASM 界面并记录 Chromium 的实际议题列表滚动。"""

from contextlib import ExitStack, contextmanager
from functools import partial
from http.client import HTTPConnection
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from math import ceil
import re
from threading import Thread
from time import monotonic
from urllib.parse import urlencode, urlsplit

from playwright.sync_api import expect, sync_playwright

VIEWPORT = {"width": 1360, "height": 900}
WHEEL_EVENTS = 100
RENDER_EVENTS = {"UpdateLayoutTree", "Layout", "PrePaint", "Paint", "Layerize"}


@contextmanager
def site_server(site, port, backend, log):
    class Handler(SimpleHTTPRequestHandler):
        extensions_map = {**SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm"}

        def log_message(self, fmt, *args):
            print(fmt % args, file=log, flush=True)

        def do_GET(self):
            if urlsplit(self.path).path.startswith("/api/"):
                self.proxy_api()
            else:
                super().do_GET()

        def do_POST(self):
            self.proxy_api()

        def do_PATCH(self):
            self.proxy_api()

        def proxy_api(self):
            if not urlsplit(self.path).path.startswith("/api/"):
                self.send_error(404)
                return
            headers = {name: self.headers[name] for name in ("Origin", "Content-Type") if name in self.headers}
            size = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(size) if size else None
            conn = HTTPConnection("127.0.0.1", urlsplit(backend).port, timeout=60)
            try:
                conn.request(self.command, self.path, body=body, headers=headers)
                response = conn.getresponse()
                data = response.read()
                self.send_response(response.status)
                if content_type := response.getheader("Content-Type"):
                    self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            finally:
                conn.close()

    with ThreadingHTTPServer(("127.0.0.1", port), partial(Handler, directory=site)) as server:
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield f"http://127.0.0.1:{port}/?backend_port={urlsplit(backend).port}"
        finally:
            server.shutdown()
            thread.join()


def button(page, name):
    return page.get_by_role("button", name=name, exact=True)


def row(page, issue):
    return button(page, f"{issue['simple_id']} {issue['title']}")


def detail(page, issue, status=None):
    expect(button(page, "返回")).to_be_visible()
    expect(page.get_by_text(issue["title"], exact=True).last).to_be_visible()
    if status is not None:
        expect(button(page, f"状态 · {status}")).to_be_visible()


def open_ready(page, url, first, *, verify_input=False):
    wasm = []
    page.on("response", lambda r: wasm.append(r) if urlsplit(r.url).path.endswith(".wasm") else None)
    page.goto(url, wait_until="commit")
    expect(button(page, "议题")).to_be_visible()
    expect(button(page, "列表")).to_be_visible()
    page.wait_for_function("label => [...document.querySelectorAll('[aria-label]')].some(element => "
                           "element.getAttribute('aria-label') === label && "
                           "element.getBoundingClientRect().height > 0)",
                           arg=f"{first['simple_id']} {first['title']}")
    if verify_input:
        page.keyboard.press("ArrowDown")
        page.keyboard.press("Enter")
        detail(page, first)
    elapsed = page.evaluate("""() => new Promise(resolve => requestAnimationFrame(
        () => requestAnimationFrame(() => resolve(performance.now()))
    ))""")
    assert wasm and all(r.ok and r.headers.get("content-type", "").startswith("application/wasm")
                        for r in wasm), "缺少成功的 Release WASM 响应"
    return elapsed


def choose(page, query, result, *, issue=False):
    shortcut = page.evaluate("navigator.platform.includes('Mac') ? 'Meta+k' : 'Control+k'")
    page.keyboard.press(shortcut)
    search = page.get_by_placeholder("输入议题短 ID、项目、工作区或命令", exact=True)
    expect(search).to_be_focused()
    search.fill(query)
    name = re.compile(r"^" + re.escape(result["simple_id"]) + r"\s+" + re.escape(result["title"]) + r"$") if issue else result
    option = page.get_by_role("option", name=name, exact=True)
    expect(option).to_be_visible()
    search.press("Enter")
    expect(search).to_be_hidden()


def mutation(page, action, method, path):
    with page.expect_response(lambda res: res.request.method == method and urlsplit(res.url).path == path,
                              timeout=70_000) as event:
        action()
    response = event.value
    payload = response.json()
    assert response.ok and payload["success"], f"{method} {path}: {response.status} {payload}"
    return payload["data"]


def smoke(page, url, fixture, read_api, shot):
    base = f"/projects/{fixture['project']['id']}/issues"
    named = fixture["named"]
    first, second = named["keyboard_first"], named["keyboard_second"]
    issue = named["query"]
    inbox, started = fixture["statuses"]
    cold = open_ready(page, url, first, verify_input=True)
    shot("01-keyboard-first", f"冷启动并用方向键和 Enter 打开 {first['simple_id']}，{cold:.1f} ms")
    page.keyboard.press("Escape")
    expect(button(page, "返回")).to_be_hidden()
    page.keyboard.press("ArrowDown")
    page.keyboard.press("Enter")
    detail(page, second)
    shot("02-keyboard-neighbor", f"Escape 返回，再用方向键打开 {second['simple_id']}")
    page.keyboard.press("Escape")

    choose(page, "plrs", fixture["project"]["name"])
    expect(row(page, first)).to_be_visible()
    choose(page, "dckyrd", fixture["workspace"]["name"])
    expect(page.get_by_text(fixture["workspace"]["name"], exact=True).last).to_be_visible()
    shot("03-workspace-query", "用模糊查询打开项目和工作区")
    button(page, "议题").click()
    choose(page, "cmt cmps", issue, issue=True)
    detail(page, issue, inbox["name"])
    shot("04-issue-query", f"模糊查询并打开 {issue['simple_id']}")
    page.keyboard.press("Escape")

    expect(button(page, "新建议题")).to_be_visible()
    page.keyboard.press("c")
    field = page.get_by_placeholder("议题标题", exact=True)
    title = "M4 Created invoice"
    field.fill(title)
    shot("05-create", "在新建议题表单输入标题")
    created = mutation(page, button(page, "保存").click, "POST", "/api" + base)
    expect(field).to_be_hidden()
    persisted = read_api(base + "?" + urlencode({"search": title}))
    assert [item["id"] for item in persisted] == [created["id"]]
    choose(page, "crtd nvc", created, issue=True)
    detail(page, created, inbox["name"])
    shot("06-created", f"已从 SQLite 读取新建议题 {created['simple_id']}")
    page.keyboard.press("Escape")

    choose(page, "cmt cmps", issue, issue=True)
    expect(button(page, "编辑标题")).to_be_visible()
    page.keyboard.press("e")
    expect(field).to_have_value(issue["title"])
    edited_title = "M4 Comet compass edited"
    field.fill(edited_title)
    shot("07-edit", "在详情面板编辑标题")
    issue_path = f"{base}/{issue['id']}"
    edited = mutation(page, button(page, "保存").click, "PATCH", "/api" + issue_path)
    detail(page, edited, inbox["name"])
    assert edited["title"] == read_api(issue_path)["title"] == edited_title
    shot("08-edited", f"UI 与 SQLite 均显示 {edited_title}")
    page.keyboard.press("Escape")

    page.keyboard.press("Shift+F10")
    page.get_by_role("option", name="移到状态", exact=True).click()
    destination = page.get_by_role("option", name=started["name"], exact=True)
    mutation(page, destination.click, "PATCH", "/api" + issue_path)
    assert read_api(issue_path)["status_id"] == started["id"]
    choose(page, "cmt cmps", edited, issue=True)
    detail(page, edited, started["name"])
    shot("09-move-menu", "通过快捷键打开议题操作并跨状态移动")
    page.keyboard.press("Escape")

    button(page, "看板").click()
    expect(page.get_by_text(inbox["name"], exact=True)).to_be_visible()
    expect(page.get_by_text(started["name"], exact=True)).to_be_visible()
    grip = row(page, named["drag"]).locator('[aria-label="拖动议题"]')
    anchor = row(page, named["started"])
    expect(grip).to_be_visible()
    expect(anchor).to_be_visible()
    source, target = grip.bounding_box(), anchor.bounding_box()
    shot("10-board-before", "拖拽前，可见两个状态列")

    def drag():
        page.mouse.move(source["x"] + source["width"] / 2, source["y"] + source["height"] / 2)
        page.mouse.down()
        page.mouse.move(target["x"] + target["width"] / 2, target["y"] + target["height"] / 2, steps=20)
        page.mouse.up()

    drag_path = f"{base}/{named['drag']['id']}"
    mutation(page, drag, "PATCH", "/api" + drag_path)
    assert read_api(drag_path)["status_id"] == started["id"]
    moved = row(page, named["drag"])
    expect(moved).to_be_visible()
    assert abs(moved.bounding_box()["x"] - anchor.bounding_box()["x"]) < 2, "卡片没有移入目标列"
    shot("11-board-after", "用真实指针拖拽并从 SQLite 核对目标状态")
    button(page, "列表").click()
    expect(row(page, first)).to_be_visible()
    row(page, first).click(button="right")
    expect(page.get_by_role("option", name="移到状态", exact=True)).to_be_visible()
    button(page, "关闭").click()

    choose(page, "cmt cmps", edited, issue=True)
    mutation(page, button(page, "移入待分诊").click, "PATCH", "/api" + issue_path)
    assert read_api(issue_path)["triage"]
    page.keyboard.press("Escape")
    button(page, "待分诊").click()
    expect(row(page, edited)).to_be_visible()
    expect(row(page, first)).to_be_hidden()

    cycle_name = "M4 Verification cycle"
    button(page, "新建周期").click()
    page.get_by_placeholder("周期名称", exact=True).fill(cycle_name)
    page.get_by_placeholder("开始日期 YYYY-MM-DD", exact=True).fill("2026-11-20")
    page.get_by_placeholder("结束日期 YYYY-MM-DD", exact=True).fill("2026-11-27")
    project_path = f"/projects/{fixture['project']['id']}"
    cycle = mutation(page, button(page, "保存").click, "POST", "/api" + project_path + "/cycles")
    row(page, edited).click()
    mutation(page, button(page, cycle_name).last.click, "PATCH", "/api" + issue_path)
    assert read_api(issue_path)["cycle_id"] == cycle["id"]
    page.keyboard.press("Escape")
    button(page, cycle_name).click()
    expect(row(page, edited)).to_be_visible()
    expect(row(page, first)).to_be_hidden()

    view_name = "M4 Cycle view"
    button(page, "保存当前视图").click()
    view_field = page.get_by_placeholder("视图名称", exact=True)
    view_field.fill(view_name)
    saved = mutation(page, lambda: view_field.press("Enter"), "POST", "/api" + project_path + "/saved-views")
    assert saved["filter"] == {"cycle_id": cycle["id"]}
    button(page, "全部议题").click()
    expect(row(page, first)).to_be_visible()
    button(page, view_name).click()
    expect(row(page, edited)).to_be_visible()
    expect(row(page, first)).to_be_hidden()
    shot("12-cycle-view", "右键菜单、分诊、周期分配及保存视图均已验证")
    return cold


def percentile(values, rank):
    return sorted(values)[ceil(rank * len(values)) - 1]


def visible_rows(page):
    return page.locator('[role="button"][aria-label^="VO-"]').evaluate_all("""rows => rows
        .map(row => [row.getBoundingClientRect(), row.getAttribute('aria-label')])
        .filter(([r]) => r.height > 0 && r.width > 0 && r.top >= 100 && r.bottom <= innerHeight - 34)
        .sort(([a], [b]) => a.top - b.top)
        .map(([, label]) => label)""")


def wheel(page):
    for _ in range(WHEEL_EVENTS):
        page.mouse.wheel(0, 132)
        page.wait_for_timeout(50)


def benchmark(page, url, first, shot, out, write_json):
    open_ready(page, url, first)
    box = row(page, first).bounding_box()
    assert box is not None, "未找到实际议题列表的滚轮输入区域"
    page.mouse.move(box["x"] + box["width"] / 2, box["y"] + box["height"] / 2)
    before = visible_rows(page)
    assert before, "滚动前未呈现实际议题"
    shot("12-scroll-before", "以实际议题列表为滚动目标")
    page.evaluate("""() => {
        const sample = window.m4Raf = {times: [], id: 0};
        function step(time) { sample.times.push(time); sample.id = requestAnimationFrame(step); }
        sample.id = requestAnimationFrame(step);
    }""")
    wheel(page)
    times = page.evaluate("""() => { cancelAnimationFrame(m4Raf.id); return m4Raf.times; }""")
    after = visible_rows(page)
    assert after and after[0] != before[0], "rAF 采样期间议题列表未滚动"
    intervals = [b - a for a, b in zip(times, times[1:])]
    assert len(intervals) >= 60, "滚动期间 rAF 样本不足"
    raf = {
        "timestamps_ms": times, "first_row_before": before[0], "first_row_after": after[0],
        "p50_ms": percentile(intervals, 0.50), "p95_ms": percentile(intervals, 0.95),
        "max_ms": max(intervals), "wheel_events": WHEEL_EVENTS,
    }
    write_json(out / "web-scroll-raf.json", raf)
    shot("13-scroll-after-raf", f"实际议题滚动 rAF p95 {raf['p95_ms']:.2f} ms")

    # 另采一次滚动，避免 Chromium 的 tracing 开销进入 rAF 样本。
    cdp = page.context.new_cdp_session(page)
    events, completed = [], []
    cdp.on("Tracing.dataCollected", lambda batch: events.extend(batch["value"]))
    cdp.on("Tracing.tracingComplete", lambda event: completed.append(event))
    cdp.send("Tracing.start", {"categories": "devtools.timeline", "transferMode": "ReportEvents"})
    try:
        wheel(page)
    finally:
        cdp.send("Tracing.end")
    deadline = monotonic() + 15
    while not completed and monotonic() < deadline:
        page.wait_for_timeout(50)
    cdp.detach()
    final = visible_rows(page)
    assert final and final[0] != after[0], "Chromium 渲染采样期间议题列表未滚动"
    assert completed and not completed[0].get("dataLossOccurred"), "Chromium tracing 不完整"
    write_json(out / "web-scroll-trace.json", {"traceEvents": events})
    rendering = {}
    for name in sorted(RENDER_EVENTS):
        durations = [ev["dur"] / 1000 for ev in events if ev.get("ph") == "X" and ev.get("name") == name]
        if durations:
            rendering[name] = {"events": len(durations), "p95_ms": percentile(durations, 0.95),
                               "total_ms": sum(durations)}
    started, frames = {}, []
    for event in events:
        if event["name"] != "AnimationFrame::Render":
            continue
        key = event["pid"], event["tid"], event["id2"]["local"]
        if event["ph"] == "b":
            started[key] = event["ts"]
        elif event["ph"] == "e" and key in started:
            frames.append((event["ts"] - started.pop(key)) / 1000)
    assert frames, "Chromium trace did not capture AnimationFrame::Render slices"
    rendering["AnimationFrame::Render"] = {"events": len(frames), "p95_ms": percentile(frames, 0.95),
                                          "total_ms": sum(frames)}
    assert rendering, "Chromium trace 未捕获到渲染事件"
    shot("14-scroll-after-trace", "Chromium 渲染采样完成")
    return {
        "raf": {key: value for key, value in raf.items() if key != "timestamps_ms"},
        "raf_intervals": len(intervals), "trace_rendering": rendering,
        "trace_first_row_before": after[0], "trace_first_row_after": final[0],
    }


def capture(args, backend, fixture, api, write_json):
    read_api = partial(api, backend)
    out = args.out
    with ExitStack() as stack:
        proxy_log = stack.enter_context((out / "web-proxy.log").open("w", encoding="utf-8"))
        step_log = stack.enter_context((out / "web-steps.log").open("w", encoding="utf-8"))
        url = backend if args.same_origin else stack.enter_context(
            site_server(args.site, args.web_port, backend, proxy_log)
        )
        playwright = stack.enter_context(sync_playwright())
        with playwright.chromium.launch(channel=args.channel, headless=True) as browser:
            errors = []

            def run(flow):
                with browser.new_context(viewport=VIEWPORT) as context:
                    page = context.new_page()
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    page.on("console", lambda msg: print(f"Chromium {msg.type}: {msg.text}", file=step_log, flush=True)
                            if msg.type in ("error", "warning") else None)

                    def shot(name, message):
                        page.screenshot(path=out / f"web-{name}.png", timeout=5000)
                        print(f"{name} {message}", file=step_log, flush=True)

                    try:
                        return flow(page, shot)
                    except Exception:
                        try:
                            shot("failure", "步骤异常")
                        except Exception:
                            pass
                        raise

            cold = run(lambda page, shot: smoke(page, url, fixture, read_api, shot))
            total = len(read_api(f"/projects/{fixture['project']['id']}/issues"))
            assert total == fixture["total_issues"] + 1
            perf = run(lambda page, shot: benchmark(page, url, fixture["named"]["keyboard_first"], shot, out, write_json))
            assert not errors, errors
            results = {
                **perf, "browser": browser.version, "headless": True, "viewport": VIEWPORT,
                "seeded_issues": fixture["total_issues"], "benchmark_issues": total,
                "cold_to_usable_ms": cold,
                "budgets": {"web_cold_start_2000_ms": cold <= 2000,
                            "web_render_frame_p95_16_ms": perf["trace_rendering"]["AnimationFrame::Render"]["p95_ms"] <= 16},
            }
            write_json(out / "web-results.json", results)
            return results
