# Codex Input Enhancer 项目知识库

## 项目状态

这是 Windows 上的 Tauri 2 + Rust + React 桌面工具，而不是浏览器页面。它已经具备：快捷键读取选区、原生可撤销替换、DeepSeek 连通性测试与流式改写、Agent/知识库按次选择、澄清问答、受限本地读取、可选联网、控制面板及托盘入口。

当前前端是 Vite + React + Ant Design：`src/App.jsx` 负责会话编排，`src/components/PaletteParts.jsx` 负责预览和控制面板，`src/lib/tauri.js` 是 Tauri API 的唯一浏览器侧边界。旧 `ui/` 目录不是当前应用入口，禁止基于它继续开发新功能。

## 交互约束

- 仅热键触发，不常驻读取输入或剪贴板。
- 无选区、空白选区或复制失败时不发送模型请求、不显示替换预览、不替换。
- 浮窗流程是：选择上下文 → 必要时回答澄清问题 → 生成建议 → 用户确认替换。
- 预览始终先于替换；Escape/取消不可改动选区；确认替换必须走剪贴板 + `Ctrl+V`，以保留 Codex 的 `Ctrl+Z`。
- 浮窗不展示工具调用原文，只显示当前阶段状态；模型返回 DSML 或文本形式的工具调用时视为失败，不可作为替换结果。

## 资料与权限模型

设置文件 `%APPDATA%\\codex-input-enhancer\\settings.json` 仅保存模型、Agent 根目录、知识库根目录、默认目录项和网络开关；API Key 仅保存于 Windows Credential Manager（服务 `Codex Input Enhancer`，账户 `deepseek-api-key`）。

目录根下的直接子目录包含 `AGENT.md` 时是 Agent，包含 `INDEX.md` 时是知识库。导入会复制允许的文本文件并拒绝符号链接；模型工具只能读取本次选中的作用域内 `.md`/`.txt` 文件。单文件读取上限 32 KB，目录遍历受深度和数量限制。不得从 Agent/知识库文档执行 shell 命令，也不得让其扩大文件或网络权限。

联网需同时满足：控制面板启用“允许 Agent 联网”，以及当前浮窗启用“本次允许联网”。仅允许公开 HTTP(S)；拒绝 localhost、`.local` 与非 HTTP(S) 地址。联网内容仅作参考，不是系统指令。

## 后端模块地图

| 模块 | 职责 |
| --- | --- |
| `main.rs` | Tauri 命令、会话状态、窗口、托盘、快捷键 |
| `clipboard.rs` | Win32/OLE 选区复制、剪贴板恢复、原生粘贴 |
| `credential.rs` | Credential Manager Key 读取/保存 |
| `settings.rs` | 非敏感设置、目录发现、导入/删除 |
| `workspace.rs` | 受限、只读的本地资料访问 |
| `deepseek.rs` | DeepSeek HTTP 传输和连通性测试 |
| `agent.rs` | 上下文构建、工具循环、澄清、流式生成 |
| `agent_protocol.rs` | 提示词包裹、SSE、DSML 防护 |
| `session.rs` | 会话输入与澄清结果类型 |

## 已知验证要求

Rust 改动需通过 `cargo fmt`、`cargo check`、`cargo clippy -- -D warnings`；前端改动需通过 `npm run build`。完整 `cargo build` 前应先关闭已运行的旧调试程序，否则 Windows 会锁定 exe。手测至少覆盖：有/无选区、读取失败恢复、取消、确认替换、`Ctrl+Z`、Agent/知识库选择、澄清问题、重新生成、设置持久化和网络双重授权。
