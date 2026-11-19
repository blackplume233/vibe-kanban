"""运行已构建的 M4 原生或 Web Release 前端，使用独立的本地 SQLite。

从仓库根目录启动。--server 应指向支持 VK_ASSET_DIR 的 standalone server。
脚本将 SQLite 写入本次证据目录中的 data/。

Build once:
  cargo build --locked --bin server
Browser prerequisite:
  python -m pip install playwright
  python -m playwright install chromium
Usage (PowerShell, from the worktree):
  $tools = '.artifact/goal/2026-11-18-vibe-own-edition/harness/tools'
  python "$tools/m4-runtime.py" web --server F:/vk-targets/m4/debug/server.exe --site F:/vk-targets/m4/m1-site --backend-port 59401 --web-port 59402 --out F:/vk-targets/m4/proof-web
  python "$tools/m4-runtime.py" windows --server F:/vk-targets/m4/debug/server.exe --native F:/vk-targets/m4/release/vibe-own.exe --backend-port 59411 --out F:/vk-targets/m4/proof-native

--site 指向使用当前 M4 WASM 构建的静态站点（M1 的站点布局可直接复用）。
原生测量须在可见、未锁屏的桌面会话中运行。两次运行分别重建 SQLite。
冷启动测量从新原生进程或新 Chromium 上下文的导航开始，夹具服务器已就绪。
原生以首次真实列表滚动为终点，Web 以键盘打开首条议题后的绘制机会为终点。
原生 --packaged 在准备夹具后关闭 standalone server，再启动包内后端并计时。
原生 --cold-runs 9 使用事件通知分段计时。每次启动新的 UI 进程，等首批议题可见后发送一次滚轮，采样前后核对编译进程。
--enforce-budgets 检查原生 1000 ms、Web 2000 ms 和原生帧时间 p95 16 ms。
Web rAF 受刷新率约束，脚本记录原始间隔和独立 Chromium 渲染 trace。
"""

import argparse
from contextlib import ExitStack, closing, contextmanager
import json
from mmap import ACCESS_READ, mmap
import os
from pathlib import Path
import re
import socket
import sqlite3
import subprocess
import sys
from time import monotonic, sleep
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from uuid import UUID, uuid5

TOOLS = Path(__file__).resolve().parent
REPO = TOOLS.parents[4]
BULK_COUNT = 5000
PROJECT = "M4 Harness Polaris"
WORKSPACE = "M4 Harness Dockyard"
STATUS_INBOX = "M4 Inbox"
STATUS_STARTED = "M4 In Progress"
NAMED = {
    "keyboard_first": "M4 Keyboard entry",
    "keyboard_second": "M4 Keyboard neighbor",
    "query": "M4 Comet compass",
    "drag": "M4 Drag card",
    "started": "M4 Started anchor",
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def write_json(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def api(backend, path, method="GET", payload=None, timeout=15):
    data = None if payload is None else json.dumps(payload).encode()
    request = Request(backend + "/api" + path, data, {"Content-Type": "application/json"}, method=method)
    try:
        with urlopen(request, timeout=timeout) as response:
            result = json.load(response)
    except HTTPError as error:
        raise RuntimeError(f"{method} {path}: HTTP {error.code}: {error.read().decode(errors='replace')}") from error
    require(result["success"], f"{method} {path}: {result.get('message')}")
    return result.get("data")


def port_is_free(port):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", port))


@contextmanager
def server(args, assets, log_path):
    env = os.environ.copy()
    temp = log_path.parent / "runtime-temp"
    temp.mkdir()
    env.update(
        BACKEND_PORT=str(args.backend_port), PREVIEW_PROXY_PORT="0", HOST="127.0.0.1",
        VK_ALLOWED_ORIGINS=f"http://127.0.0.1:{args.web_port}", TMP=str(temp), TEMP=str(temp),
        TMPDIR=str(temp), RUST_LOG="info", NO_COLOR="1",
    )
    env["VK_ASSET_DIR"] = str(assets)
    if args.same_origin:
        env["VK_WEB_ROOT"] = str(args.site)
    env.pop("VK_SHARED_API_BASE", None)
    backend = f"http://127.0.0.1:{args.backend_port}"
    with log_path.open("wb") as log:
        process = subprocess.Popen([str(args.server)], cwd=REPO, env=env, stdout=log, stderr=log)
        try:
            deadline = monotonic() + 45
            while True:
                require(process.poll() is None and monotonic() < deadline,
                        f"等待本地服务器失败。见 {log_path}")
                try:
                    with urlopen(backend + "/api/health", timeout=0.5):
                        break
                except (URLError, TimeoutError):
                    sleep(0.05)
            database = assets / "db.v2.sqlite"
            require(process.poll() is None and database.is_file(),
                    f"服务器未使用 VK_ASSET_DIR 指定的数据库 {database}。见 {log_path}")
            yield backend, env, database
        finally:
            if process.poll() is None:
                subprocess.run(["taskkill.exe", "/F", "/T", "/PID", str(process.pid)],
                               stdout=log, stderr=log, check=False)
                process.wait(timeout=10)


def seed(backend, database):
    require(api(backend, "/projects") == [], "夹具数据库中已有项目")
    project = api(backend, "/projects", "POST", {"name": PROJECT})
    base = f"/projects/{project['id']}"
    statuses = []
    for name, category, color in [
        (STATUS_INBOX, "unstarted", "#6b7280"),
        (STATUS_STARTED, "started", "#0d9488"),
    ]:
        statuses.append(api(backend, base + "/statuses", "POST", {
            "name": name, "category": category, "color": color,
        }))
    workspace = api(backend, "/workspaces", "POST", {"name": WORKSPACE})
    project_uuid = UUID(project["id"])
    inbox, started = [UUID(status["id"]).bytes for status in statuses]
    timestamp = "2026-11-18 00:00:00.000"

    def record(title, order, status):
        return (uuid5(project_uuid, title).bytes, project_uuid.bytes, status, title,
                f"M4 fixture, record {order}", order, timestamp, timestamp)

    with closing(sqlite3.connect(database, timeout=15)) as db, db:
        db.execute("PRAGMA foreign_keys = ON")
        require(db.execute("SELECT COUNT(*) FROM issues").fetchone()[0] == 0, "夹具库中已有议题")
        db.executemany(
            "INSERT INTO issues (id, project_id, status_id, title, description, sort_order, created_at, updated_at) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (record(title, order, started if key == "started" else inbox)
             for order, (key, title) in enumerate(NAMED.items())),
        )
        db.executemany(
            "INSERT INTO issues (id, project_id, status_id, title, description, sort_order, created_at, updated_at) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (record(f"M4 Load issue {i + 1:04d}", len(NAMED) + i, inbox if i % 2 == 0 else started)
             for i in range(BULK_COUNT)),
        )
    issues = api(backend, base + "/issues")
    require(len(issues) == BULK_COUNT + len(NAMED), f"实际 API 返回 {len(issues)} 条议题")
    require(sum(row["title"].startswith("M4 Load issue ") for row in issues) == BULK_COUNT,
            "压力数据行数不匹配")
    by_title = {row["title"]: row for row in issues}
    return {
        "project": project, "workspace": workspace, "statuses": statuses,
        "named": {key: by_title[title] for key, title in NAMED.items()},
        "bulk_issues": BULK_COUNT, "total_issues": len(issues),
    }


def native(args, out, env, fixture):
    log_path = out / "native-fps.log"
    with (out / "native-window.log").open("wb") as log:
        completed = subprocess.run([
            "powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
            str(TOOLS / "m4-native-window.ps1"), "-Executable", str(args.native),
            "-Evidence", str(out), "-Log", str(log_path), "-ProjectId", fixture["project"]["id"],
        ], env=env, stdout=log, stderr=log)
    require(completed.returncode == 0, f"原生窗口采样失败。见 {out / 'native-window.log'} 和 {log_path}")
    metrics = json.loads((out / "native-window.json").read_text(encoding="utf-8-sig"))
    frames = []
    active = False
    last_offset = metrics["initial_offset_px"]
    lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
    for line in lines[metrics["benchmark_start_log_line"]:metrics["benchmark_end_log_line"]]:
        offset = re.search(r"\bm4 issue offset=(-?\d+)\b", line)
        if offset and int(offset[1]) != last_offset:
            last_offset = int(offset[1])
            active = True
        perf = re.search(r"nsh::perf.*\bfps (\d+)/\d+ frame [\d.]+/[\d.]+/([\d.]+)/", line)
        if perf:
            if active and int(perf[1]) > 0:
                frames.append({"frames": int(perf[1]), "frame_p95_ms": float(perf[2]), "line": line})
            active = False
    require(len(frames) >= 3, f"只有 {len(frames)} 个实际议题滚动窗口。见 {log_path}")
    p95 = max(frame["frame_p95_ms"] for frame in frames)
    results = {
        **metrics, "total_issues": fixture["total_issues"], "scroll_windows": frames,
        "max_window_frame_p95_ms": p95,
        "frame_metric": "NSH Win32 main-thread work per presented frame, excluding VSync wait",
        "budgets": {"native_cold_start_1000_ms": metrics["cold_to_usable_ms"] <= 1000,
                    "native_frame_p95_16_ms": p95 <= 16},
    }
    write_json(out / "native-results.json", results)
    return results


def uses_fixture_assets(executable):
    # Refuse old servers that could otherwise open the user's default database.
    with executable.open("rb") as file:
        if not executable.stat().st_size:
            return False
        with mmap(file.fileno(), 0, access=ACCESS_READ) as image:
            return image.find(b"VK_ASSET_DIR") >= 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("mode", choices=("windows", "web"))
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--native", type=Path)
    parser.add_argument("--site", type=Path)
    parser.add_argument("--out", type=Path, required=True, help="本次运行的新证据目录")
    parser.add_argument("--backend-port", type=int, default=59401)
    parser.add_argument("--web-port", type=int, default=59402)
    parser.add_argument("--channel", help="Chrome/Edge Playwright channel；默认为 Playwright Chromium")
    parser.add_argument("--cold-runs", type=int, default=0, help="Win32 分段测量次数，使用同一 SQLite 夹具")
    parser.add_argument("--packaged", action="store_true", help="Win32 正式包内嵌服务，计时包含后端启动")
    parser.add_argument("--same-origin", action="store_true", help="由 standalone server 同源托管 Web 包")
    parser.add_argument("--enforce-budgets", action="store_true", help="测量超预算时返回失败并保留证据")
    args = parser.parse_args()
    for name in ("server", "native", "site", "out"):
        path = getattr(args, name)
        if path is not None:
            setattr(args, name, path.resolve())
    if not args.server.is_file() or not uses_fixture_assets(args.server):
        parser.error("--server 应支持 VK_ASSET_DIR，见 --help 中的编译说明")
    if args.mode == "windows" and (os.name != "nt" or not args.native or not args.native.is_file()):
        parser.error("windows 模式需要 Windows 和 --native Release 可执行文件")
    if args.mode == "web" and (not args.site or not (args.site / "index.html").is_file()):
        parser.error("web 模式需要 --site 指向已构建的站点目录")
    if args.cold_runs < 0 or (args.cold_runs and args.mode != "windows"):
        parser.error("--cold-runs 仅用于 windows，必须为非负整数")
    if args.packaged and (args.mode != "windows" or not (args.native.parent / "nsh/app.json").is_file()):
        parser.error("--packaged 需要保留 exe 旁边的 nsh/ 目录")
    if args.same_origin and args.mode != "web":
        parser.error("--same-origin 仅用于 web")
    if not all(1 <= port <= 65535 for port in (args.backend_port, args.web_port)) or args.backend_port == args.web_port:
        parser.error("两端口必须不同，范围 1–65535")
    port_is_free(args.backend_port)
    if args.mode == "web" and not args.same_origin:
        port_is_free(args.web_port)
    args.out.mkdir(parents=True, exist_ok=False)
    with ExitStack() as processes:
        backend, env, database = processes.enter_context(
            server(args, args.out / "data", args.out / "server.log")
        )
        fixture = seed(backend, database)
        write_json(args.out / "fixture.json", fixture)
        print(f"SQLite 已校验 {fixture['total_issues']} 条议题，证据目录 {args.out}", flush=True)
        if args.packaged:
            processes.close()
            env.pop("BACKEND_PORT", None)
            env["NSH_APP_DIR"] = str(args.native.parent / "nsh")
            env["VK_NSH_BENCHMARK"] = "1"
        if args.mode == "web":
            sys.dont_write_bytecode = True
            from m4_web import capture
            results = capture(args, backend, fixture, api, write_json)
        elif args.cold_runs:
            sys.dont_write_bytecode = True
            from m4_cold import capture
            results = capture(args, args.out, env, fixture)
            write_json(args.out / "native-cold-stages.json", results)
        else:
            results = native(args, args.out, env, fixture)
    print(json.dumps(results["budgets"], ensure_ascii=False), flush=True)
    if args.enforce_budgets and not all(results["budgets"].values()):
        sys.exit("M4 超出性能预算，详见证据目录。")


if __name__ == "__main__":
    main()
