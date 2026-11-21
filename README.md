# AI 求职助手

桌面端**岗位筛岗工具**：自动抓取 Boss 直聘的岗位信息与 JD 全文，按你的求职画像打分（百分制），
筛出值得投递的岗位。投递动作不在本程序内完成。

技术栈：**Tauri 2（Rust 壳 + 内嵌业务核心） + WebView 界面 + Python 3.14 sidecar（patchright 驱动真实 Chrome） + SQLite**。

业务规则、存储、协议全在同一个 Rust crate 内；唯一的外部进程是**只负责浏览器操作**的 Python sidecar。
打包产物是单个 exe，用户无需安装 Python、Node 或 SQLite（`rusqlite` 的 `bundled` 已把 SQLite 编进二进制）。

工程约定见 [`AGENTS.md`](AGENTS.md)（**动代码前必读第 0 节状态看板**），各模块的目录结构与红线见
[`backend-tauri/AGENTS.md`](backend-tauri/AGENTS.md)、[`frontend-vue/AGENTS.md`](frontend-vue/AGENTS.md)、[`scraper/AGENTS.md`](scraper/AGENTS.md)，
现状与坑见各模块的 `HANDOFF.md`。

---

## 架构

```
frontend-vue/  ──invoke()──▶  backend-tauri/ (Tauri 2 壳)  ──直接调用──▶  backend-tauri/src/core/ (领域规则)
   WebView                   命令层                          存储 · 打分 · 协议
                                │
                                └──stdin/stdout JSON Lines──▶  scraper/ (Python sidecar)
```

| 目录 | 职责 | 边界 |
|---|---|---|
| `frontend-vue/` | 界面：页面、组件、设计系统 | 不碰系统 API、不起子进程，一切经 `invoke` |
| `backend-tauri/` | 窗口、单实例锁、毛玻璃、命令注册 + **内嵌核心 `src/core/`** | 命令层不得写领域规则 |
| `scraper/` | patchright 浏览器自动化：抓取、登录、聊天、求职期望 | 不打分、不落库、不持有业务状态 |

前端与 Rust **同进程**，没有 HTTP、没有端口。sidecar 是 `app` 的子进程，协议是 stdin/stdout 上的 JSON Lines。

**为什么界面跑在 WebView 里**：毛玻璃依赖系统合成层，前端只负责在透明画布上画内容；纯原生（egui 等）
拿不到这套合成效果，且细节动效需要逐帧自己插值。

**为什么 sidecar 必须是 Python**：抓取依赖 `patchright`（Playwright 的反检测分支）+ 本机真实 Chrome，
Rust 侧没有等价物。

---

## 功能

### 抓取
- Boss 直聘（DOM-only 解析，不走接口通道——接口只在首屏触发、翻页恒为空且丢字段）
- 关键词批量搜索；可一键读取 Boss 账号里的求职期望职位填入关键词
- JD 全字段采集：正文、公司规模、融资阶段、行业、在招岗位数、HR 活跃度等 21 个字段
- **实时入库**：每抓到一条合格岗位立刻打分入库，不用等整轮结束
- 翻页用**无限滚动**判定（按卡片数增长），不是「下一页」按钮
- 反爬：本机真实 Chrome + 持久化 profile + patchright stealth，每次请求前随机延时 1.5~9.8 秒

### 筛选与打分（百分制）
- **大模型通道**：任意 OpenAI 兼容接口（DeepSeek / OpenAI / 通义千问 / 本地 Ollama），带初筛线与三级容错解析
- **规则通道**：技能 40 + 薪资 20 + 经验 15 + 方向 15 + 公司质量 10；外包/短期项目信号命中时总分减半
- 大模型失败自动回退规则，不中断流程
- **无 JD 一律否决**（走 `RuleScore::passes()`，不是简单的 `score < min_score`）
- **分值门槛**：低于门槛的 JD 不入库；门槛与满分由 `get_scoring_spec` 下发，前端不硬编码
- 十条排除规则：排除关键词、薪资上下限、学历、工作年限、HR 活跃度、工作制、公司成立年限等
- **敏感库**：已入库与已排除的岗位入库前先去重，避免重复抓取

### 队列与详情
- 三个页签：**全部 / 未排除 / 已排除**；按分数、时间排序
- 单击行展开详情抽屉：打分依据（五维明细）、公司信息、JD 全文
- 浅色 / 深色主题（Liquid Glass 设计语言，两套主题各自调优）

### Boss 消息与打招呼
- 会话列表、历史消息、发送文本、发送简历、撤回
- **打招呼文案由大模型生成 → 你确认后才发**（生成与发送刻意拆成两个命令，生成慢不影响界面）

### 配置
- 配置页可读写：大模型 Key / Base URL / 模型、初筛开关、关键词、单轮上限、分值门槛
- 门槛与上限走 `set_limit`，其余键走 `set_app_config`，**只提交改动过的字段**（后端对 `null` 的语义是「删除该键」）
- API Key 只落数据库，界面回显尾 4 位；未配置时走规则打分，不影响使用

---

## 开发环境

### 前置

- **Rust 工具链**（`cargo` 不在 PATH 时用 `%USERPROFILE%\.cargo\bin\cargo.exe`）
- **Node.js + Tauri CLI**（`backend-tauri/node_modules` 或全局 `npx tauri`）
- Python 3.14 + `pip install patchright`
- 本机安装的 Chrome
- 界面重写为 Vue 3 + Vite + TypeScript 时另需 Node 的包管理器（见 [`frontend-vue/HANDOFF.md`](frontend-vue/HANDOFF.md)）

### 必配的环境变量

```powershell
$env:CARGO_TARGET_DIR = "$env:TEMP\jt-dev"   # 独占目录：MSVC 命令行超限会报 D8050；
                                              # 且工作区内产物继承 Low 完整性标签，启动必报 os error 5
$env:TEMP = "$env:TEMP"; $env:TMP = $env:TEMP # 临时库测试依赖它，不设会报 create_dir_all 权限拒绝
```

### 构建与运行

```powershell
# 桌面端（内嵌当前 frontend-vue/dist）
cd backend-tauri
npx tauri build --no-bundle

# 把产物拷到工作区外再启动
Copy-Item backend-tauri/target/release/jobassistant-app.exe "$env:TEMP\jobassistant-app.exe"
& "$env:TEMP\jobassistant-app.exe"
```

> **必须用 `tauri build`**：`cargo build --release` 不内嵌前端，运行会白屏。
> **必须运行工作区外的产物**：工作区内的 exe 被降为 Low 完整性级别，写不了 `%LOCALAPPDATA%`，
> 启动即报 `os error 5`。

### 只改界面时的快循环

界面是 WebView 加载的静态产物，**改完不重新构建壳是看不到的**。当前 `tauri.conf.json` 的
`beforeBuildCommand` 为空串，所以必须先单独构建前端再打壳：

```powershell
cd frontend-vue && trunk build    # Leptos 版；清掉 CARGO_TARGET_DIR 再跑，trunk 不认 --target-dir
cd ../app && npx tauri build --no-bundle
```

---

## 登录

1. 在「概览」页的抓取面板点 **扫码登录**；
2. 程序弹出 Chrome 窗口停在 Boss 登录页（验证码需手动处理）；
3. 登录成功后关掉浏览器窗口即可，后续复用同一份持久化 profile。

程序使用专属 Chrome profile（`%LOCALAPPDATA%\ai-job-assistant\data\browser-profile`），与日常浏览器互不影响。

> 同一时间只能有一个实例占用该 profile，否则报 `Target page, context or browser has been closed`。
> 若登录窗口一闪就白屏：多半是被 Boss 风控识别，检查是否设了 `AJA_NO_STEALTH=1` 或改小了延时。

---

## 大模型配置

「配置」页填写（任意 OpenAI 兼容接口）：

| 服务商 | Base URL | Model |
|---|---|---|
| DeepSeek | `https://api.deepseek.com` | `deepseek-chat` |
| OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` |
| 通义千问 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-plus` |
| 本地 Ollama | `http://127.0.0.1:11434/v1` | `qwen2.5:14b` |

API Key 只落数据库 `app_config` 表，界面只回显尾 4 位。未配置时走规则打分，不影响使用。

---

## 数据

SQLite 单文件（开发态在项目根 `data/`，打包态在 `%LOCALAPPDATA%\ai-job-assistant\data\`）：

| 表 | 用途 |
|---|---|
| `job_queue` | 岗位队列：JD 全文、公司侧字段、`match_score` / `score_source` / `score_reason` / `rejected` |
| `jd_decisions` | 每条岗位的筛选决策与理由（「决策记录」页的数据源） |
| `sensitive_jobs` | 敏感库：已入库 / 已排除，抓取前先去重 |
| `app_config` | API Key、模型、排除规则、登录 cookie、门槛 |

`data/` 含登录 cookie 与 Chrome profile，已被 `.gitignore` 忽略，**不得** `git add -f`。
**动真实库之前先备份**（`Copy-Item` 到 `data/backup/` 并核对 SHA256）；迁移必须整体事务。

---

## 界面开发约定

细节见 [`frontend-vue/AGENTS.md`](frontend-vue/AGENTS.md)，这里只列最容易踩的几条：

- 依赖方向单向：`pages → features → components → lib_ui`，`api` 可被任意层引用，`lib_ui` 不得反向引用
- **禁止硬编码满分与门槛**，一律由 `get_scoring_spec` 下发
- **页面背景必须透明**，否则毛玻璃失效
- 可交互控件必须带 `data-testid`，命名 `<页面/模块>-<功能>-<控件类型>`
- **CSS 文件不许用 `//!` / `///` 注释**——CSS 解析器会判整份样式表非法并静默丢弃
- 设计语言：Liquid Glass，令牌在 `frontend-vue/styles/tokens.css`，深/浅双主题各自调优；
  装载的技能在 `frontend-vue/.agents/skills/`，来源记录在 `SOURCES.md`

---

## 验证门禁

```powershell
cd backend-tauri && cargo test                          # 后端单测
cd frontend-vue && cargo check --target wasm32-unknown-unknown   # 前端静态检查（Leptos 版）
python -m compileall -q scraper                         # sidecar 语法检查
cd backend-tauri && npx tauri build --no-bundle                   # 产物构建（必须用 tauri build）
```

门禁不通过的提交不允许 `git commit`。用例数、构建体积这类会变动的数字不写进文档，现场跑。

---

## 已知限制

- 只接入 Boss 直聘；站点改版后需同步 `scraper/boss.py` 的 DOM 选择器
- 频繁抓取会触发 Boss 风控（页面停在「确认中…」，日志提示「第 1 页没有拿到任何岗位」）。程序已加 1.5~9.8 秒随机延时，仍需控制频率
- 薪资筛选：Boss 的 `salary` 参数是模糊命中，程序会本地再按区间精确过滤一次
- 不含简历上传（在线简历通过 Boss 既有的简历卡片发送）
- **从 Tauri 子进程拉起真实 Chrome 仍会失败**（WebView2 与 sidecar 的 Chrome 撞 Chromium 单例命名对象），单独运行 `scraper/sidecar.py` 时正常。详见 [`scraper/HANDOFF.md`](scraper/HANDOFF.md)
- **登录入口尚未接线**：三条登录命令后端已就绪，界面还没有调用点
- **界面正在从 Leptos 重写为 Vue 3**：`frontend-vue/` 当前是 Leptos 版，重写方案见 [`ARCH-003`](docs/architecture/ARCH-003-vue-frontend-migration.md)
- Liquid Glass 第二层（SVG 湍流折射）与第三层（WebGL 卡片间真折射）未做，只有第一层 CSS 玻璃

---

## 许可

第三方前端技能的许可与来源见 [`frontend-vue/.agents/skills/SOURCES.md`](frontend-vue/.agents/skills/SOURCES.md) 与同目录 `LICENSE`。
