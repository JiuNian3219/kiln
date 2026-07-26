# Codex 输入增强器

本地优先的 Windows 桌面应用：将 Codex 中选中的草稿补全为可直接执行的任务提示词。

## 本地开发

前置条件：Node.js 22、Rust stable（含 `rustfmt` 与 `clippy`）以及 Windows WebView2。

```powershell
npm ci
npm run tauri:dev
```

常用命令：

| 命令 | 用途 |
| --- | --- |
| `npm run format` | 按统一规则格式化前端代码与配置 |
| `npm run lint` | 执行 ESLint 静态检查 |
| `npm run build` | 构建前端静态资源 |
| `npm run check:frontend` | 前端格式、静态检查与构建 |
| `npm run check:backend` | Rust 格式、Clippy 与单元测试 |
| `npm run check` | 执行完整前后端校验 |
| `npm run tauri:build` | 构建 Windows 安装包 |

## 工程约定

- 前端使用 Prettier 和 ESLint；提交前应运行 `npm run check:frontend`。
- 后端使用 `rustfmt`、Clippy（警告视为错误）和 `cargo test`；提交前应运行 `npm run check:backend`。
- GitHub Actions 在 Windows 环境对推送到 `main` 和所有 PR 执行同一套校验。
- 编辑器遵从 `.editorconfig`：UTF-8、LF、末尾换行、前端两空格/Rust 四空格。
