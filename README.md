# Vibe Own

Vibe Own 是基于 [Vibe Kanban](https://github.com/BloopAI/vibe-kanban) 的本地优先 Agent 看板。议题、工作区和执行记录保存在本机 SQLite。Windows 原生应用与 Web 共用同一套 NSH 界面和 Rust API。

- 用看板、列表、分诊、周期和保存视图管理议题，或用 `Ctrl+K` 搜索议题、项目和工作区。
- 为议题启动独立 Git 工作树，查看 Agent 日志、diff 和终端，并从工作区创建 PR。
- 同步 GitHub Issues、查看 CI 检查并发起修复。对配置过的仓库与评审者可自动续接文本评审。
- 管理 ACP 执行器、并发、持久队列、用量、费率和 MCP 配置。

Windows 用户解压完整的 `vibe-own.zip` 后运行 `vibe-own/vibe-own.exe`。开发构建使用下方命令。Web 包可由本机 standalone 服务直接托管，发布资产布局见 [RELEASING.md](RELEASING.md)。

GitHub 操作使用 Git 凭据与 GitHub API 凭据。需要 API 直连时，在应用启动前设置 `VK_GITHUB_TOKEN`。Agent 认证由各自的适配器管理。

交互和性能验收见 [方案与证据](.artifact/goal/2026-11-18-vibe-own-edition/evidence/product-interaction.md)。

## 远程 ACP 节点

默认在本机执行。需要远程执行时，在 POSIX 节点上部署 [`vibe-acp` OpenSSH 子系统](crates/executors/src/executors/acp/trusted_node_subsystem.py)，为专用账户配置固定的 ACP 适配器、模型凭据及所需 MCP 工具。客户端和节点使用同一份可写工作树。节点也需要能读取并更新 linked worktree 的 Git 元数据。跨系统挂载时，确认两端的 Git 都能解析工作树元数据。

应用启动前，将 `VK_ACP_TRUSTED_NODES_FILE` 指向本机的绝对配置路径。例如：

```json
{
  "build-node": {
    "registry_id": "codex-acp",
    "host": "build-node.example.net",
    "user": "vibe",
    "port": 22,
    "identity_file": "C:/vibe-keys/build-node",
    "known_hosts_file": "C:/vibe-keys/known_hosts",
    "local_root": "D:/shared-git",
    "remote_root": "/srv/shared-git"
  }
}
```

在 Agent 配置中编辑对应 ACP 预设，加入 `"trusted_node": "build-node"`，再用该预设启动工作区。将配置、身份文件和已核验的主机密钥保存在共享目录之外。恢复会话时使用原节点及原工作目录。停止任务会发送 ACP 取消。SSH 断连后，节点子系统会结束对应的 Agent 进程组。

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (`nightly-2026-08-01`, see `rust-toolchain.toml`)
- [Node.js](https://nodejs.org/) (>=20.19) and [pnpm](https://pnpm.io/) (10.13.1)
- LLVM (clang and `llvm-ar` for wasm32, libclang for the server build)

```bash
pnpm run nsh:install
pnpm run nsh:install-web
pnpm run dev
```

`dev` packages NSH Web and starts the standalone server with the package as its `VK_WEB_ROOT`.
Open the local URL printed by the server. `dev:qa` enables server QA mode, and on Windows x64
`dev:windows` packages and launches the Win32 app with the server in its own process.

### Building the web app

```bash
pnpm run build:web
pnpm run build:windows  # Windows x64
pnpm run build:npx      # Python 3.11+ required for archiving
```

`build:web` produces `target/package-web/vibe-own/`. Copy the whole directory as `web/` beside the
standalone server, or configure `VK_WEB_ROOT`. `build:npx` archives the server and Web together.
See [RELEASING.md](RELEASING.md) for CLI pinning, native packages and release asset layout.

### Environment Variables

The following environment variables can be configured at build time or runtime:

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `POSTHOG_API_KEY` | Build-time | Empty | PostHog analytics API key (disables analytics if empty) |
| `POSTHOG_API_ENDPOINT` | Build-time | Empty | PostHog analytics endpoint (disables analytics if empty) |
| `PORT` | Runtime | Auto-assign | Standalone server port when `BACKEND_PORT` is unset |
| `BACKEND_PORT` | Runtime | `0` (auto-assign) | Standalone server port, overrides `PORT` |
| `VK_WEB_ROOT` | Runtime | `web/` beside the executable | Directory containing the NSH `index.html` and static assets |
| `HOST` | Runtime | `127.0.0.1` | Backend server host |
| `MCP_HOST` | Runtime | Value of `HOST` | MCP server connection host (use `127.0.0.1` when `HOST=0.0.0.0` on Windows) |
| `MCP_PORT` | Runtime | Value of `BACKEND_PORT` | MCP server connection port |
| `DISABLE_WORKTREE_CLEANUP` | Runtime | Not set | Disable all git worktree cleanup including orphan and expired workspace cleanup (for debugging) |
| `VK_ALLOWED_ORIGINS` | Runtime | Not set | Comma-separated list of origins that are allowed to make backend API requests (e.g., `https://my-vibekanban-frontend.com`) |
| `VK_SHARED_API_BASE` | Runtime | Not set | Base URL for the remote/cloud API used by the local desktop app |
| `VK_SHARED_RELAY_API_BASE` | Runtime | Not set | Base URL for the relay API used by tunnel-mode connections |
| `VK_TUNNEL` | Runtime | Not set | Enable relay tunnel mode when set (requires relay API base URL) |

**Build-time variables** must be set when compiling the server. **Runtime variables** are read when the application starts.

#### Self-Hosting with a Reverse Proxy or Custom Domain

When running Vibe Kanban behind a reverse proxy (e.g., nginx, Caddy, Traefik) or on a custom domain, you must set the `VK_ALLOWED_ORIGINS` environment variable. Without this, the browser's Origin header won't match the backend's expected host, and API requests will be rejected with a 403 Forbidden error.

Set it to the full origin URL(s) where your frontend is accessible:

```bash
# Single origin
VK_ALLOWED_ORIGINS=https://vk.example.com

# Multiple origins (comma-separated)
VK_ALLOWED_ORIGINS=https://vk.example.com,https://vk-staging.example.com
```

### Remote Deployment

When running Vibe Kanban on a remote server (e.g., via systemctl, Docker, or cloud hosting), you can configure your editor to open projects via SSH:

1. **Access via tunnel**: Use Cloudflare Tunnel, ngrok, or similar to expose the web UI
2. **Configure remote SSH** in Settings → Editor Integration:
   - Set **Remote SSH Host** to your server hostname or IP
   - Set **Remote SSH User** to your SSH username (optional)
3. **Prerequisites**:
   - SSH access from your local machine to the remote server
   - SSH keys configured (passwordless authentication)
   - VSCode Remote-SSH extension

When configured, the "Open in VSCode" buttons will generate URLs like `vscode://vscode-remote/ssh-remote+user@host/path` that open your local editor and connect to the remote server.

See the [documentation](https://vibekanban.com/docs/settings/general) for detailed setup instructions.
