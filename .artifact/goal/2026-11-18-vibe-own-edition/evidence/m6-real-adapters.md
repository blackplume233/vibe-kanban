# M6 已安装 ACP 适配器联调

Windows 上用已安装的 `@zed-industries/claude-agent-acp@0.18.0` 与 `@zed-industries/codex-acp@0.9.5` 验证 ACP v1。探针使用各自的临时 Git 目录与适配器数据目录，按通用执行器的请求形状发送 `initialize`、`session/new`、`session/prompt` 和 `session/load`。续接步骤先退出第一个适配器进程，再启动同一个适配器加载返回的会话。MCP 描述指向本地代理进程，它将请求转发到仓库构建的 `vibe-kanban-mcp`，只记录方法名。

| 安装包及启动方式 | `initialize` | `session/new` | 重启适配器后 `session/load` | MCP 序列与 prompt |
|---|---|---|---|---|
| Claude，包内 CLI | 成功，宣告 `loadSession=true` | `-32603` | 未取得会话 ID | 未启动 MCP。空 `mcpServers` 对照同样返回 `-32603`。 |
| Claude，`CLAUDE_CODE_EXECUTABLE` 指向本机 `claude.exe` | 两次成功 | 成功 | 成功 | 每次会话各发起一次 MCP `initialize` 和 `tools/list`。两次 `session/prompt` 均返回认证错误 `-32000`，没有发起 `tools/call`。 |
| Codex，包内 CLI | 成功，宣告 `loadSession=true` | 认证错误 `-32000` | 未取得会话 ID | 未启动 MCP。 |

两组探针均以 `1` 退出，因为隔离数据目录下的适配器未满足带认证 prompt 的完整验收。探针未输出原始错误内容或凭据。现有 `CLAUDE_CODE` 与 `CODEX` profile 键和旧会话标识保持不变。通过模拟 ACP Agent 连接真实 MCP 进程的独立验收见 [m6-acp.md](m6-acp.md)。

## 现有登录复验

在临时 Git 目录中复用已登录的 Claude CLI 和 Codex 身份，通过直连运行相同的 ACP 探针。Claude 显式指定 `CLAUDE_CODE_EXECUTABLE`，Codex 使用适配器自带的 CLI。两端均完成 `initialize`、`session/new`、`session/prompt`，以及重启适配器后的 `session/load` 和第二次 `session/prompt`。四次请求均返回 `end_turn`。

| 适配器 | MCP `initialize` | MCP `tools/list` | MCP `tools/call` |
| --- | ---: | ---: | ---: |
| Claude 与本机 CLI | 2 | 2 | 0 |
| Codex | 2 | 2 | 2 |

探针退出码为 0。Claude 包内 CLI 在 `session/new` 仍返回 `-32603`，关闭 MCP 的对照也相同。已验证的 Claude 配置依赖本机 CLI。

## 复跑

在本工作树中使用 Python 3、Node、Git 与上述已安装适配器。`vibe-kanban-mcp` 构建命令见 [m6-acp.md](m6-acp.md#ACP-Agent-发现本地-MCP-工具)。下列命令从此文档提取并运行已执行的探针。每次执行均分配独立临时目录。

```bash
python -c 'from pathlib import Path; p=Path(".artifact/goal/2026-11-18-vibe-own-edition/evidence/m6-real-adapters.md"); Path("F:/vk-targets/m6/m6-real-acp.py").write_text(p.read_text(encoding="utf-8").split("```python\n",1)[1].split("\n```",1)[0]+"\n",encoding="utf-8")'
python F:/vk-targets/m6/m6-real-acp.py F:/vk-targets/m6 F:/vk-targets/m6/debug/vibe-kanban-mcp.exe
```

```python
import json
import os
from pathlib import Path
from queue import Queue, Empty
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from threading import Thread

if sys.argv[1] == 'proxy':
    executable, trace = sys.argv[2:]
    with subprocess.Popen([executable, '--mode', 'global'], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as server:
        def relay():
            for raw in sys.stdin.buffer:
                method = json.loads(raw).get('method', '')
                with open(trace, 'a', encoding='utf-8') as log:
                    log.write(method + '\n')
                server.stdin.write(raw)
                server.stdin.flush()
            server.stdin.close()
        Thread(target=relay, daemon=True).start()
        for raw in server.stdout:
            sys.stdout.buffer.write(raw)
            sys.stdout.buffer.flush()
    raise SystemExit

mcp = Path(sys.argv[2]).resolve(strict=True)
launcher = Path(shutil.which('claude-agent-acp.cmd'))
packages = launcher.parent / 'node_modules' / '@zed-industries'
script = Path(__file__).resolve()
root = Path(tempfile.mkdtemp(prefix='acp-real-', dir=sys.argv[1]))


def probe(name, entry, config_var, enable_mcp=True, use_cli=False):
    cwd = root / name
    cwd.mkdir()
    config = cwd / 'data'
    config.mkdir()
    subprocess.run(['git', 'init', '-q'], cwd=cwd, check=True, capture_output=True)
    trace = cwd / 'mcp.txt'
    env = {**os.environ, config_var: str(config), 'NO_BROWSER': '1',
           'VIBE_BACKEND_URL': 'http://127.0.0.1:1', 'RUST_LOG': 'off'}
    if use_cli:
        env['CLAUDE_CODE_EXECUTABLE'] = shutil.which('claude.exe')
    mcp_servers = [{'name': 'vibe-kanban', 'command': sys.executable,
                    'args': [str(script), 'proxy', str(mcp), str(trace)], 'env': []}]
    base = {'cwd': str(cwd), 'mcpServers': mcp_servers if enable_mcp else []}
    report = []

    def turn(session_id=None):
        with subprocess.Popen([shutil.which('node'), str(packages / entry)], cwd=cwd, env=env,
                              text=True, encoding='utf-8', stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as proc:
            inbox = Queue()
            def collect():
                for line in proc.stdout:
                    try:
                        inbox.put(json.loads(line))
                    except json.JSONDecodeError:
                        pass
                inbox.put(None)
            Thread(target=collect, daemon=True).start()

            def send(message):
                proc.stdin.write(json.dumps({'jsonrpc': '2.0', **message}) + '\n')
                proc.stdin.flush()

            def call(request_id, method, params):
                send({'id': request_id, 'method': method, 'params': params})
                while True:
                    message = inbox.get(timeout=25)
                    if message is None:
                        raise RuntimeError(method + ' transport closed')
                    if message.get('method') and 'id' in message:
                        send({'id': message['id'], 'result': {'outcome': {'outcome': 'cancelled'}}})
                    elif message.get('id') == request_id:
                        break
                if 'error' in message:
                    error = message['error']
                    detail = (str(error.get('message', '')) + str(error.get('data', ''))).lower()
                    known = [key for key in ('auth', 'login', 'not logged in', 'signed in', 'api key',
                                              'credential', 'connection', 'spawn', 'enoent', 'exited',
                                              'missing', 'invalid', 'permission', 'unsupported') if key in detail]
                    report.append((method, 'error', error.get('code'), known))
                    return None
                result = message.get('result', {})
                report.append((method, 'ok', result.get('stopReason'),
                               result.get('agentCapabilities', {}).get('loadSession')))
                return result

            try:
                if call(1, 'initialize', {'protocolVersion': 1, 'clientCapabilities': {}}) is None:
                    return None
                result = call(2, 'session/new', base) if session_id is None else call(
                    2, 'session/load', {**base, 'sessionId': session_id})
                if result is None:
                    return None
                session_id = session_id or result['sessionId']
                call(3, 'session/prompt', {'sessionId': session_id, 'prompt': [{'type': 'text',
                      'text': 'If available, call only the Vibe Kanban list_repos tool once. Do not edit files. Then answer COMPLETE.'}]})
                return session_id
            except (BrokenPipeError, RuntimeError, Empty) as exc:
                report.append(('transport', type(exc).__name__))
                return None
            finally:
                proc.stdin.close()
                try:
                    proc.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    subprocess.run(['taskkill', '/PID', str(proc.pid), '/T', '/F'], check=False,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    session_id = turn()
    if session_id:
        turn(session_id)
    methods = trace.read_text(encoding='utf-8').splitlines() if trace.exists() else []
    print(name, report, 'MCP', {key: methods.count(key) for key in ('initialize', 'tools/list', 'tools/call')}, flush=True)
    return len(report) == 6 and all(result[1] == 'ok' for result in report) and methods.count('tools/list') >= 2

with ThreadPoolExecutor(max_workers=4) as pool:
    jobs = [pool.submit(probe, 'claude', 'claude-agent-acp/dist/index.js', 'CLAUDE_CONFIG_DIR'),
            pool.submit(probe, 'codex', 'codex-acp/bin/codex-acp.js', 'CODEX_HOME'),
            pool.submit(probe, 'claude-no-mcp', 'claude-agent-acp/dist/index.js', 'CLAUDE_CONFIG_DIR', False),
            pool.submit(probe, 'claude-local-cli', 'claude-agent-acp/dist/index.js', 'CLAUDE_CONFIG_DIR', True, True)]
    results = [job.result() for job in jobs]
raise SystemExit(0 if all(results[:2]) else 1)
```
