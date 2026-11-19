# 循环 008

## 子目标

验证共享产品入口和正式 NSH 包，在隔离工作树验证默认构建切换。

## 候选与决策

共享 UI 由已经打包的 Win32 与 Web 产物直接验证。默认构建与 Web 静态托管先进入独立检出，使运行结果可在对等验收前复核。

## 执行与验证

- 共享入口 `7cbc13b483` 生成两端正式包。原生与 Chromium 按实际控件选工作区，显示真实工作树 diff 和实时日志。关闭原生窗口后，进程退出、回环端口关闭、子进程心跳停止。见 [打包报告](../subagents/m7-package-report.md#共享产品入口复验)。
- Win32 在 5,005 条议题上九次冷启动用时 650–810 ms，包含内嵌服务启动。浏览器同源托管回归中首次键盘打开议题为 723.8 ms，滚动渲染切片 p95 为 1.676 ms。验证用的空工作区曾使 Git 状态返回 HTTP 500，修复后通过实际浏览器复验。
- [构建候选](../evidence/m0-build.md#loop-008-默认构建候选)通过工作区测试、类型生成、默认 check 和 lint。源代码切换留在隔离工作树等待对等验收。
- [真实 ACP 会话](../evidence/m6-acp.md#真实本地模型)通过 OpenCode 与本机 Gemma 4，产品 API 返回 2,046 输入 token、38 输出 token 与零成本。Claude 与 Codex 的认证证据仍待补齐。
- Win32 从键盘创建议题，状态变化由 SQLite 读回。从议题启动已持久化关联、工作树与首轮执行。
- Win32 关闭回归使用独立数据库与工作树。夹具延迟 `after_head_commit` 写入，在状态为 `completed` 且最终 HEAD 仍为空时关闭窗口。进程退出 0、API 端口关闭，SQLite 中最终 HEAD 与工作树提交 `44bc01268bb87018e04afc139b66e96a10af255f` 一致，日志没有 panic。正式包在 `F:/vk-targets/main/product-windows-shutdown/`。
- 正式 Web 包以键盘完成议题、工作区、日志、工作树变更和 PR 表单，保留录像。Win32 包在沙箱提交并跟踪草稿 PR。Chromium 前台滚动渲染 p95 2.712 ms，呈现间隔 p95 19.557 ms，基线 rAF p95 19.6 ms。见 [产品交互证据](../evidence/product-interaction.md)。
- 私有沙箱通过 CI 修复、真实 Codex 推送、绿色状态读回和 Claude 自动文本续接。Claude/Codex 的通用 ACP 新建与会话续接均已有实际适配器证据。
- 合入关闭修正和终端资源依赖后的隔离候选通过默认 `check`、`lint`、生成类型校验和 361 项工作区测试。两个目标的候选打包在同一工作树执行。

## 终止检查

- decision: continue
- reason: Web 键盘、Windows 草稿 PR、GitHub 修复与评审、真实 ACP 适配器和隔离默认构建均取得证据。
- remaining_evidence: Win32 手势串联验收、可选远程节点接线、最终包与 React 退役。
