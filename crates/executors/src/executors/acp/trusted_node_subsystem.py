#!/usr/bin/env python3
"""OpenSSH `vibe-acp` subsystem: verify a shared checkout and run one configured agent.

Arguments: REMOTE_ROOT REGISTRY_ID /absolute/agent [FIXED_AGENT_ARGS ...]
Example sshd_config entry on the POSIX node:
    Subsystem vibe-acp /usr/bin/python3 -I /opt/vibe/trusted_node_subsystem.py /srv/workspaces kilo /opt/acp/kilo acp

Use a dedicated node account with read/write access to the mounted worktrees and
their linked Git metadata. Keep the wrapper and agent outside the shared root.
Provision MCP tools and agent credentials under the node account.
"""

import json
import os
from pathlib import Path
import re
import select
import signal
import stat
import subprocess
import sys


PROBE_NAME = re.compile(r"\.vk-acp-probe-[a-zA-Z0-9]+\Z")
NONCE = re.compile(rb"[a-f0-9]{32}\Z")
MAX_REQUEST = 64 * 1024
MAX_PENDING = 16 * 1024 * 1024


def directory(value, root):
    path = Path(value)
    if (
        not path.is_absolute()
        or path == root
        or root not in path.parents
        or str(path) != value
        or path.resolve(strict=True) != path
        or not path.is_dir()
    ):
        raise ValueError("working directory is outside the configured root or traverses a symlink")
    return path


def verify_git(repo):
    result = subprocess.run(
        [
            "/usr/bin/git", "-C", str(repo), "rev-parse", "--path-format=absolute",
            "--show-toplevel", "--git-dir", "--git-common-dir",
        ],
        check=True,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        text=True,
        timeout=5,
    )
    worktree, git_dir, common_dir = result.stdout.splitlines()
    if Path(worktree).resolve(strict=True) != repo or any(
        not Path(path).is_dir() or not os.access(path, os.R_OK | os.W_OK | os.X_OK)
        for path in (git_dir, common_dir)
    ):
        raise ValueError("Git worktree metadata is not accessible on the node")


def acknowledge_probe(path, name):
    if not PROBE_NAME.fullmatch(name):
        raise ValueError("invalid workspace probe name")
    directory_fd = os.open(path, os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        fd = os.open(name, os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory_fd)
        with os.fdopen(fd, "r+b", buffering=0) as probe:
            st = os.fstat(probe.fileno())
            if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1:
                raise ValueError("workspace probe is not a private regular file")
            nonce = probe.read(33)
            if not NONCE.fullmatch(nonce):
                raise ValueError("workspace probe content is invalid")
            probe.seek(0)
            probe.write(b"ack:" + nonce)
            os.fsync(probe.fileno())
    finally:
        os.close(directory_fd)


def run_agent(command):
    def terminate(_signal, _frame):
        raise SystemExit(1)

    signal.signal(signal.SIGHUP, terminate)
    signal.signal(signal.SIGTERM, terminate)
    agent = subprocess.Popen(command, stdin=subprocess.PIPE, start_new_session=True)
    try:
        input_fd = agent.stdin.fileno()
        os.set_blocking(input_fd, False)
        pending = bytearray()
        while agent.poll() is None:
            readable, writable, _ = select.select([0], [input_fd] if pending else [], [], 0.25)
            if readable:
                chunk = os.read(0, 65536)
                if not chunk:
                    break  # Disconnect must stop the agent even if it ignores stdin EOF.
                pending.extend(chunk)
                if len(pending) > MAX_PENDING:
                    raise ValueError("ACP agent stopped reading its input")
            if writable:
                try:
                    written = os.write(input_fd, pending)
                    del pending[:written]
                except BlockingIOError:
                    pass
                except BrokenPipeError:
                    break
        return agent.poll() or 0
    finally:
        # Clean up descendants too, including when the leader has already exited.
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(agent.pid, sig)
            except ProcessLookupError:
                break
            try:
                agent.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass


def main():
    root_arg, registry_id, *agent = sys.argv[1:]
    root = Path(root_arg)
    if root == Path("/") or not root.is_absolute() or root.resolve(strict=True) != root:
        raise ValueError("subsystem root must be a canonical, dedicated absolute directory")
    if not agent or not os.path.isabs(agent[0]):
        raise ValueError("subsystem requires an absolute agent executable")

    # Use unbuffered input to keep any subsequent ACP bytes in the pipe.
    stdin = sys.stdin.buffer.raw
    line = stdin.readline(MAX_REQUEST + 1)
    if len(line) > MAX_REQUEST or not line.endswith(b"\n"):
        raise ValueError("invalid workspace preflight frame")
    request = json.loads(line)
    if request["registry_id"] != registry_id:
        raise ValueError("profile selected a different registry agent")
    workspace = directory(request["workspace"], root)
    cwd = directory(request["cwd"], root)
    if cwd != workspace and workspace not in cwd.parents:
        raise ValueError("working directory must be inside this workspace")
    repos = [directory(value, root) for value in request["repositories"]]
    for repo in repos:
        if repo.parent != workspace:
            raise ValueError("repository must be directly inside this workspace")
        verify_git(repo)
    probes = {directory(path, root): name for path, name in request["probes"]}
    if probes.keys() != {workspace, cwd, *repos}:
        raise ValueError("working directory or repository is missing a shared workspace probe")
    for path, name in probes.items():
        acknowledge_probe(path, name)

    os.chdir(cwd)
    sys.stdout.buffer.write(b"vibe-acp/1 PROOF\n")
    sys.stdout.buffer.flush()
    if stdin.readline(17) != b"vibe-acp/1 START\n":
        raise ValueError("client did not confirm the shared workspace")
    return run_agent(agent)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(f"vibe-acp subsystem: {error}", file=sys.stderr)
        sys.exit(1)
