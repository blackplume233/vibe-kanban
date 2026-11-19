"""使用 Playwright 操作 M4 WASM 界面并记录 Chromium 的实际议题列表滚动。

默认使用 headless。用 M4_WEB_HEADED=1 在可见、未锁屏的桌面上采集显示反馈；
两次运行可用 m4-runtime.py 的同一个 --channel 保持 Chromium 构建一致。
"""

from contextlib import ExitStack, contextmanager
from functools import partial
from http.client import HTTPConnection
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from math import ceil, floor
from os import getenv
import re
from threading import Thread
from time import monotonic
from urllib.parse import urlencode, urlsplit

from playwright.sync_api import expect, sync_playwright

VIEWPORT = {"width": 1360, "height": 900}
WHEEL_EVENTS = 100
WHEEL_DELTA = 132
WHEEL_PAUSE_MS = 50
SCROLL_SPEED = 2640  # 持续滚动使用约 5 秒，与间歇滚轮的名义速度相同。
TRACE_START = "m4-scroll-start"
TRACE_END = "m4-scroll-end"
TRACE_CATEGORIES = "devtools.timeline,blink.user_timing,benchmark,disabled-by-default-devtools.timeline.frame"
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
    assert response.ok, f"{method} {path}: {response.status} {response.text()}"
    payload = response.json()
    assert payload["success"], f"{method} {path}: {payload}"
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
    git_status = f"/api/workspaces/{fixture['workspace']['id']}/git/status"
    with page.expect_response(lambda response: urlsplit(response.url).path == git_status) as event:
        choose(page, "dckyrd", fixture["workspace"]["name"])
    response = event.value
    payload = response.json()
    assert response.ok and payload["success"] and payload["data"] == [], (response.status, payload)
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
    for placeholder, value in (
        ("周期名称", cycle_name),
        ("开始日期 YYYY-MM-DD", "2026-11-20"),
        ("结束日期 YYYY-MM-DD", "2026-11-27"),
    ):
        field = page.get_by_placeholder(placeholder, exact=True)
        field.click()
        expect(field).to_be_focused()
        field.fill(value)
        expect(field).to_have_value(value)
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
        page.mouse.wheel(0, WHEEL_DELTA)
        page.wait_for_timeout(WHEEL_PAUSE_MS)


def intervals(times):
    return [b - a for a, b in zip(times, times[1:])]


def cadence(gaps, period):
    return {
        "intervals": len(gaps), "p50_ms": percentile(gaps, 0.50),
        "p95_ms": percentile(gaps, 0.95), "max_ms": max(gaps),
        "baseline_period_ms": period, "gap_threshold_ms": 1.5 * period,
        "gaps_at_least_1_5_periods": sum(gap >= 1.5 * period for gap in gaps),
        "estimated_missed_ticks": sum(max(0, floor(gap / period + 0.5) - 1) for gap in gaps),
    }


def sample_raf(page):
    page.bring_to_front()
    page.evaluate("""() => {
        const sample = window.m4Raf = {
            times: [], id: 0, hidden: document.hidden,
            screen: {width: screen.width, height: screen.height, dpr: devicePixelRatio}
        };
        sample.visibilityChanged = () => { sample.hidden ||= document.hidden; };
        document.addEventListener('visibilitychange', sample.visibilityChanged);
        function step(time) { sample.times.push(time); sample.id = requestAnimationFrame(step); }
        sample.id = requestAnimationFrame(step);
    }""")
    wheel(page)
    sample = page.evaluate("""() => {
        const {id, times, screen, hidden, visibilityChanged} = window.m4Raf;
        cancelAnimationFrame(id);
        document.removeEventListener('visibilitychange', visibilityChanged);
        delete window.m4Raf;
        return {times, screen, hidden: hidden || document.hidden};
    }""")
    assert not sample["hidden"], "rAF 采样期间页面不可见"
    assert len(sample["times"]) >= 61, "滚动期间 rAF 样本不足"
    return sample


def clock_baseline(context):
    page = context.new_page()
    try:
        page.set_content("""<!doctype html><style>
            body {margin: 0}
            main {height: 100vh; overflow: auto}
            div {height: 100000px; background: repeating-linear-gradient(
                #eee 0 36px, #aaa 36px 44px)}
        </style><main><div></div></main>""")
        page.mouse.move(VIEWPORT["width"] / 2, VIEWPORT["height"] / 2)
        sample = sample_raf(page)
        offset = page.locator("main").evaluate("element => element.scrollTop")
        assert offset > 0, "显示时钟基线未滚动"
    finally:
        page.close()
    gaps = intervals(sample["times"])
    return {**cadence(gaps, percentile(gaps, 0.50)), "scroll_top_px": offset,
            "timestamps_ms": sample["times"], "screen": sample["screen"]}


def scroll_trace(page, x, y, out, write_json):
    # 与 rAF 分开采样。持续输入避免将 50 ms 的人工轮间停顿误计为丢帧。
    cdp = page.context.new_cdp_session(page)
    events, completed = [], []
    try:
        cdp.on("Tracing.dataCollected", lambda batch: events.extend(batch["value"]))
        cdp.on("Tracing.tracingComplete", lambda event: completed.append(event))
        cdp.send("Tracing.start", {"categories": TRACE_CATEGORIES, "transferMode": "ReportEvents"})
        try:
            page.evaluate("name => { performance.mark(name); }", TRACE_START)
            cdp.send("Input.synthesizeScrollGesture", {
                "x": x, "y": y, "yDistance": -WHEEL_EVENTS * WHEEL_DELTA,
                "speed": SCROLL_SPEED, "gestureSourceType": "mouse", "preventFling": True,
                "interactionMarkerName": "M4IssueListScroll",
            })
            page.evaluate("name => { performance.mark(name); }", TRACE_END)
            page.wait_for_timeout(250)  # 让末尾的呈现反馈进入 trace。
        finally:
            cdp.send("Tracing.end")
        deadline = monotonic() + 15
        while not completed and monotonic() < deadline:
            page.wait_for_timeout(50)
    finally:
        cdp.detach()
    write_json(out / "web-scroll-trace.json", {"traceEvents": events})
    assert completed and not completed[0].get("dataLossOccurred"), "Chromium tracing 不完整"
    return events


def summarize_trace(events, period):
    start, end = (
        [ev for ev in events if ev.get("name") == mark and "blink.user_timing" in ev.get("cat", "").split(",")]
        for mark in (TRACE_START, TRACE_END)
    )
    assert len(start) == len(end) == 1, "Chromium trace 未捕获唯一滚动窗口"
    pid = start[0]["pid"]
    begin, finish = start[0]["ts"], end[0]["ts"]
    assert end[0]["pid"] == pid and finish > begin, "Chromium 滚动窗口无效"
    active = [ev for ev in events if ev.get("pid") == pid and begin <= ev.get("ts", -1) <= finish]
    rendering = {}
    for name in sorted(RENDER_EVENTS | {"AnimationFrame", "AnimationFrame::Render"}):
        started, durations = {}, []
        for event in active:
            if event.get("name") != name:
                continue
            if event["ph"] == "X":
                durations.append(event["dur"] / 1000)
            elif event["ph"] in ("b", "e"):
                ids = event.get("id2", {})
                key = event["tid"], ids.get("local", ids.get("global", event.get("id")))
                if event["ph"] == "b":
                    started[key] = event["ts"]
                elif key in started:
                    durations.append((event["ts"] - started.pop(key)) / 1000)
        if durations:
            rendering[name] = {"events": len(durations), "p95_ms": percentile(durations, 0.95),
                               "total_ms": sum(durations)}
    assert "AnimationFrame::Render" in rendering, "Chromium trace 未捕获 AnimationFrame::Render"

    # Blink 用 PresentationFeedback 的时间戳记录该事件。多个回调可对应同一显示帧。
    presented = sorted({ev["ts"] for ev in active if ev.get("name") == "AnimationFrame::Presentation"})
    presentation = {"events": len(presented), "cadence": None}
    if len(presented) > 1:
        presentation["cadence"] = cadence([gap / 1000 for gap in intervals(presented)], period)
    return {"trace_rendering": rendering, "trace_presentation": presentation,
            "trace_active_window": {"renderer_pid": pid, "start_us": begin, "end_us": finish}}


def benchmark(page, url, first, shot, out, write_json):
    baseline = clock_baseline(page.context)
    write_json(out / "web-scroll-clock.json", baseline)
    page.bring_to_front()
    open_ready(page, url, first)
    box = row(page, first).bounding_box()
    assert box is not None, "未找到实际议题列表的滚轮输入区域"
    x, y = box["x"] + box["width"] / 2, box["y"] + box["height"] / 2
    page.mouse.move(x, y)
    before = visible_rows(page)
    assert before, "滚动前未呈现实际议题"
    shot("12-scroll-before", "以实际议题列表为滚动目标")
    sample = sample_raf(page)
    assert sample["screen"] == baseline["screen"], "基线与产品采样的显示配置不同"
    after = visible_rows(page)
    assert after and after[0] != before[0], "rAF 采样期间议题列表未滚动"
    period = baseline["p50_ms"]
    raf = {
        **cadence(intervals(sample["times"]), period),
        "timestamps_ms": sample["times"], "first_row_before": before[0], "first_row_after": after[0],
        "wheel_events": WHEEL_EVENTS, "wheel_delta_px": WHEEL_DELTA, "wheel_pause_ms": WHEEL_PAUSE_MS,
    }
    write_json(out / "web-scroll-raf.json", raf)
    shot("13-scroll-after-raf", f"实际议题滚动 rAF p95 {raf['p95_ms']:.2f} ms")

    events = scroll_trace(page, x, y, out, write_json)
    final = visible_rows(page)
    assert final and final[0] != after[0], "Chromium 渲染采样期间议题列表未滚动"
    trace = summarize_trace(events, period)
    shot("14-scroll-after-trace", "Chromium 渲染采样完成")
    return {
        "raf": {key: value for key, value in raf.items() if key != "timestamps_ms"},
        "raf_intervals": raf["intervals"],
        "clock_baseline": {key: value for key, value in baseline.items() if key != "timestamps_ms"},
        **trace, "trace_first_row_before": after[0], "trace_first_row_after": final[0],
        "trace_scroll_speed_px_s": SCROLL_SPEED, "trace_scroll_distance_px": WHEEL_EVENTS * WHEEL_DELTA,
    }


def capture(args, backend, fixture, api, write_json):
    read_api = partial(api, backend)
    out = args.out
    headless = getenv("M4_WEB_HEADED") != "1"
    with ExitStack() as stack:
        proxy_log = stack.enter_context((out / "web-proxy.log").open("w", encoding="utf-8"))
        step_log = stack.enter_context((out / "web-steps.log").open("w", encoding="utf-8"))
        url = backend if args.same_origin else stack.enter_context(
            site_server(args.site, args.web_port, backend, proxy_log)
        )
        playwright = stack.enter_context(sync_playwright())
        with playwright.chromium.launch(channel=args.channel, headless=headless) as browser:
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
            assert total == fixture["total_issues"] + 1 and total >= 5000
            perf = run(lambda page, shot: benchmark(page, url, fixture["named"]["keyboard_first"], shot, out, write_json))
            assert not errors, errors
            results = {
                **perf, "browser": browser.version, "channel": args.channel, "headless": headless,
                "viewport": VIEWPORT,
                "seeded_issues": fixture["total_issues"], "benchmark_issues": total,
                "cold_to_usable_ms": cold,
                "web_frame_metric": "Blink AnimationFrame::Render main-thread rendering slice (NSH --fps frame proxy)",
                "web_presentation_metric": "Intervals between distinct Chrome presentation feedback timestamps for Blink animation frames",
                "web_raf_metric": "requestAnimationFrame callback intervals compared to a same-browser native overflow scroller",
                "budgets": {"web_cold_start_2000_ms": cold <= 2000,
                            "web_render_frame_p95_16_ms": perf["trace_rendering"]["AnimationFrame::Render"]["p95_ms"] <= 16},
            }
            write_json(out / "web-results.json", results)
            return results
