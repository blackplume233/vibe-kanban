# 循环 006

## 子目标

验收 M1 的锁定 nightly 路线闸门，并集成可复用的议题 API 和看板状态。

## 候选与决策

- A. 使用 Rust 1.99.1 已有运行证据，等待产品包验证 nightly。
- B. 在独立工作树用锁定 nightly 重跑双端闸门，分别保留指标和截图。

选 B。一次通过同一版后端与客户端，可在产品 UI 合入前确认依赖调整和现场运行。

## 执行与验证

- `30c38899d` 的 Win32 Release、WASM Release 和后端通过 `m1-gate.sh both`。双端接通 HTTP 与 WS，在同一 Git 工作区显示追加日志与 diff。Win32 的 5000 行 `virtual_list` 滚动 `frame` p95 为 5.77 ms。Chrome rAF 间隔 p95 为 19.60 ms。日志和截图见 [M1 闸门](../evidence/m1-gate.md)。
- M4 提交 `76e44c994` 已接入共用请求方法、议题 API 和乐观看板状态。切片的 19 项 Windows 测试与 WASM 编译检查通过，产品界面验证仍在进行。
- M7 隔离切片分别覆盖启动与打包、PTY、工作区及会话操作。工作区切片负责 SQLite 议题关联和结构化 HTTP 错误，会话切片负责共享 WebSocket 通路。
- GitHub 测试仓库已读到 PR 提交的失败状态。当前凭据无法写入 Actions workflow。以外部 CI 在对应提交实际运行测试并通过 Commit Statuses 上报，保留失败、Agent 修复推送和恢复的可追溯记录。

## 终止检查

- decision: continue
- reason: M1、M2 已通过。其余里程碑仍需产品集成或最终证据。
- remaining_evidence: 产品键盘流程、议题列表帧耗时、双端冷启动、M3 展示页、GitHub 往返、ACP 迁移、正式包与默认构建退役、E1–E7 终止审计。
