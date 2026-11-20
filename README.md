# AI 求职助手

桌面端**岗位筛岗工具**：自动抓取 Boss 直聘的岗位信息与 JD 全文，按你的求职画像打分（1-10 分），
筛出值得投递的岗位放进「待投递队列」。投递动作不在本程序内，由使用队列数据的一方自行完成。

技术栈：**Tauri 2（Rust 壳） + Vite + React + TypeScript（界面） + Python FastAPI（抓取后端） + patchright（Playwright 反检测版） + SQLite**

打包产物是**单个可执行文件**：Python 后端被 PyInstaller 打成 exe 后由 Rust 编译期 `include_bytes!` 嵌入主程序，双击即用，无需安装 Python 或 Node。

---

## 功能

### 抓取
- **Boss 直聘**（已接入；智联招聘 / 51job 在界面置灰未接入）
- 筛选项与 Boss 官网一致，**全部多选**：城市、职位类型、薪资（6 档 + 自定义区间）、工作经验、学历、公司规模、融资阶段、行业、搜索最大翻页次数
- **关键词批量搜索**：输入框回车/逗号分隔添加多个关键词，逐个搜索；可一键「读取期望职位」拉取 Boss 账号里已配置的求职期望，或从推荐词里选
- **JD 全字段采集**：JD 正文、公司规模、融资阶段、行业、在招岗位数、成立年限、注册资本、HR 活跃度、公司地址、公司介绍
- **达不到数量就继续**：一轮搜完未达「有效 JD 上限」会自动刷新页面重新搜索，直到达标或没有新岗位
- **实时入库**：每抓到一条合格岗位立刻入库并打分，不用等整轮结束
- **暂停 / 继续 / 停止**：暂停后浏览器停在当前 JD 页面方便人工核对，停止指令秒级生效
- **反爬**：使用本机真实 Chrome + 持久化 profile + patchright stealth，每次请求前随机延时 1.5~9.8 秒

### 筛选与打分
- **打分（满分 10）**：
  - 配置了大模型（任意 OpenAI 兼容接口：DeepSeek / OpenAI / 通义千问 / 本地 Ollama）→ 用大模型打分并给出理由；
  - 未配置 → 规则打分兜底：技能 40 + 薪资 20 + 经验 15 + 方向 15 + 公司质量 10，外包/短期项目信号总分减半；
  - 大模型调用失败自动回退规则，不中断流程。
- **分值门槛**：低于设定分数的 JD 直接不入库（默认 7 分）
- **排除规则**（任意一项留空即表示不按该项排除）：
  - 排除关键词（命中公司名或岗位名即排除，空格/逗号/顿号/分号均可分隔，默认带 32 个词）
  - 薪资下限低于 / 薪资上限高于
  - JD 岗位要求的学历（大专 / 本科 / 硕士 / 博士）
  - JD 要求的工作年限
  - HR 活跃度（今日 / 3 天内 / 一周内）
  - 工作制（只投双休 / 排除大小周单休）
  - 公司成立年限
- **敏感库**：所有「已入库」与「已排除」的岗位都会记入敏感库，下一轮抓取先查它直接跳过，避免重复劳动。敏感库可搜索、按类型筛选、多选移出。**注意：移出一条记录后，该岗位下一轮会被当成新岗位重新评估。**

### 队列
- 三个状态页签：**未投递 / 已投递 / 不投递**，卡片多列瀑布流，滚动加载（每次 100 条）
- 每张卡片：分数徽章、职位名、公司、薪资、平台、城市/经验/学历/规模、公司地址；**单击展开**看公司详情、打分依据与 JD 全文
- 行内「不投递 / 允许投递」标签单击即切换（点击后播 2 秒移出动画）；勾选 2 条以上出现批量操作
- 支持按加入时间 / 薪资 / 分值排序，正序倒序切换
- 浅色 / 深色主题

---

## 目录结构

```
ai-job-assistant/
├── backend/                     # Python FastAPI 后端
│   ├── entry.py                 # PyInstaller 入口（包一层绝对导入）
│   ├── aja-backend.spec         # PyInstaller 打包配置（含 patchright driver）
│   ├── requirements.txt
│   ├── app/
│   │   ├── main.py              # FastAPI 入口 + 抓取任务编排 + 24 个接口
│   │   ├── store.py             # SQLite 存储层（job_queue / jd_decisions / sensitive_jobs / app_config）
│   │   ├── scorer.py            # 大模型 / 规则 双通道打分
│   │   ├── browser.py           # patchright 启动与复用浏览器、登录会话、profile 处理
│   │   └── cookie_import.py     # 从 Chrome profile 读 cookie（登录态兜底）
│   └── scrapers/
│       ├── base.py              # 抓取器基类（限速、暂停/停止信号）
│       └── boss.py              # Boss 直聘解析（接口拦截 + DOM 兜底）
├── web/                         # Vite + React + TS + Tailwind 前端
│   └── src/{App.tsx, api.ts, index.css, main.tsx}
├── src-tauri/                   # Tauri 壳（Rust）
│   ├── build.rs                 # 决定是否把后端 exe 嵌进主程序
│   └── src/{lib.rs, commands.rs, main.rs}
├── tools/make_icons.py          # 生成应用图标
├── build.bat                    # 一键构建
└── data/                        # 运行时生成（不入库）
```

---

## 开发环境

### 1. 后端（Python 3.11+）

```bash
cd backend
pip install -r requirements.txt      # fastapi / uvicorn / patchright / openai / pydantic
python -m app.main                   # 会打印 AJA_BACKEND_PORT=<port>
```

可用环境变量：

| 变量 | 说明 |
|---|---|
| `AJA_PORT` | 指定端口；不填则随机 |
| `AJA_DATA_DIR` | 数据目录；默认打包时用 exe 同级 `data/`，开发时用项目根 `data/` |
| `AJA_PROXY` | 需要代理时才设置，默认直连 |
| `AJA_NO_STEALTH` | 设为 1 时关闭 patchright stealth（调试用，**开着会被 Boss 风控**） |

### 2. 前端（Node 18+）

```bash
cd web
npm install
npm run dev                          # http://127.0.0.1:5183
```

浏览器里单独调试时，用 `VITE_AJA_PORT=<后端端口>` 指定后端；在 Tauri 里运行时端口由 Rust 侧自动注入。

### 3. 桌面端

```bash
cargo install tauri-cli --version "^2.0"      # 或 npm i -g @tauri-apps/cli

# 开发（热重载）
cd web && npm run dev                          # 终端 A
cd src-tauri && cargo tauri dev                # 终端 B
```

### 4. 打包成单文件 exe

直接双击根目录的 **`build.bat`**，或手动执行：

```bash
cd web && npm run build                        # -> web/dist
cd ../backend && python -m PyInstaller aja-backend.spec --noconfirm --clean
cp dist/aja-backend.exe ../backend-exe/        # 内嵌用副本
cd ../src-tauri && cargo tauri build
```

产物：
- `src-tauri/target/release/ai-job-assistant.exe` —— **单文件，双击即用**
- `src-tauri/target/release/bundle/nsis/*.exe` —— NSIS 安装包

> 打包前务必先关掉正在运行的 `ai-job-assistant.exe` / `aja-backend.exe`，否则 `cargo tauri build`
> 会报「另一个程序正在使用此文件 (os error 32)」。
>
> 若只用 `cargo build --release`（而不是 `cargo tauri build`），前端不会内嵌，程序会去连开发服务器，
> 表现为白屏 / `ERR_CONNECTION_REFUSED`。

---

## 登录

1. 顶栏「招聘网站」里点 **Boss直聘** 右侧的绿色「登录」；
2. 程序弹出一个 Chrome 窗口停在 Boss 登录页（验证码需手动处理）；
3. 登录成功后状态点变绿，程序把 cookie 存进数据库（`app_config.last_auth`）；
4. **直接关掉浏览器窗口即可**，后续抓取复用同一份持久化 profile 的登录态；
5. 点「退出」可清除该平台登录态。

> 浏览器用的是本机安装的 Chrome（`channel="chrome"`），找不到时回退到内置的 patchright。
> 程序使用专属的 Chrome profile（`data/browser-profile`），与你的日常浏览器互不影响。
>
> **同一时间只能有一个程序实例占用 `data/browser-profile`**，否则会报
> `Target page, context or browser has been closed`。
>
> 若登录窗口一闪就白屏：那是被 Boss 反爬识别了。本项目已用 patchright + 真实 Chrome 规避，
> 如果仍出现，检查是否设置了 `AJA_NO_STEALTH=1` 或改动了 `browser.py` 的启动参数。

---

## 大模型配置

顶栏「模型与简历配置」里填写（任意 OpenAI 兼容接口）：

| 服务商 | Base URL | Model |
|---|---|---|
| DeepSeek | `https://api.deepseek.com` | `deepseek-chat` |
| OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` |
| 通义千问 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-plus` |
| 本地 Ollama | `http://127.0.0.1:11434/v1` | `qwen2.5:14b` |

保存后顶栏标签从「规则打分（不消耗 token）」变成「大模型打分 · N token」，N 是累计消耗的 token 数；
对已入库岗位可点「重新打分」批量重算。

---

## 后端接口

后端默认只监听 `127.0.0.1`，供桌面端自己调用：

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/health` | 探活 |
| POST | `/api/start` | 开始抓取（筛选条件见 `StartReq`） |
| POST | `/api/stop` `/api/pause` `/api/resume` | 停止 / 暂停 / 继续 |
| GET | `/api/status` | 运行状态、进度、统计、登录态 |
| POST | `/api/limit` | 设置有效 JD 上限与分值门槛 |
| GET | `/api/expectations` | 读取 Boss 账号里的求职期望 |
| GET | `/api/platforms` | 平台列表 + 登录态 |
| POST | `/api/login/start` `/api/login/save` `/api/login/logout` | 登录窗口 / 保存登录态 / 退出登录 |
| GET | `/api/login/status` | 登录会话状态 |
| GET | `/api/jobs` | 岗位列表（分页、排序、按状态过滤） |
| POST | `/api/jobs/reject` `/api/jobs/allow` `/api/jobs/deliver` | 不投递 / 允许投递 / 标记已投递 |
| GET | `/api/sensitive` | 敏感库列表（可按平台、关键词、类型过滤） |
| POST | `/api/sensitive/delete` `/api/sensitive/clear` | 移出所选 / 清空 |
| GET | `/api/logs` | 运行日志（增量拉取） |
| GET/POST | `/api/config` | 读写配置（大模型、排除规则、上限、门槛） |
| POST | `/api/rescore` | 重新打分 |

---

## 数据库

运行时自动生成 `job_assistant_pro.db`（开发态在项目根 `data/`，打包态在
`%LOCALAPPDATA%\ai-job-assistant\data\`）：

| 表 | 用途 |
|---|---|
| `job_queue` | 岗位队列：JD 全文、公司侧字段、`match_score` / `score_source` / `score_reason` / `rejected` / `token_used` |
| `jd_decisions` | 每条岗位的筛选决策与理由（决策日志流的数据源） |
| `sensitive_jobs` | 敏感库：已入库 / 已排除的岗位，抓取前先去重 |
| `app_config` | API Key、模型、排除规则、登录 cookie（`last_auth`）、上限与门槛 |

---

## 打分口径

求职者画像写在 `backend/app/scorer.py` 的 `PROFILE` 常量里（年限、期望薪资、城市、核心/加分技能），
按需修改即可。默认画像：约 10 年 Java 后端 / 全栈，期望 15-35K，核心技能
Java / Spring Boot / Spring Cloud / MyBatis / MySQL / Redis / RabbitMQ。

规则打分维度：

| 维度 | 满分 | 说明 |
|---|---|---|
| 技能 | 40 | JD 命中核心技能权重 1.0、加分技能 0.5，封顶 40；无 JD 时降权给 12 |
| 薪资 | 20 | 与期望区间 [15K, 35K] 的重合比例；超出区间按差距递减 |
| 经验 | 15 | 与 10 年经验贴合度：8-12 年 15 分，5-8 年 12 分，其余递减 |
| 方向 | 15 | Java / 全栈 / 后端 / 服务端 15 分，Node/Go/Python 10 分 |
| 公司质量 | 10 | 外包、驻场、短期项目、项目制直接 0 分 |

外包 / 短期项目信号命中时**总分再减半**（避免「看起来匹配」的外包岗拿高分）。

---

## 会话与自动打招呼（已实现）

界面右侧「JD 智能对话 / 会话面板」不是占位，已接通的接口：

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/chat/friends` | 会话列表（Boss「沟通」页好友） |
| GET | `/api/chat/messages` | 单个会话的消息（可强制刷新，默认 10 分钟缓存） |
| POST | `/api/chat/send` | 发送文本消息 |
| POST | `/api/chat/open` | 打开 / 主动发起某个岗位的会话 |
| GET/POST | `/api/chat/resumes` `/api/chat/resume` | 读取在线简历列表 / 发送简历 |
| POST | `/api/chat/recall` | 撤回消息 |
| POST | `/api/greet` | 生成打招呼文案（大模型；未配置或失败走规则兜底） |
| GET | `/api/auto-greet/next` `/api/auto-greet/one` | 自动打招呼：取下一个候选 / 对单个执行 |

消息能力跑在**独立车道**（独立线程 + 独立 Playwright 客户端，连同一台 Chrome），
所以采集任务跑几分钟时照样能收发消息、发招呼。注意：**同机只能开一个程序实例**，否则抢 `data/browser-profile` 会报
`Target page, context or browser has been closed`。

---

## 已知限制

- 只接入了 Boss 直聘；智联招聘 / 51job 在界面上置灰，`backend/app/browser.py` 里仍保留它们的登录页与
  cookie 名单，接入时补一个 scraper 即可。
- Boss 页面结构与接口字段会变，改版后需调整 `backend/scrapers/boss.py` 的解析逻辑
  （目前接口拦截 + DOM 兜底两条路径）。
- 频繁抓取会触发 Boss 风控：页面停在「确认中…」，接口与 DOM 都为空，日志会提示
  `第 1 页没有拿到任何岗位`。程序已加 1.5~9.8 秒随机延时，但仍需控制频率，必要时等几分钟再试。
- 薪资筛选：Boss 自己的 `salary` 参数是模糊命中的，程序会本地再按你勾选的区间精确过滤一次。
- **不含简历上传**（在线简历通过 Boss 既有的简历卡片发送），投递动作不在本程序内完成。
- 暂无自动化测试：改动前请先看 `AGENTS.md` 第 4 节的三层验证门禁。

---

## 工程约定（给 AI / 协作者）

仓库根目录的 [`AGENTS.md`](AGENTS.md) 是本项目的工程宪法（状态看板、目录写权限边界、验证门禁、提交规范），
需求与接口契约见 `docs/specs/SPEC-001-mvp.md`，架构决策见 `docs/adr/`。动代码前先读宪法第 0 与第 2 节。
