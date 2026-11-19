"""Record E3 keyboard acceptance on the packaged NSH Web UI and a real local server.

Requires Windows, Git, Python Playwright, a Chromium installation, and OpenCode
with a preloaded model on a running loopback Ollama. Every run owns its SQLite,
Git repository and worktree. The last product action opens the PR form.
"""

import argparse
from contextlib import ExitStack, suppress
from importlib.util import module_from_spec, spec_from_file_location
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
from time import monotonic
from urllib.parse import urlsplit
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener

from playwright.sync_api import TimeoutError as PlaywrightTimeout, expect, sync_playwright

PROJECT = "E3 Polaris"
INBOX = "E3 Inbox"
STARTED = "E3 In Progress"
REPOSITORY = "E3 local fixture"
FILE = "release-state.txt"
TITLE = f"Edit only {FILE}: replace state=planned with state=verified. Do not commit or push."
VARIANT = "OFFLINE_PROBE"
PROFILE = {"executor": "COPILOT", "variant": VARIANT}
SEARCH = "输入议题短 ID、项目、工作区或命令"
VIEWPORT = {"width": 1360, "height": 900}


class KeyboardGap(RuntimeError):
    pass


class FixtureError(RuntimeError):
    pass


class ProductError(RuntimeError):
    pass


def load_m4(repo):
    path = repo / ".artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m4-runtime.py"
    spec = spec_from_file_location("m4_runtime", path)
    module = module_from_spec(spec)
    previous = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    return module


def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args], text=True, timeout=30).strip()


def prepare(args, write_json):
    try:
        with build_opener(ProxyHandler({})).open(args.ollama + "/api/tags", timeout=5) as res:
            models = {item["name"] for item in json.load(res)["models"]}
    except (URLError, TimeoutError) as exc:
        raise FixtureError(f"Loopback Ollama is unavailable at {args.ollama}") from exc
    if args.model not in models:
        raise FixtureError(f"Preload {args.model} in the local Ollama before running E3")
    data = args.out / "data"
    data.mkdir()
    wrapper = args.out / "e3-local-agent.cmd"
    wrapper.write_text(f'@echo off\n"{args.opencode}" acp --pure\n', encoding="utf-8")
    config = {
        "enabled_providers": ["ollama"],
        "model": f"ollama/{args.model}",
        "autoupdate": False,
        "plugin": [],
        "permission": {"*": "deny", "read": "allow", "edit": "allow", "glob": "allow", "grep": "allow"},
        "provider": {"ollama": {"npm": "@ai-sdk/openai-compatible", "name": "Ollama",
                                "options": {"baseURL": args.ollama + "/v1"},
                                "models": {args.model: {"name": args.model, "tool_call": True}}}},
    }
    config_file = args.out / "e3-opencode.json"
    write_json(config_file, config)
    agent_env = {
        "OPENCODE_CONFIG": str(config_file), "OPENCODE_CONFIG_CONTENT": json.dumps(config),
        "OPENCODE_DISABLE_DEFAULT_PLUGINS": "true", "OPENCODE_DISABLE_MODELS_FETCH": "true",
        "OPENCODE_DISABLE_PROJECT_CONFIG": "true", "OPENCODE_DISABLE_AUTOUPDATE": "true",
        "OPENCODE_DISABLE_LSP_DOWNLOAD": "true", "NO_BROWSER": "1",
    }
    for key in ("CONFIG", "CACHE", "STATE", "DATA"):
        directory = args.out / f"opencode-{key.lower()}"
        directory.mkdir()
        agent_env[f"XDG_{key}_HOME"] = str(directory)
    write_json(data / "profiles.json", {"executors": {"COPILOT": {VARIANT: {"COPILOT": {
        "base_command_override": str(wrapper), "allow_all_tools": True, "env": agent_env,
    }}}}})
    repo = args.out / "fixture-repo"
    repo.mkdir()
    (repo / FILE).write_text("state=planned\n", encoding="ascii")
    git(repo, "init", "--initial-branch=main")
    git(repo, "add", FILE)
    git(repo, "-c", "user.name=E3 fixture", "-c", "user.email=e3@example.invalid",
        "-c", "commit.gpgSign=false", "-c", "core.hooksPath=NUL", "commit", "-m", "Baseline")
    if git(repo, "remote"):
        raise FixtureError("The disposable repository must have no remotes")
    return repo


def seed(api, repo):
    if api("/projects") or api("/workspaces") or api("/repos"):
        raise FixtureError("Refusing to seed a nonempty fixture database")
    info = api("/info")
    assert VARIANT in info["executors"]["COPILOT"], sorted(info["executors"]["COPILOT"])
    config = info["config"]
    config.update(analytics_enabled=False, relay_enabled=False, pr_auto_description_enabled=False,
                  commit_reminder_enabled=False)
    config["notifications"].update(push_enabled=False, sound_enabled=False)
    api("/config", "PUT", config)
    project = api("/projects", "POST", {"name": PROJECT})
    base = f"/projects/{project['id']}/statuses"
    inbox = api(base, "POST", {"name": INBOX, "category": "unstarted", "color": "#6b7280"})
    started = api(base, "POST", {"name": STARTED, "category": "started", "color": "#0d9488"})
    repository = api("/repos", "POST", {"path": str(repo), "display_name": REPOSITORY})
    return project, inbox, started, repository


class Flow:
    def __init__(self, page, out, console):
        self.page = page
        self.out = out
        self.console = console
        self.step = "startup"
        self.passed = []
        self.keys = []

    def button(self, name):
        return self.page.get_by_role("button", name=name, exact=True)

    def focused(self):
        return self.page.evaluate("""() => {
          const el = document.activeElement;
          return {tag: el.tagName, role: el.getAttribute('role'), label: el.getAttribute('aria-label'),
                  placeholder: el.getAttribute('placeholder'), text: (el.innerText || '').slice(0, 110)};
        }""")

    def press(self, key):
        before = self.focused()
        self.page.keyboard.press(key)
        self.keys.append({"step": self.step, "key": key, "focus_before": before})
        print(f"{self.step}: {key} {before}", file=self.console, flush=True)

    def type(self, value):
        punctuation = {" ": "Space", ":": "Shift+Semicolon", "_": "Shift+Minus",
                       "=": "Equal", "-": "Minus", ".": "Period", "/": "Slash"}
        for char in value:
            key = f"Shift+Key{char}" if char.isupper() else punctuation.get(char, char)
            self.press(key)

    def tab_to(self, target, name, limit=60):
        expect(target).to_be_visible()
        trace = []
        for index in range(limit + 1):
            if target.evaluate("el => el === document.activeElement || el.contains(document.activeElement)"):
                return
            trace.append(self.focused())
            if index < limit:
                self.press("Tab")
        raise KeyboardGap(f"Tab never reached {name}. Last focus stops: {trace[-12:]}")

    def activate(self, name):
        target = self.button(name)
        expect(target).to_be_enabled(timeout=15_000)
        self.tab_to(target, name)
        self.press("Enter")

    def palette(self, query, result):
        self.press("Control+k")
        search = self.page.get_by_placeholder(SEARCH, exact=True)
        expect(search).to_be_focused()
        self.type(query)
        option = self.page.get_by_role("option", name=result, exact=True)
        expect(option).to_be_visible()
        expect(option).to_be_enabled()
        self.press("Enter")
        expect(search).to_be_hidden()

    def shot(self, name):
        self.page.screenshot(path=self.out / f"{name}.png", timeout=5_000)

    def pass_step(self):
        self.passed.append(self.step)
        self.shot(f"{len(self.passed):02d}-{self.step}")
        print(f"PASS {self.step}", flush=True)

    def mutate(self, method, path, key="Enter"):
        match = lambda r: r.request.method == method and urlsplit(r.url).path == "/api" + path
        with self.page.expect_response(match, timeout=70_000) as event:
            self.press(key)
        response = event.value
        result = response.json()
        if not response.ok or not result["success"]:
            raise ProductError(f"{method} {path} returned {response.status}: {result}")
        return result["data"], response.request.post_data_json


def accept(flow, origin, api, repo, fixture, agent_timeout, sockets, raw_frames):
    page = flow.page
    project, inbox, started, registered = fixture
    issues = f"/projects/{project['id']}/issues"
    with page.expect_response(lambda response: urlsplit(response.url).path.endswith(".wasm"),
                              timeout=25_000) as wasm:
        page.goto(origin, wait_until="commit")
    assert wasm.value.ok and wasm.value.headers.get("content-type", "").startswith("application/wasm")
    expect(page).to_have_title("vibe-own")
    expect(flow.button("新建议题").first).to_be_visible()
    expect(page.get_by_text(PROJECT, exact=True).first).to_be_visible()

    flow.step = "find-project"
    flow.palette("plrs", PROJECT)
    expect(page.get_by_text(PROJECT, exact=True).first).to_be_visible()
    flow.pass_step()

    flow.step = "create-issue"
    flow.press("c")
    field = page.get_by_placeholder("议题标题", exact=True)
    expect(field).to_be_focused()
    flow.type(TITLE)
    expect(field).to_have_value(TITLE)
    flow.shot("before-create-issue")
    created, _ = flow.mutate("POST", issues)
    issue_path = f"{issues}/{created['id']}"
    persisted = api(issue_path)
    assert (persisted["title"], persisted["status_id"]) == (TITLE, inbox["id"]), persisted
    expect(field).to_be_hidden()
    flow.pass_step()

    flow.step = "find-issue"
    flow.palette("rls stt", re.compile(rf"^{re.escape(created['simple_id'])}\s+{re.escape(TITLE)}$"))
    expect(flow.button("返回")).to_be_visible()
    expect(flow.button(f"状态 · {INBOX}")).to_be_visible()
    flow.pass_step()

    flow.step = "move-status"
    flow.palette("change status", "移到状态")
    listbox = page.get_by_role("listbox", name="命令结果", exact=True)
    target = listbox.get_by_role("option", name=STARTED, exact=True)
    expect(target).to_be_enabled()
    # NSH's non-searchable Command handles keys on the focusable wrapper.
    picker = listbox.locator("xpath=ancestor::*[@tabindex='0'][1]")
    flow.tab_to(picker, "status picker")
    # The only enabled option is the destination. End explicitly selects it.
    flow.press("End")
    flow.mutate("PATCH", issue_path)
    assert api(issue_path)["status_id"] == started["id"]
    expect(flow.button(f"状态 · {STARTED}")).to_be_visible()
    flow.pass_step()

    flow.step = "launch-workspace"
    flow.palette("launch agent", "启动 Agent 工作区")
    executor = page.get_by_role("combobox", name="执行器", exact=True)
    expect(executor).to_be_enabled(timeout=15_000)
    flow.tab_to(executor, "executor select")
    flow.press("Enter")
    search = page.get_by_placeholder("搜索选项", exact=True)
    expect(search).to_be_focused()
    flow.type(VARIANT)
    expect(page.get_by_role("option", name=f"COPILOT · {VARIANT}", exact=True)).to_be_visible()
    flow.press("Enter")
    expect(search).to_be_hidden()
    submit = flow.button("启动工作区")
    expect(submit).to_be_enabled(timeout=15_000)
    flow.tab_to(submit, "workspace launch")
    flow.shot("before-launch-workspace")
    launched, request = flow.mutate("POST", "/workspaces/start")
    workspace, execution = launched["workspace"], launched["execution_process"]
    path = f"/workspaces/{workspace['id']}"
    assert request["executor_config"] == PROFILE and request["local_issue_id"] == created["id"], request
    assert request["repos"] == [{"repo_id": registered["id"], "target_branch": "main"}], request
    assert api(path)["local_issue_id"] == created["id"] and execution["run_reason"] == "codingagent"
    expect(flow.button("日志与变更")).to_be_visible()
    flow.pass_step()

    flow.step = "find-workspace"
    # The branch’s random prefix uniquely distinguishes the workspace from its source issue.
    branch = workspace["branch"]
    search_prefix = branch.split("-", 1)[0].replace("/", "", 1)
    flow.palette(search_prefix, workspace["name"])
    expect(flow.button("日志与变更")).to_be_visible()
    flow.pass_step()

    flow.step = "logs-and-diff"
    flow.activate("日志与变更")
    session = page.get_by_text(f"Session {execution['session_id'].split('-')[0]} · 1 processes", exact=True)
    expect(session).to_be_visible()
    expect(page.get_by_text("Watching worktree", exact=True)).to_be_visible()
    diff_socket = "/api" + path + "/git/diff/ws"
    session_socket = f"/api/execution-processes/stream/session/ws?session_id={execution['session_id']}"
    log_socket = f"/api/execution-processes/{execution['id']}/raw-logs/ws"
    deadline = monotonic() + agent_timeout
    while True:
        process = api(f"/execution-processes/{execution['id']}")
        if process["status"] != "running":
            break
        if monotonic() >= deadline:
            raise FixtureError(f"Offline Agent did not finish within {agent_timeout}s: {execution['id']}")
        page.wait_for_timeout(500)
    if process["status"] != "completed":
        raise FixtureError(f"Offline Agent {execution['id']} ended as {process['status']}; see server.log")
    worktree = Path(api(path)["container_ref"]) / repo.name
    assert worktree.resolve().is_relative_to(flow.out), f"Worktree escaped fixture: {worktree}"
    if (worktree / FILE).read_text(encoding="utf-8").strip() != "state=verified":
        raise FixtureError(f"Offline Agent did not make the requested change in {worktree / FILE}")
    assert git(worktree, "rev-parse", "HEAD") == git(repo, "rev-parse", "main")
    assert not git(worktree, "remote")
    expect(page.get_by_text(re.compile(rf"(?:^|/){re.escape(FILE)}$")).first).to_be_visible(timeout=15_000)
    for line in ("state=planned", "state=verified"):
        expect(page.get_by_text(line, exact=False).last).to_be_visible()
    assert {diff_socket, session_socket, log_socket} <= sockets, sockets
    assert raw_frames.get(log_socket, 0) > 0, "No stdout/stderr patch reached the browser's raw log stream"
    expect(page.get_by_text(re.compile(r'^\s*\{"(?:Message|Done|Usage|ToolCall)"\s*:')).first).to_be_visible()
    flow.pass_step()

    flow.step = "open-pr-form"
    flow.activate("创建 PR")
    expect(page.get_by_text("创建 Pull Request", exact=True)).to_be_visible()
    expect(page.get_by_text(f"{workspace['branch']} → main", exact=True)).to_be_visible()
    title = page.locator('[aria-label="PR 标题"] input').and_(page.get_by_role("textbox"))
    expect(title).to_have_value(workspace["name"])
    expect(page.get_by_role("button", name="推送并创建 PR", exact=True)).to_be_visible()
    # Confirm a newly mounted input is also reachable by keyboard, leaving the form open.
    flow.tab_to(title, "PR title")
    flow.pass_step()
    return {"issue": created["id"], "workspace": workspace["id"], "execution": execution["id"]}


def browser_run(args, m4, origin, api, repo, fixture):
    with ExitStack() as stack:
        console = stack.enter_context((args.out / "steps.log").open("w", encoding="utf-8"))
        playwright = stack.enter_context(sync_playwright())
        browser = playwright.chromium.launch(channel=args.channel, headless=True)
        stack.callback(browser.close)
        context = browser.new_context(viewport=VIEWPORT, service_workers="block",
                                      record_video_dir=args.out / "video", record_video_size=VIEWPORT)
        stack.callback(context.close)
        page = context.new_page()
        page.set_default_timeout(10_000)
        flow = Flow(page, args.out, console)
        sockets, raw_frames, blocked, errors = set(), {}, [], []
        origin_host = urlsplit(origin).netloc
        issue_path = f"/api/projects/{fixture[0]['id']}/issues"

        def route_request(route):
            request = route.request
            url = urlsplit(request.url)
            local = url.netloc == origin_host
            allowed = (request.method in {"GET", "HEAD", "OPTIONS"}
                       or (request.method == "POST" and url.path in {issue_path, "/api/workspaces/start"})
                       or (request.method == "PATCH" and url.path.startswith(issue_path + "/")))
            if local and allowed:
                route.continue_()
            else:
                blocked.append(f"{request.method} {request.url}")
                route.abort()

        def connect(ws):
            url = urlsplit(ws.url)
            path = url.path + ("?" + url.query if url.query else "")
            if url.netloc != origin_host:
                errors.append(f"Unexpected WebSocket origin {ws.url}")
                return
            sockets.add(path)
            if path.endswith("/raw-logs/ws"):
                def frame(payload):
                    if b'"STDOUT"' in payload or b'"STDERR"' in payload:
                        raw_frames[path] = raw_frames.get(path, 0) + 1

                ws.on("framereceived", lambda payload: frame(payload.encode() if isinstance(payload, str) else payload))

        context.route("**/*", route_request)
        page.on("websocket", connect)
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("console", lambda msg: print(f"Browser {msg.type}: {msg.text}", file=console, flush=True)
                if msg.type in {"error", "warning"} else None)
        results = {"status": "failed", "browser": browser.version, "origin": origin}
        try:
            results.update(accept(flow, origin, api, repo, fixture, args.agent_timeout, sockets, raw_frames))
            assert not blocked and not errors, (blocked, errors)
            results["status"] = "passed"
        except Exception as exc:
            results.update(failed_step=flow.step, error=str(exc), error_type=type(exc).__name__)
            with suppress(Exception):
                flow.shot("failure")
                results["failure_focus"] = flow.focused()
                (args.out / "failure-dom.txt").write_text(page.locator("body").aria_snapshot(), encoding="utf-8")
            raise
        finally:
            results.update(passed=flow.passed, keys=flow.keys, sockets=sorted(sockets),
                           raw_log_frames=raw_frames, blocked_requests=blocked, page_errors=errors)
            m4.write_json(args.out / "results.json", results)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[5])
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True, help="New directory for isolated test artifacts")
    parser.add_argument("--channel", help="Installed Chromium channel, e.g. chrome")
    parser.add_argument("--opencode", default=shutil.which("opencode.cmd") or shutil.which("opencode"))
    parser.add_argument("--ollama", default="http://127.0.0.1:11434")
    parser.add_argument("--model", default="gemma4:latest")
    parser.add_argument("--agent-timeout", type=int, default=600)
    args = parser.parse_args()
    args.ollama = args.ollama.rstrip("/")
    if not re.fullmatch(r"http://127\.0\.0\.1:\d{1,5}", args.ollama):
        parser.error("--ollama must use http://127.0.0.1:<port>")
    for name in ("repo", "server", "site", "out"):
        setattr(args, name, getattr(args, name).resolve())
    if not args.opencode or not Path(args.opencode).is_file():
        parser.error("Specify the existing OpenCode CLI launcher with --opencode")
    if not (args.site / "index.html").is_file() or not any(args.site.glob("nsh.*.wasm")):
        parser.error("--site must point at the packaged NSH Web directory")
    m4 = load_m4(args.repo)
    if not args.server.is_file() or not m4.uses_fixture_assets(args.server):
        parser.error("--server must support VK_ASSET_DIR and same-origin VK_WEB_ROOT")
    with args.server.open("rb") as executable, m4.mmap(executable.fileno(), 0, access=m4.ACCESS_READ) as image:
        if image.find(b"VK_WEB_ROOT") < 0:
            parser.error("--server must support VK_WEB_ROOT for same-origin product hosting")
    args.same_origin = True
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        args.web_port = args.backend_port = sock.getsockname()[1]
    args.out.mkdir(parents=True, exist_ok=False)
    try:
        repo = prepare(args, m4.write_json)
        # Scoped to this driver and its subprocesses. The backend refreshes the
        # ACP registry at startup even before the user opens Agent settings.
        os.environ.update(HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1",
                          ALL_PROXY="http://127.0.0.1:1", NO_PROXY="localhost,127.0.0.1,::1")
        with m4.server(args, args.out / "data", args.out / "server.log") as (origin, _, _):
            def api(path, method="GET", payload=None):
                return m4.api(origin, path, method, payload)

            fixture = seed(api, repo)
            browser_run(args, m4, origin, api, repo, fixture)
    except (FixtureError, KeyboardGap, ProductError, PlaywrightTimeout) as exc:
        sys.exit(f"{type(exc).__name__}: {exc}. Evidence: {args.out}")
    print(f"PASS. Evidence: {args.out}", flush=True)


if __name__ == "__main__":
    main()
