# Markdown PDF Desktop

Windows 本地 Markdown 批量编辑与 PDF 导出程序。桌面壳使用 Tauri 2 + 系统 WebView2，Markdown、主题和 PDF 都在本机处理；程序不启动 HTTP 服务，也不监听端口。页面侧的 DNS 与 HTTP/HTTPS 请求被黑洞规则和本机拒绝代理阻断。

## 主要功能

- 打开一个本地文件夹，递归展示 `.md` / `.markdown` 文件。
- 中间编辑、右侧即时预览，默认 850 ms 防抖自动保存。
- 使用文件 `mtime` 做乐观并发检查，避免覆盖外部程序刚写入的版本。
- 多选文件后先统计批量替换数量，再执行替换。
- 批量替换前复制原文件到 `<工作区>/.mdpdf-backup/<时间>/`。
- 按工作区相对目录串行导出 PDF，避免同时启动多个 Edge 进程争抢内存。
- 实时预览在前端 WebView2 内通过 Rust WebAssembly + Comrak 完成，主题、封面、目录、分页和差分逻辑随 WASM 一起内置；Mermaid 11.16.0 作为本地静态资源内置。
- WASM 使用 `simd128`，原生渲染在 x86-64 上运行时检测并使用 AVX2；Markdown 解析结果、正则和主题样式采用单项热缓存，Mermaid/差分替换均为缓存友好的单遍顺序扫描。

## 数据布局与执行边界

侧栏只长期保存一个连续的 `Vec<DocumentEntry>`：绝对路径、相对路径、文件名、大小和修改时间。所有 Markdown 正文都不常驻批量列表，只在当前文档预览、保存或单个导出任务中读取。

```text
文件夹扫描 -> Vec<DocumentEntry>
                   |
                   +-> 当前文件 String -> WebAssembly/Comrak -> WebView2 srcdoc
                   |
                   +-> 选择的路径数组 -> 逐文件备份/替换
                   |
                   +-> 逐文件 HTML -> 本机 Edge headless -> PDF
```

没有上传副本、Axum、Caddy、Docker 端口映射、HTTP 轮询或数据库。批量 PDF 由单许可证 `Semaphore(1)` 串行化；过载时等待，不会无限并发创建 Chromium 进程。

## Windows 构建

前置条件：

- Windows 10/11（NSIS 安装包内置 Microsoft Edge WebView2 离线运行时）
- Visual Studio 的“使用 C++ 的桌面开发”工作负载
- Rust stable MSVC toolchain

```powershell
rustup default stable-msvc
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127 --locked
npm run build:wasm
cargo test
cargo build --release
```

可执行文件：

```text
target\release\mdpdf-desktop.exe
```

生成可整体复制到其他 Windows 10/11 机器的目录（必须连同 `themes` 一起复制）：

```powershell
.\scripts\package_portable.ps1
```

输出位于 `dist\Markdown-PDF-Desktop\`。主题、样式、Prism、Mermaid 和 WASM 均编译进程序/前端资源，目标机器不需要额外的 `themes` 目录，也不需要 Cargo、Node.js 或 `mmdc`。

构建 NSIS 安装包时安装 Tauri CLI 后执行：

```powershell
cargo install tauri-cli --version "^2"
cargo +stable-x86_64-pc-windows-msvc tauri build --bundles nsis
```

`tauri.conf.json` 的 WebView2 安装模式为 `offlineInstaller`。安装包包含 WebView2 离线运行时，即使目标机器没有预装 WebView2，也能直接安装；代价是安装包体积增加约 127 MB。应用主题、样式、Prism、Mermaid 和 WASM 均编译进程序，不需要额外复制 `themes` 目录。

## 使用

1. 启动 `mdpdf-desktop.exe`。
2. 点击“フォルダを開く”，选择 Markdown 文档目录。
3. 单击文件编辑；勾选复选框建立批量选择集。
4. 批量替换必须先点“置換件数を確認”，条件没有变化时才允许执行。
5. 批量 PDF 会保留工作区目录结构写入所选输出目录。

Mermaid 默认关闭。勾选后由程序内置的 Mermaid 在 WebView2 中直接生成 SVG，预览和 PDF 都不调用 `mmdc`，不需要 Node.js，也不会下载 CDN 资源。运行时只在含 Mermaid 的 PDF 中注入，并在进程内缓存一份脚本文本，普通 Markdown 不承担这部分复制成本。

## 验证

```powershell
cargo fmt -- --check
cargo test
cargo test --manifest-path wasm-renderer\Cargo.toml
node --check public\app.js
node scripts\wasm_smoke.mjs
npm run bench:wasm
.\scripts\windows_smoke.ps1 -ExePath target\release\mdpdf-desktop.exe
```

`windows_smoke.ps1` 会启动程序三秒，确认主进程存活、主进程没有 TCP/UDP endpoint，并统计 WebView2 子进程的 endpoint，然后关闭测试进程。

默认烟雾测试要求应用主进程没有 endpoint，并单独报告 WebView2 Runtime 的 endpoint 数量。要检查严格 air-gap 条件，执行：

```powershell
.\scripts\windows_smoke.ps1 -ExePath target\release\mdpdf-desktop.exe -RequireNoNetwork
```

严格模式在当前 Evergreen WebView2 上可能失败：微软说明 WebView2 会依据 Windows“诊断数据”设置收集部分必需/可选诊断数据，而且应用不能控制全部诊断收集。本程序已关闭 SmartScreen、组件更新、后台页面网络，并黑洞页面代理/DNS；若环境要求整个 WebView2 进程树绝对零外联，仍需由 Windows Firewall/组策略在部署端阻断，或改用不基于 WebView2 的原生 UI。这是平台边界，不应把“应用无需网络”误写成“系统运行时绝不联网”。
