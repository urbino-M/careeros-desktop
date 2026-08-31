# PostdocOS Desktop

PostdocOS 的 Apple Silicon 本机桌面版本。生产包使用 Tauri 2、React、Rust、SQLite、内置 Codex App Server 和内置 Typst；安装后不会启动 localhost 服务，也不依赖 Python、Streamlit、Node.js 或 MacTeX。

## 安装与首次启动

1. 打开 `PostdocOS.dmg`，把 PostdocOS 拖入“应用程序”。
2. 首版尚未签名。如果 macOS 阻止启动，请在 Finder 中右键 PostdocOS，选择“打开”，再确认一次。
3. 首次启动会把旧版数据库、材料、历史版本和配置复制到 `~/Library/Application Support/PostdocOS`，并先建立时间戳备份。旧项目保持不变。
4. 在“设置”中登录 ChatGPT/Codex 或保存 OpenAI API Key；Gmail 草稿功能需要单独完成 Google OAuth。

应用只会创建 Gmail 草稿并附加已审核 CV，不包含发送邮件的接口。创建草稿也不会自动把申请标记为“已联系”。

## 开发与验证

- `POSTDOCOS_LEGACY_ROOT`：指定旧版 `postdoc-os` 目录。
- `POSTDOCOS_DATA_DIR`：指定隔离的数据目录，供测试迁移使用。
- `./node_modules/.bin/tsc --noEmit`：前端类型检查。
- `./node_modules/.bin/vite build`：前端生产构建。
- 在 `src-tauri` 运行 `cargo test --lib`：Rust、迁移、并发、去重、Gmail MIME 和 Typst 回归。
- `./node_modules/.bin/tauri build`：生成 `.app` 与 DMG。

第三方模型数据库、账号、能力和路由接口已预留；当前版本仅启用 OpenAI/Codex，未实现的服务商不能被任务选中。
