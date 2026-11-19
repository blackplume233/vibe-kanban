"""Use one real vibe server, worktree and dev process for both NSH endpoints."""

from base64 import b64encode
from contextlib import contextmanager
import json
from importlib import import_module
import os
from pathlib import Path
import re
import subprocess
import sys
from tempfile import TemporaryDirectory
from time import monotonic, sleep
from urllib.error import URLError
from urllib.request import Request, urlopen

BACKEND_PORT = int(os.environ.get("M1_BACKEND_PORT", "59301"))
WEB_PORT = int(os.environ.get("M1_WEB_PORT", "59302"))
BACKEND = f"http://127.0.0.1:{BACKEND_PORT}"
TOOLS = Path(__file__).parent
WORKSPACE = "M1 local worktree"


def api(path, method="GET", payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    request = Request(BACKEND + "/api" + path, data, {"Content-Type": "application/json"}, method=method)
    with urlopen(request, timeout=15) as response:
        result = json.load(response)
        if not result["success"]:
            raise RuntimeError(f"{method} {path}: {result.get('message')}")
        print(f"HTTP {response.status} {method} {path}", flush=True)
        return result.get("data")


@contextmanager
def server(repo, target, temp):
    env = os.environ.copy()
    env.update(BACKEND_PORT=str(BACKEND_PORT), HOST="127.0.0.1", TEMP=str(temp), TMP=str(temp),
               APPDATA=str(temp), LOCALAPPDATA=str(temp),
               VK_ALLOWED_ORIGINS=f"http://127.0.0.1:{WEB_PORT}")
    env.pop("VK_SHARED_API_BASE", None)
    with (target / "server-run.log").open("wb") as log:
        process = subprocess.Popen(
            [str(target / "debug/server.exe")], cwd=repo, env=env, stdout=log, stderr=log
        )
        try:
            deadline = monotonic() + 40
            while True:
                if process.poll() is not None or monotonic() >= deadline:
                    raise RuntimeError(f"vibe server failed to start; see {log.name}")
                try:
                    with urlopen(BACKEND + "/api/health", timeout=2):
                        if process.poll() is None:
                            break
                except (URLError, TimeoutError):
                    pass
                    sleep(0.25)
            yield env
        finally:
            if process and process.poll() is None:
                process.terminate()
                process.wait(timeout=10)


@contextmanager
def fixture(temp):
    repository = temp / "m1-spike"
    repository.mkdir()

    def git(*args):
        subprocess.run(["git", "-C", str(repository), *args], check=True, stdout=subprocess.DEVNULL)

    git("init", "--initial-branch=main")
    git("config", "core.autocrlf", "false")
    (repository / "review.txt").write_text("Ship locally\nstatus=planned\n", encoding="utf-8", newline="\r\n")
    git("add", "review.txt")
    git("-c", "user.name=M1 smoke", "-c", "user.email=m1-smoke@example.invalid", "commit", "-m", "Base revision")
    repo_id = api("/repos", "POST", {"path": str(repository), "display_name": "M1 Spike"})["id"]
    workspace_id = None
    try:
        # ScriptRequest runs cmd.exe /C, so avoid nesting quoted arguments.
        script = """
Set-Content -Path review.txt -Value @('Ship locally','status=verified','log_transport=websocket') -Encoding ascii
for ($i=1; $i -le 90; $i++) {
    Write-Output ('M1 live tick {0:000}' -f $i)
    Start-Sleep -Milliseconds 500
}
"""
        encoded = b64encode(script.encode("utf-16-le")).decode("ascii")
        command = f"powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}"
        api(f"/repos/{repo_id}", "PUT", {"dev_server_script": command})
        name = f"{WORKSPACE} {temp.name[-8:]}"
        workspace_id = api("/workspaces", "POST", {"name": name})["id"]
        workspace = f"/workspaces/{workspace_id}"
        api(workspace + "/repos", "POST", {"repo_id": repo_id, "target_branch": "main"})
        processes = api(workspace + "/execution/dev-server/start", "POST")
        execution_id = processes[0]["id"]
        print(f"Fixture workspace={workspace_id} session={processes[0]['session_id']} process={execution_id}", flush=True)
        yield name, workspace_id, execution_id
    finally:
        if workspace_id is not None:
            api(f"/workspaces/{workspace_id}/execution/stop", "POST")
            api(f"/workspaces/{workspace_id}?delete_branches=true", "DELETE")
        api(f"/repos/{repo_id}", "DELETE")


def windows(target, evidence, env, workspace_id, execution_id):
    stderr = target / "native-stderr.log"
    capture = subprocess.run([
        "powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
        str(TOOLS / "m1-native-window.ps1"), "-Executable", str(target / "release/vibe-own.exe"),
        "-Evidence", str(evidence), "-Log", str(stderr),
        "-WorkspaceId", workspace_id, "-ExecutionId", execution_id,
    ], env=env, capture_output=True, text=True, encoding="utf-8")
    print(capture.stdout, end="", flush=True)
    if capture.stderr:
        print(capture.stderr, end="", file=sys.stderr, flush=True)
    capture.check_returncode()
    samples = stderr.read_text(encoding="utf-8").splitlines()
    active_frames = []
    last_offset = None
    moving = False
    for line in samples:
        offset = re.search(r"benchmark offset=(\d+)", line)
        if offset:
            last_offset = int(offset[1])
            moving = True
        if "nsh::perf" in line and moving:
            if "fps 0/" not in line:
                active_frames.append(line)
            moving = False
    assert len(active_frames) >= 3 and last_offset and last_offset > 500, "Missing active native scroll windows"
    p95 = max(float(re.search(r"\bframe [\d.]+/[\d.]+/([\d.]+)/", line)[1]) for line in active_frames)
    evidence.joinpath("m1-native-perf.md").write_text(
        "# Win32 滚动实测\n\n```text\nWindows Release --fps, 5000 rows, scripted Win32 wheel input\n"
        + capture.stdout
        + "NSH frame metric: Win32 main-thread work per presented frame, excludes VSync wait.\n"
        + "\n".join(active_frames)
        + f"\nLast observed scroll offset: {last_offset} px\nMaximum one-second frame p95: {p95:.2f} ms\n```\n",
        encoding="utf-8", newline="\n",
    )
    print(f"Win32 scroll p95 (max of {len(active_frames)} active windows): {p95:.2f} ms", flush=True)
    return p95 <= 16


def main():
    mode, repo, target, evidence = sys.argv[1:]
    repo, target, evidence = map(Path, (repo, target, evidence))
    passed = True
    with TemporaryDirectory(prefix="m1-", dir=target) as path:
        temp = Path(path)
        with server(repo, target, temp) as env, fixture(temp) as (name, workspace_id, execution_id):
            if mode != "windows":
                browser = import_module("m1-web-browser")
                browser.capture(target / "m1-site", evidence, BACKEND, WEB_PORT, name, workspace_id, execution_id)
            if mode != "web":
                passed = windows(target, evidence, env, workspace_id, execution_id)
    if not passed:
        sys.exit("Win32 frame p95 exceeded 16 ms")


if __name__ == "__main__":
    main()
