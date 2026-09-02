# CareerOS Desktop

CareerOS 的本机桌面版本，目前发布 Apple Silicon macOS 和 Windows x64 安装包。生产包使用 Tauri 2、React、Rust、SQLite、内置 Codex App Server 和内置 Typst；安装后不会启动 localhost 服务，也不依赖 Python、Streamlit、Node.js 或 MacTeX。

## 安装与首次启动

1. macOS：打开 `CareerOS.dmg`，把 CareerOS 拖入“应用程序”。当前使用 ad-hoc 签名；如果 macOS 阻止启动，请在 Finder 中右键 CareerOS，选择“打开”，再确认一次。
2. Windows：运行 `CareerOS_*_x64-setup.exe`。当前尚无 Authenticode 证书，首次安装可能出现 SmartScreen 提示。
3. 当前兼容版本继续读取旧技术路径 `~/Library/Application Support/PostdocOS`，因此改名不会复制或丢失现有数据库和材料。
4. 在首次引导或“设置”中连接 ChatGPT/Codex，或使用兼容 Responses 的模型 URL 与 API Key；Gmail 草稿功能需要单独完成 Google OAuth。

应用只会创建 Gmail 草稿并附加已审核 CV，不包含发送邮件的接口。创建草稿也不会自动把申请标记为“已联系”。

## 开发与验证

- `POSTDOCOS_LEGACY_ROOT`：指定旧版 `postdoc-os` 目录。
- `POSTDOCOS_DATA_DIR`：指定隔离的数据目录，供测试迁移使用。
- `./node_modules/.bin/tsc --noEmit`：前端类型检查。
- `./node_modules/.bin/vite build`：前端生产构建。
- 在 `src-tauri` 运行 `cargo test --lib`：Rust、迁移、并发、去重、Gmail MIME 和 Typst 回归。
- `./node_modules/.bin/tauri build`：生成 `.app` 与 DMG。

## GitHub 自动发布

`.github/workflows/release.yml` 只在推送 `v*` tag 时运行。它会先要求 tag、
`package.json`、`src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json` 的版本完全
一致，再分别生成 Apple Silicon ad-hoc signed DMG 和 Windows x64 NSIS 安装
EXE。两个平台都成功后才会创建 GitHub Release，并附带 `SHA256SUMS.txt` 和
第三方归属文件。

在打 tag 前，也可以从 GitHub Actions 手动运行该 workflow 做一次只构建、不
发布的双平台预检；手动运行生成的 DMG/EXE 只保留为 workflow artifacts。

例如当前版本在相关修改已经提交后执行：

```bash
git tag v0.1.0
git push origin v0.1.0
```

当前自动发布不使用 Apple Developer ID、公证或 Windows Authenticode 证书。
macOS 包需要按未识别开发者应用的方式首次打开；Windows 可能显示 SmartScreen
提示。流水线下载固定版本的官方 Codex/Typst 平台二进制，并在打包前验证 SHA256。

第三方模型数据库、账号、能力和路由接口已预留；当前版本仅启用 OpenAI/Codex，未实现的服务商不能被任务选中。

## 第三方集成与服务账号

OpenAI Codex CLI 和 Typst CLI 是随桌面应用分发的独立第三方组件，不属于 CareerOS 自有的 MIT 源码。OpenAI、Codex、ChatGPT 和 Typst 等名称仅用于准确说明兼容性、集成方式和上游来源。

CareerOS 是独立项目，与 OpenAI 或 Typst Project 不存在隶属、联合开发、认可或背书关系。用户通过 ChatGPT/Codex 登录或其他合法 provider/API 使用云服务时，仍须遵守相应服务商的条款、账号资格和 API 使用规则；软件许可证不授予任何云服务权益。

## License

本项目原创源码采用 [MIT License](LICENSE)。MIT License 仅覆盖 CareerOS 自有源码，不覆盖随应用分发的第三方组件。

OpenAI Codex CLI 和 Typst CLI 分别依据各自的 Apache License 2.0 条款独立许可。详细版本、哈希、修改状态、上游归属以及对应 LICENSE/NOTICE 文件见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
