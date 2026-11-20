# ADR-001 单文件桌面架构：Rust 壳 + 内嵌前端 + 嵌入 Python 后端

- **状态**：Accepted（2026-11-20 依据存量代码回填）
- **决策者**：架构（存量架构全能优化模式）

## 背景
目标用户是非开发者，程序必须「双击即用」，不能要求安装 Python / Node；同时核心能力（浏览器自动化 + 大模型打分）在 Python 生态实现成本最低。

## 决策
1. Rust（Tauri 2）作为唯一外壳与进程编排者，负责单实例锁、解包内嵌后端、捕获端口、崩溃日志。
2. Python 后端由 PyInstaller 打成 `aja-backend.exe`，构建期通过 `include_bytes!` 嵌入主程序，运行期解包到本地临时/应用目录再执行。
3. 通信仅走 `127.0.0.1:<随机端口>` 的 HTTP，端口由后端 stdout 上报、Rust 捕获、前端 `invoke('get_backend_port')` 获取。
4. 前端为 Vite 静态产物，构建期写入 `frontendDist=../web/dist`。

## 备选与放弃原因
- **纯 Python 桌面（PyQt/Electron-only）**：PyQt 界面迭代慢、样式能力弱；Electron 体积与内存更大且仍需打包 Python。
- **后端打包成 sidecar 外部 exe**：多文件分发破坏「单文件」体验（`backend-sidecar/` 为旧方案残留，已弃用）。
- **把 `cargo build --release` 当发布命令**：不会执行 `cargo tauri build` 的前端内嵌步骤，导致白屏，故发布只认 `build.bat` / `cargo tauri build`。

## 后果
- ✅ 单文件分发、无需用户装环境。
- ⚠️ 每次改后端需重新走 PyInstaller，构建链长（`build.bat` 五步）。
- ⚠️ 打包态无控制台，故障只能靠 `%LOCALAPPDATA%\ai-job-assistant\logs\crash.log` 与 `data/logs`，故 `get_crash_log_path` 必须保留。
