# 开发进度记录

> 本文件记录 JobAssistant 每一轮的改动、踩过的坑和待办。追加式维护，最新的在最上面。

## 2026-11-20 工程治理补齐：AGENTS.md 宪法 + SPEC 冻结 + 清理敏感库清空残留

### 现象
`POST /api/sensitive/clear`（界面「清空敏感库」）会抛 `AttributeError: 'Store' object has no attribute '_lock'`。

### 根因
`backend/app/store.py` 早期是「单连接 + `self._lock` / `self._conn()`」实现，后来重构成每线程独立连接
（`threading.local` + `conn()` / `tx()`），`clear_sensitive()` 是唯一没跟着改的方法。

### 改动
- `backend/app/store.py`：`clear_sensitive()` 改用 `with self.tx() as c:`，与其它写方法一致。
- 新增 `AGENTS.md`（根目录工程宪法，六节：状态看板 / 技术栈 / 目录边界 / 契约同源 / 三层验证门禁 / Git 规范）。
- 新增 `docs/specs/SPEC-001-mvp.md`（状态 `SPEC_FROZEN`）与 `docs/adr/ADR-001`、`docs/adr/ADR-002`。
- `README.md`：补「会话与自动打招呼（已实现）」接口表，更正「JD 智能对话是占位」的过时描述，新增「工程约定」。

### 验证
- `python -m compileall -q app scrapers entry.py` 通过。
- `Store(tmp).clear_sensitive()` 单点验证通过（见本轮终端输出）。

### 待办
- 引入 pytest（store 去重/筛选、scorer 规则打分、薪资解析）与 vitest（`api.ts` 端口解析），回填 `AGENTS.md` 第 4 节槽位 B。
- 决定 E2E 方案后回填槽位 C。

## 2026-11-19（深夜 4）自动打招呼误报「已发满本轮上限 0 条」+ 顶栏状态高亮

### 现象
- 点「自动打招呼 开」后立刻停下，日志：「自动打招呼：已发满本轮上限 0 条，自动打招呼已停止」。
- 用户明确要求：自动打招呼只看「待投递」列表，**列表空了才停**，不该有别的上限。

### 根因
- 前端 `tick()` 里有一条 `if (autoDoneRef.current.size >= maxJdRef.current)`，
  把抓取用的「每日 JD 上限 `maxJd`」也当成了本轮发送上限；而 `max_jd` 在库里是 **0**（0 = 不限制），
  于是 `0 >= 0` 立刻成立 → 开一次就停。
- 顺带两处错：计数用的是 `autoDoneRef`（只装「跳过」的岗位，成功发送并不往里加），
  所以上限判断本来也数不准；停止时只 `setAutoOn(false)`、没同步 `autoOnRef`。

### 改动（`web/src/App.tsx`）
- 删掉上限判断：现在只有三种停止——**待投递列表发完**、连续失败 3 次自动暂停、用户手动关闭。
- 新增 `stopAuto(message, level)`：同时清 `autoOnRef`、`autoNextAt` 和 `autoOn`，
  所有停止分支（取不到岗位 / 列表发完）统一走它，避免定时器拿旧 ref 再排一轮。
- 新增倒计时显示：`autoNextAt` + 每秒跳动的 `autoNow`，算出 `autoNextIn`。
- 顶栏「自动打招呼」按钮：运行中是**实心绿**，暂停是**实心琥珀**；旁边多一个状态胶囊
  （绿色呼吸点 +「12s 后发下一条」/「发送中…」/「已暂停」+「· 已发 N 条」），
  悬停看明细。暂停时点「继续」会清零连败计数。
- 日志里不再出现「上限」字样，开启时提示「按待投递列表逐条发，发完自动停止」。

### 验证
- `npm run build` 通过（`index-CxqdvYGf.js` 296.88 kB）；`cargo tauri build` 通过。
- 桌面 exe 已换新（71,005,184 字节，md5 `2a59fa74672ac8657d344f3668395568`）；
  `/api/auto-greet/next` 返回 11 条待投递，重启后循环能正常跑起来。

## 2026-11-19（深夜 3）关掉 Chrome 窗口后消息/打招呼不再报错：自动重新打开浏览器

### 现象
用户：「自动回复 窗口关闭后不能继续」。手动关掉采集用的 Chrome 窗口后，消息列表、打招呼一直报
「浏览器窗口已关闭或崩溃，下次操作会自动重新打开」，但**从来没有真的重新打开过**，
自动打招呼连败 3 次就自己暂停了。

### 根因
- `Runtime.ensure_browser()` 里其实有探活逻辑（`ctx.pages` 抛错 → 重启 Chrome），
  但**只有走采集线程的路径才会调用它**。
- 消息走 `ChatLane.ensure_ctx()`：它自己 `connect_over_cdp(端口)`，浏览器没了就连不上；
  原来这里直接 `raise`，从不请求采集线程重新拉起浏览器。
- `_run_chat()` 的前置检查是 `if rt._ctx is None` 才 `ensure_main_browser()`；
  窗口被关掉后 `_ctx` 仍是**过期但不为 None**，检查被跳过 → 永远没有重启的机会。
- 因此那句「下次操作会自动重新打开」是空头承诺。

### 改动（`backend/app/main.py` 的 `ChatLane`）
- `ensure_ctx()`：连不上时不再直接抛，改成最多两轮——第一轮失败就把浏览器拉起来再连一次。
- 新增 `ChatLane._relaunch(reason)`：`time.sleep(0.8)` 等旧进程让出调试端口 →
  `rt.run_in_worker(rt.ensure_browser, timeout=150)` 让**采集线程**重新启动 Chrome →
  记一条「浏览器已重新打开，消息车道正在重连」→ `time.sleep(1.2)` 等端口监听 → 再连。
  失败时抛「Chrome 窗口已关闭，自动重新打开失败：…」，前端能看到真正的原因。
- 浏览器没被关、只是消息车道自己断了的情况仍走原来的 `_drop_locked()` 重连逻辑，不受影响。

### 验证
- 真机（开发态后端 + 真实 Chrome，2026-11-19 19:0x）：
  1. 正常打开浏览器 → `消息车道已连上浏览器（端口 1273）`；
  2. 强杀 Chrome 进程模拟「关掉窗口」→ 下一次消息请求报
     「浏览器窗口已关闭或崩溃，下次操作会自动重新打开」；
  3. 再下一次请求触发重开，日志依次出现
     「浏览器不可用（窗口已关闭或浏览器已退出），正在自动重新打开」
     →「启动 patchright（持久化目录 …）」
     →「浏览器已重新打开，消息车道正在重连」
     →「消息车道已连上浏览器（端口 1273）」，请求恢复正常（耗时约 30 秒）。
- 顺带修掉两个让「自动重开」失效的坑：
  - `ensure_browser()` 原来只靠 `ctx.pages` 探活，Chrome 被关掉后它**仍返回旧列表**，
    于是把已经死掉的上下文当活的用。现在再加一次 `ctx.browser.is_connected()` 判断。
  - `ChatLane._drop_locked()` 原来每次都 `pw.stop()`，同一线程里再
    `sync_playwright().start()` 会被 patchright 误判成「在 asyncio 循环里用同步 API」
    （日志原样报这句），消息车道从此全废。现在只丢浏览器/上下文、保留驱动，
    确实连不上时（第 3 次尝试）才重建驱动；重开浏览器走新加的 `Runtime.restart_browser()`。

---

## 2026-11-19（深夜 2）配置与筛选条件没回显：配置只在启动读一次 + 筛选条件只写不读

### 现象
用户：「配置信息没有回显」「也没有记住我的筛选条件」。重启程序后配置抽屉里的 Base URL / 模型 /
提示词 / 技能看着像没保存，筛选面板（关键词、城市、薪资…）也回到默认值。

### 根因（两个独立问题）
1. **配置读取失败被静默吞掉**：`web/src/App.tsx` 启动时 `.getConfig().catch(() => {})` 只请求一次。
   Tauri 先建窗口、sidecar 后端稍后就绪，第一次请求经常直接连接失败 → 配置永远停在默认值，
   而数据库里其实是有值的（`/api/config` 本身返回是完整的）。没有任何重试，界面就一直「空着」。
2. **筛选条件只写不读**：`POST /api/start` 里 `store.set_config_json("last_filter", req.model_dump())`
   存了，但全仓库没有任何地方读回来（`grep -rn last_filter` 只有这一行 + pyc），
   `/api/config` 也不返回它 → 前端无从还原。

### 改动
- `backend/app/main.py` `get_config()`：返回值增加 `"last_filter": rt.store.get_config_json("last_filter", None)`。
- `web/src/api.ts`：`Config` 增加 `last_filter?: Record<string, any> | null`。
- `web/src/App.tsx`：
  - 启动配置加载改成 `loadConfig()`，失败每 1.5 秒重试（最多 60 次），成功才把值灌进 state；
    卸载时清理定时器。
  - 新增 `applyLastFilter(lf)`：把 `last_filter` 里的 keywords / platforms / city / salary /
    experience / degree / scale / stage / industry / job_type / max_pages / exclude_keywords
    还原到筛选面板（数组字段先做类型过滤，空数组不覆盖）。
  - 配置加载成功后调 `applyLastFilter(c.last_filter)`；effect 依赖补上 `applyLastFilter`。

### 验证
- `cd web && npm run build` 通过（`dist/assets/index-a3c-Pge9.js` 294.46 kB）。
- 库里确实有这套条件：`last_filter` = platforms=boss、keywords=[全栈工程师, Node.js, …]、
  city=101280600、salary=[sal:15-30]、experience=[106,107]、scale=[303,302]、max_pages=3。

---

---

## 2026-11-19（深夜）采集与消息并行：消息走独立车道 · 打招呼提示进日志

### 需求
用户：「消息列表和抓取 jd 不能同时运行吗」「采集、主程序、自动打招呼、自己打招呼各自使用不同的线程，互不影响」，
以及「自动打招呼的提示、错误放日志里面，现在的位置会破坏布局」。

### 根因
所有碰浏览器的动作都挤在采集线程：`run_scrape` 一次调用抓完所有页，期间消息请求只能在队列里排队，
等满 90~120 秒后报错。旧代码还会误报「Boss 消息页没有响应」——看门狗读的是 `BossChat` 的全局
`_last_act`，上次成功开过消息页就会留下一个非 0 的旧时间戳。

### 方案（同一台 Chrome，两个 Playwright 客户端）
Playwright 允许**多个客户端连同一台浏览器**（`connect_over_cdp`），于是：
- 采集线程：照旧启动浏览器（`launch_persistent_context`）并跑抓取；
- 新增 `ChatLane`（`backend/app/main.py`，线程名 `aja-chat`）：自己的 `sync_playwright().start()` +
  `connect_over_cdp("http://127.0.0.1:{cdp_port}")`，连的是同一个持久化上下文，只驱动自己的标签页。
- 端口：`Runtime.cdp_port = _free_port()`，`launch_context(..., cdp_port=...)`
  （`backend/app/browser.py`）把 `--remote-debugging-port=N` 加进 Chrome 启动参数。
- 不新开浏览器 → 指纹、登录态、cookie 都不用搬，Boss 也看不出多了一个浏览器。

### 改动
- `_run_chat()`：任务投递到 `rt.chat_lane`；看门狗读 `chat_lane.worker_busy()/worker_task()`；
  只有 `submitted` 之后刷新的 `activity()` 才算本任务自己的进展；放弃时置 `state["abort"]`，
  排到队的 `_wrap` 直接跳过，避免前端 3 秒轮询在事后堆出一串重复浏览器操作。
- 消息类接口里的 `rt.ensure_browser()` 全改成 `rt.chat_lane.ensure_ctx()`（登录、登录态检查仍走采集线程）。
- `ChatLane.ensure_ctx()` 连接失败不会去杀采集用的浏览器；`reset()` 只 `pw.stop()` 断开自己的 CDP。
- 拉会话列表/聊天记录失败时用 `BossChat.cached_friends()/cached_messages()` 兜底，返回 `stale: true`。
- 前端 `web/src/App.tsx`：自动打招呼的提示/错误改走 `pushLog()` 写进日志面板（前缀 `[打招呼]`，
  warn/error 会染色）；顶栏那行状态文字删掉（它 `shrink-0` + 长句会把按钮挤变形），已发/失败计数挪进按钮 `title`。

### 验证
- 临时脚本：采集线程被 60 秒假任务占住时，消息车道仍能在 **0.0 秒**内完成 `goto + title`（并行成立）。
- 开发态起服务：`/api/health` 正常；`/api/chat/friends` 在未登录的 profile 下返回可读错误而不是卡死。
- **打包版真机并行验证**（2026-11-19 18:15，抓取「后端开发/Java」正在进行中）：
  `GET /api/chat/friends` 在 **2.5 秒**内返回真实会话列表，同时 `/api/status` 仍 `running:true`，
  采集日志照常推进；`backend.log` 记 `消息车道已连上浏览器（端口 5447），采集和消息互不阻塞`。
  同一时刻两个车道各自操作不同标签页，抓取节奏没被打乱。
- `npm run build` 通过。

### 注意
- 打包后 patchright 的 driver 仍要可用（采集线程本来就在用 `sync_playwright().start()`），消息车道只是多起一个客户端。
- 浏览器重启后端口不变（`cdp_port` 进程内固定），`ensure_ctx()` 会自动重连。

---

## 2026-11-19（晚）「生成回复」报 `大模型返回了空内容`（推理模型的思考吃光输出预算）

### 现象
点「生成回复」后提示：`大模型生成失败（ValueError: 大模型返回了空内容），已用规则话术代替`。

### 根因
用户配的模型是 **`deepseek-flash`，属于推理型模型**：它先写 `reasoning_content`（思维链），
再写 `content`，而**思考 token 也算在 `max_tokens` 里**。

`greeting.py` 里打招呼语只给了 `max_tokens=400`。实测同一份 prompt：

| max_tokens | finish_reason | reasoning_tokens | content 长度 |
|---|---|---|---|
| 400 | `length` | 400 | **0** ← 报错现场 |
| 800 | `stop` | 694 | 191 |
| 1500 | `stop` | 542 | 261 |

思考把 400 全吃光 → `choices[0].message.content` 为空 → 抛 `ValueError("大模型返回了空内容")` → 回退规则话术。

### 本轮改动
- `backend/app/greeting.py`（`GreetingWriter.write`）：输出预算 400 → **1200**，
  被截断（`finish_reason=length` 且正文为空）时自动再试一次 **3000**；
  空了也不再只说「返回了空内容」，而是带上 finish 与思考 token 数（新增 `empty_content_reason()`）：
  「输出预算被思考过程耗尽，其中思考约 N tokens；可在设置里换非推理模型，或调大最大输出」。
- `backend/app/scorer.py`（`LLMScorer.score`）：同理会话预算 800 → **1600**，被截断再试 3200；
  解析失败信息里把 `finish` 和空内容显式打出来。

### 验证
用库里真实的 `api_key / base_url / model`（`%LOCALAPPDATA%\ai-job-assistant\data\job_assistant_pro.db`
的 `app_config` 表）直接调接口：
- 修复前 `GreetingWriter.write()` → `source=rule`，note 正是那句报错（复现成功）；
- 修复后连续两次 → `source=llm`，tokens 1027 / 1508，正文 200+ 字，`note=''`；
- `LLMScorer.score()` → `score=8, source=llm, tokens=604`，JSON 解析正常。

### 经验
推理型模型（`deepseek-flash` / `deepseek-reasoner` / 各家 thinking 模型）不能按「答案长度」给
`max_tokens`，要按「思考 + 答案」给；判断依据看 `usage.completion_tokens_details.reasoning_tokens`。

---

## 2026-11-19（更晚）Boss 登录报 `Cannot switch to a different thread`（greenlet 回归）

### 现象
点「登录 Boss」后前端报：

```
Cannot switch to a different thread
Current:  <greenlet.greenlet object ... current active started main>
Expected: <greenlet.greenlet object ... suspended active started main>
```

### 根因
上一轮 commit `7ec4585`（「采集线程改常驻 greenlet」）把 `main.py` 里登录会话的注入改成了直连：

```python
get_context=rt.ensure_browser,
start_browser=rt.ensure_browser,
step_runner=lambda fn, *a: fn(*a),   # 等于没投递
```

当时的注释断言「登录会话本身就跑在采集线程里（`rt.submit(sess.start)`）」，**这个前提是错的**：
`LoginSession.start()`（`backend/app/browser.py`）只是
`threading.Thread(target=self._run, daemon=True).start()`，跑在独立的登录调度线程上。
patchright 的 sync API 绑定在 `sync_playwright().start()` 所在线程（采集线程）的 greenlet 上，
在登录线程里直接调用就会被 greenlet 拒绝跨线程 switch。

另有一处漏网：`_steps()` 开头的 `ctx = self._open_browser()` 是**绕过 step_runner 的直连调用**，
即使 step_runner 修好，「拿上下文」这一步仍会在登录线程里启动浏览器。

### 本轮改动
- `backend/app/main.py`（`login_start`）：改回
  `get_context=None, start_browser=rt.ensure_browser, step_runner=rt.run_in_worker`，
  注释改写为「`sess.start()` 只是再起一个登录调度线程，每一步碰浏览器的动作都必须显式投递到采集线程」。
- `backend/app/browser.py`（`_steps()` 开头）：`ctx = self._open_browser()` → `ctx = yield (self._open_browser,)`。
- `backend/app/main.py`（`Runtime.run_in_worker`）：补 `_wrap.__wrapped_label__`，
  否则采集线程日志只会刷「开始执行：_wrap」，看不到具体步骤名。

> 注意：`start_browser` 要传**裸的** `rt.ensure_browser`，不能传 `rt.ensure_browser_on_worker` --
> 后者内部自己就投递一次，再被 step_runner 包一层会在采集线程里二次投递，抛
> 「内部错误：run_in_worker 不能在采集线程里再投递任务」。

### 验证
源码后端（`AJA_PORT=8791 python -m app.main`）：`POST /api/login/start {"platform":"boss"}` →
12 秒后 `GET /api/login/status` 返回 `{"state":"logged_in","message":"登录成功"}`，
`data/logs/backend.log` 时间线正常（开始 → 启动 patchright → 登录窗口已打开 → 登录成功），无 greenlet 报错。

### 待办
- 已确认除登录外的其他调用点（`run_scrape`、各 `_do/_find/_new/_run/_start`）本来就走
  `rt.submit(...)` / `_run_chat(...)` 投递到采集线程，无需改动。
- 可考虑在 login 的 step_runner 处加断言/注释防回归。

---

## 2026-11-19（晚）「把 exe 放到别的文件夹打开会闪退」排查与修复

### 现象
把 `src-tauri\target\release\ai-job-assistant.exe` 复制到别的文件夹双击，窗口一闪就没了；
放在原地双击也一样。`crash.log` 里只有「主进程启动」，没有任何退出原因。

### 根因（已复现，证据在 crash.log）
用临时 build 打出 panic 详情，实际是 **WebView2 创建失败**：

```
Failed to setup app: runtime error: failed to create webview:
WebView2 error: WindowsError(Error { code: HRESULT(0x800700AA), message: "请求的资源在使用中。" })
```

→ 上一轮 `ai-job-assistant.exe` 被异常结束（panic abort / 任务管理器结束）后，
它拉起的 `msedgewebview2.exe` 变成孤儿，继续占着 WebView2 的用户数据目录，
新进程建 webview 时直接 `E_ACCESSDENIED`，panic 后整个进程消失。
**和 exe 放在哪个文件夹无关**——换个目录只是让人误以为是路径问题。

排查中另外发现三个放大器：
1. panic hook 里又调了 `default_hook` → 二次 panic 输出会 `process::abort()`（退出码 -1073740791），
   日志根本来不及落盘；
2. 单实例锁用的是 `app_local_data_dir()`（Tauri 下其实是 `%APPDATA%\com.ai.jobassistant`），
   不是 `main.rs` 释放后端用的 `%LOCALAPPDATA%\ai-job-assistant`，两边目录对不上，锁形同虚设；
3. 锁抢不到时 `exit(0)`，和「正常退出」完全无法区分。

### 本轮改动（`src-tauri/src/lib.rs`、`commands.rs`）
- **启动前置检查**（`run()` 里，Builder 建 WebView **之前**）：
  `prepare_webview_dir()` 先抢单实例锁（固定路径 `%LOCALAPPDATA%\ai-job-assistant\app.lock`），
  抢不到 → `exit(2)`；抢到后 `taskkill /F /IM msedgewebview2.exe` 清残留、
  预建 `EBWebView` 目录，日志记下 WebView2 用户数据目录。
- **按路径杀残留**：`kill_pids_under(%LOCALAPPDATA%\ai-job-assistant)`，只结束从释放目录起的实例，
  不影响从 `target/` 打开的调试实例。
- **panic hook 不再转发 `default_hook`**：panic 变成普通 unwind，稳定拿到退出码，不再 abort。
- **WebView2 失败时写人话**：`crash.log` 追加 `[fatal]` + 三步处理指引
  （结束 msedgewebview2 → 删 `EBWebView` → 反馈日志）。
- 新命令 `get_crash_log_path`（Rust + `generate_handler!`），给界面一个日志入口。

### 验证
- `cargo tauri build` 通过（`cargo build` 在受限沙箱里被 `target/.cargo-build-lock`
  挡住过一次，切到完全权限后按 `build.bat` 的步骤正常构建成功）。
  产物：`src-tauri\target\release\ai-job-assistant.exe`（70 996 992 字节）、
  `bundle\nsis\AI Job Assistant_0.1.0_x64-setup.exe`（64.10 MiB）。
- 桌面实测（已复制到 `C:\Users\Administrator\Desktop\ai-job-assistant.exe`，md5 与构建产物一致）：
  1. **从桌面启动正常**：`crash.log` 依次出现「已清理残留的 msedgewebview2.exe」
     →「WebView2 用户数据目录: …\EBWebView」→「后端已就绪，端口 2628」，窗口正常，进程常驻；
  2. **多开被挡**：第二个实例 `exit=2`，日志写明「另一个实例正在运行（锁 …app.lock 被占用）」；
  3. **把 exe 复制到另一个文件夹（`data\.smoketest\`）启动正常**，端口 13356；
  4. **强杀后再启动能自愈**：任务管理器结束进程后重开，
     日志出现「已结束残留进程 pid=19836,20180」+「已清理残留的 msedgewebview2.exe」，随后正常起后端。
- 也就是当初「换文件夹就闪退」的场景已经走通，不再复现。

### 待办
- 若 `[fatal]` 指向 `EBWebView` 目录权限，考虑给 WebView2 换一个可控的 user data 目录。
- 前端「崩溃日志」入口还没做界面按钮，只加好了 Rust 命令。

## 2026-11-19（傍晚）Boss 主动发起会话 · 采集线程与事件循环大修 · 自动打招呼实跑

### 本轮做了什么
- **主动发起会话**：`BossChat.start_conversation()` 走「岗位详情页 → `a.btn-startchat`（立即沟通）→ 跳聊天页 → 读选中的 HR → 刷新列表 → 再点开」。返回 `{ok, uid, friend, created}`，前端「打招呼」按钮（`openJobChat(j, true)`）和 `POST /api/auto-greet/one` 都走这条路，用户不需要先在 Boss 上手点。
- **uid 来源**：会话行 DOM 上**没有任何 uid 属性**，`data-v-*` 只是 Vue 作用域标记。改成从 Vue 实例链取：`__vueParentComponent` / `__vue__.$parent` 的 `props.friend/item/data/info` 里读 `{name, uid}`（`_selected_uid()`）。
- **自动打招呼实跑通过**：真实账号上连发两条（明岩 8.3s、邝女士 9.0s/1095 token），随后自动建立下一个岗位的会话。顶栏开关 → 逐条取待投递 → 生成话术 → 发送 → 标记已投递 → 20s±5s 再来。
- **token 三项**：右上角「累计 / 模型打分 / 打招呼」；模型配置小窗可编辑提示词与技能。

### 踩过的坑（全部是真 bug，按严重程度排）
1. **`page.on("response")` 回调里调 `resp.text()` 会把 driver 连接搞断**。报 `Connection closed while reading from the driver`，之后所有操作变成 `Event loop is closed` / `Playwright Sync API inside the asyncio loop`，「浏览器卡住」误报不断。改法：监听器只把 resp 塞进队列（`self._signals`），正文在主流程里 `_drain()` 读。
2. **每个任务起一个新 greenlet 会让已启动的浏览器失效**。改成一个常驻 greenlet 循环取任务；`run_in_worker` 加自检，禁止在采集线程里再投递。
3. **`_poll` / `_wait_items` 不打活动时间戳**，30 秒活动上限把「正常但慢」的流程判死 → 触发浏览器重建 → driver 断开。现在每次轮询都 `_touch()`。
4. **`_chat_page` 缓存页停在岗位详情页**：`start_conversation` 点完「立即沟通」后页面已不是 `/web/geek/chat`，但仍被当缓存页复用 → 监听器挂不上 → 27 秒超时。现在发现路径不符就重新导航。
5. **`friends(force=True)` 的 reload 会把刚选中的会话清掉**，导致「会话建好了但界面空白」。现在先读 uid 再刷新列表再点开。
6. **单实例锁的 File 是局部变量**，`setup` 返回就 drop，锁实际没生效。改成放进 managed state。
7. **`_run_chat` 的固定总超时**会把长流程误判成「可能需要重新登录」。改成默认 120 秒，只要还在推进就顺延一次。
8. **进程被外部关掉浏览器后，后端会一直报「事件循环已关闭」**。新增 `Runtime.reset_browser()`，遇到 `Event loop is closed` / `Target closed` / `Playwright already stopped` 就丢掉上下文，下次操作自动重开。

### 待办
- 打包版启动约 20 秒后主进程会自行退出，暂未查明（已确认不是多开锁：`crash.log` 里没有「禁止多开」记录）。下一步在 `run()` 的 `run(...)` 返回值里加退出原因记录。
- 自动打招呼的「每日投递上限」目前沿用抓取上限，是否单独配置待定。

## 2026-11-19（下午·七次）「打招呼」= 主动发起会话

用户反馈：「我点击打招呼按钮，说明我正在主动发起请求，这时候应该新建一个与该 hr 的会话」，
以及「自动打招呼也是如此，根据 jd，主动发起会话」「跳转到聊天页面并新建会话」。

- 以前的行为：只去 Boss 会话列表里找，找不到就提示「右侧已列出全部 HR，挑一位」。
  但 Boss 只有在**打过招呼之后**才会为该岗位建会话，所以没聊过的岗位永远找不到。
- 实测可用入口：岗位详情页 `https://www.zhipin.com/job_detail/{encryptJobId}.html` 上的
  `a.btn-startchat`（文案「立即沟通」）。点它会跳进 `/web/geek/chat` 并自动选中该 HR，
  会话列表里立刻出现这条会话——**只建会话，不需要真的发消息**。
- `backend/scrapers/boss_chat.py` 新增 `start_conversation(ctx, friend, encrypt_job_id)`：
  1. 会话列表里已经有 → 直接 `_open_conversation`，返回 `created=False`；
  2. 否则 goto 岗位详情页 → 等 `a.btn-startchat` 可见 → 点击 → 等进入 `/web/geek/chat`；
  3. **先读下 `.chat-conversation .name-text`**（刷新列表会把它清掉），再 `friends(force=True)`
     把这位 HR 的真实条目找回来；
  4. 按 uid → 按名字 → 按 DOM 里刚建出来的行 依次兜底，最后重新 `_open_conversation`，
     保证右侧聊天框真的打开了。
  踩过的坑：`self._friends` 是 **list** 不是 dict（写成 `.values()` 直接 AttributeError）；
  `friends(force=True)` 会 reload 聊天页，把「立即沟通」刚选中的会话清空。
- `backend/app/main.py`：
  - `ChatOpenReq` 增加 `force_new: bool = False`；`/api/chat/open` 在 `force_new` 且带
    `encrypt_job_id` 时走 `start_conversation`，返回多一个 `created` 字段。
  - `POST /api/auto-greet/one`：找不到会话时不再挑「同公司唯一 HR」凑数，而是直接
    `start_conversation` 主动建会话，然后照样「生成话术 → 发送」，顺序是
    **先开会话 → 生成话术 → 发送**。
- `web/src/api.ts`：`chatOpen` 参数加 `force_new`，返回类型加 `created`。
- `web/src/App.tsx`：`openJobChat(j, forceNew = false)`；卡片展开仍只用已有会话（不打扰 Boss），
  只有「打招呼」按钮调 `openJobChat(j, true)`；建会话成功后提示
  「已向 XX 的 HR 发起沟通，右侧可以直接发消息」。顺手修掉把 `j.hr_name`（HR 名字）
  当 `uid` 传的 bug。
- 验证（dev 副本 profile，不影响用户真实数据）：`start_conversation` 返回
  `created=True uid=40761977 name=刘女士`，页面 URL 为 `/web/geek/chat`，
  `.chat-conversation .name-text` = 「刘女士」，会话区有消息卡片，输入区可用。

---
## 2026-11-19（下午·六次）大模型打分截断 + Boss 薪资反爬解码

### 1. 「第 1 页解析出 9 条却没入库、敏感库也没有」——数据其实都入库了
查库：`data/job_assistant_pro.db` 的 `job_queue` 20 条、`sensitive_jobs` 42 条，
最近几条（珠海脉冲计算、瓦石能源、深圳鼎驰）都在。用户当时看不到，是因为同一轮里
大模型打分抛异常退回了规则分，界面刷新时机也错过去了。**顺带发现了下面这个真 bug。**

### 2. 大模型打分一直静默失败（真 bug，影响所有大模型模式下的打分）
- 现象：库里 3 条 `score_reason` 是 `[大模型失败:ValueError] …`。
- 根因（两个叠在一起）：
  1. `LLMScorer.score` 里 `max_tokens=300` 太小。`deepseek-flash` 的 `reason` 是中文长句，
     300 token 只够 150 来个汉字，输出被截断，`finish_reason="length"`。
  2. `parse_llm_score` 只认**完整闭合**的 JSON，一截断就返回 `None` → 抛 ValueError → 回退规则分。
- 改法：
  - `max_tokens=300 → 800`。
  - 新增 `_score_from_obj(obj)` 与 `_score_from_partial(text)`：完整 JSON 走前者；
    被截断时用正则 `"score"\s*[:：]\s*(\d{1,2})` 从残缺文本里抠分，`reason` 能抠多少算多少。
  - 截断时在 reason 末尾标注「（输出被截断，理由可能不完整）」，不再谎报「无法解析」。
- 验证：同一个库里 3 条任务实跑大模型后返回真实分数 6 / 5 / 4（tokens 1284 / 1621 / 1319）。

### 3. Boss 薪资反爬字符解码（薪资二次校验以前完全失效）
- 实测：列表卡片 `span.job-salary` 的 innerText 是私用区字符，码点 `U+E032→"1"`、`U+E036→"5"`，
  即**码点 - 0xE031 = 数字**。例：`\ue032\ue037-\ue032\ue038K` = `16-17K`。
- `backend/app/scorer.py` 新增 `decode_boss_salary(text)`（`_PUA_BASE = 0xE031`，其它私用区字符丢弃），
  `RuleScorer.parse_salary` 入口先解码。以前 `SALARY_RE` 永远匹配不到加密文本 → 返回 None →
  `salary_out_of_range` 一律放行，薪资区间等于没生效；现在真正会拦。
- `RuleScorer._salary` 修 9999 上限溢出：先把 `low/high` 夹到画像上限（`PROFILE["salary_high"]`）
  再算 `hi - lo`，避免「15-9999K」被当成满分区间的正中拿满分。
- `backend/scrapers/boss.py`：`_parse_dom_cards` 的 `salary` 过 `decode_boss_salary`；
  `_parse_detail_dom` 的薪资选择器收窄为 `.job-banner .info-primary .salary` /
  `.job-banner .salary` / `.info-primary .salary`，兜底才用 `.job-banner .salary`——
  注意裸 `.salary` 会命中页面里全部推荐岗位（实测 37 个），绝不能直接用。
- 验证：真实搜索页 15 张卡片全部解出正确薪资（`25-35K`、`8-9K`、`18-28K`、`15-18K·13薪`），空值 0；
  `8-9K` / `6-10K` 在 `15-35` 区间下被正确排除，`16-17K` 在 `20-30` 下被正确排除。

### 待办
1. 自动打招呼真实环境验证（会真发消息给 HR）。
2. 已有 3 条 `[大模型失败:ValueError]` 的旧记录不会自动重算，需要的话点「重新打分」。

---
## 2026-11-19（下午·五次）打分开关进顶栏 + token 三项统计

用户需求：
- 「大模型打分和规则打分 放在外边，铭感库的左边，不是在铭感库的弹窗页面出现」
- 「累计 token 分模型打分和打招呼的消耗，共计 3 个显示」

- `web/src/App.tsx`
  - 顶部工具栏原来那个只读小标签换成可点的分段开关「大模型打分 | 规则打分」（位置：招聘网站之后、「敏感库」之前），切到「大模型打分」时右侧出现「初筛≥ [5] 分」输入框；敏感库弹窗里那份开关与初筛输入已删除。
  - 右上角 token 改成三项：`累计 N token | 模型打分 N | 打招呼 N`；配置小窗里也同步显示三项。
- `backend/app/main.py`
  - `Runtime` 新增 `self.tokens_score`（读 `app_config.tokens_score`）；老数据没有该键时按 `tokens_total - tokens_greet` 回填。
  - `add_tokens(n, kind)` 现在 `score` 与 `greet` 分别累计并各自持久化；`/api/config`、`/api/status`、`/api/auto-greet/next`、`/api/greet` 的返回体都带上 `tokens_score`。
- `web/src/api.ts`：`Config` / `GreetResp` / `AutoGreetJob` 补 `tokens_score`。

回答用户提问「现在可以边打招呼边抓取数据吗」：可以，但**不能真并行**。抓取与消息操作共用同一个采集线程（patchright 页面必须留在该线程），只能排队；`_run_chat` 的看门狗会识别「浏览器正被「抓取…」占用」，只在超过 90 秒时提示，不会误报浏览器卡死。

---
## 2026-11-19（下午·四次）大模型打分前的规则初筛

用户需求：「大模型打分前先用规则做一次初步筛选，然后调用大模型结合 jd 详情和自己的技能打分」「规则给分和大模型给分只生效一个」。

- `backend/app/scorer.py`
  - 新增 `Scorer.llm_prefilter`（默认 5 分，0=不设初筛）。
  - `Scorer.score()` 流程：先跑 `_rule_score()`；
    - 规则模式 → 直接返回规则分；
    - 大模型模式 → 规则分 < `llm_prefilter` 时直接判掉并注明「[规则初筛 N 分 < 5，未调用大模型]」，**不花 token**；过了初筛才调 `llm.score(job, profile, filters)`，并且**最终只采用大模型的分**（两种给分不叠加、不混用）；大模型异常时才退回规则分。
- `backend/app/main.py`
  - `_build_scorer()` 从 `app_config.llm_prefilter` 读初筛线；`/api/config` 暴露并可保存 `llm_prefilter`（0~10 夹取）。
- `web/src/App.tsx` / `api.ts`
  - 敏感库浮层的打分开关旁，切到「大模型打分」时出现「初筛≥ [5] 分」输入框，失焦即保存。

已验证（fake LLM 单测）：好岗位 → `9 llm`（调用 1 次）；差岗位 → `4 rule` 且不调用大模型；规则模式 → `8 rule` 且不调用。

---
## 2026-11-19（下午·三次）打招呼兜底 + 打分方式开关

用户需求：
- 「打招呼时提示：还没有和这个hr聊过，修复它，点击打招呼生成直接与hr沟通对话框」
- 「敏感库列表左边加上一个 大模型打分|规则打分 的开关 / 使用大模型打分时，获取jd后直接使用简历+配置技能+过滤条件和jd匹配打分」「默认使用规则打分」

- `backend/app/main.py`
  - `/api/chat/open` 不再返回「还没聊过」：先按 `encrypt_job_id` 找会话，找不到再按公司名筛 HR；只有 1 位就直接返回该会话（带 `fallback: "brand"`），多位则返回 `candidates` 列表让界面挑。
  - `/api/auto-greet/one` 同样加了「同公司唯一 HR」兜底，否则 `skip` 掉并说明原因。
  - `Runtime._build_scorer()` 读 `score_mode`（`rule`/`llm`，默认 `rule`），把 `use_llm`、`profile`（含「配置→技能」）、`filters`（薪资区间 + 分值门槛 + 排除关键词/公司拼成一句话）都灌进 `Scorer`。
  - 新增 `Runtime._filter_text()`；`/api/config` 增加并保存 `score_mode`，返回体带 `score_mode`。
- `backend/app/scorer.py`
  - `build_user_prompt(job, profile=None, filters="")`：技能列表复用 `greeting._profile_skills()`（**注意**：直接对字符串做遍历会把「Java」拆成 J、a、v，必须走这个函数按分隔符切），末尾追加「【我的筛选条件（投递偏好）】…」。
  - `LLMScorer.score(job, profile, filters)`；`Scorer.__init__(..., use_llm=False)`，只有开关打开且配了 key 才走大模型，否则规则打分。
- `web/src/App.tsx` / `api.ts`
  - 敏感库浮层标题左边加「大模型打分 | 规则打分」分段开关，默认规则；切换即 `POST /api/config {score_mode}` 并重建打分器，图片/日志立刻可见。
  - `openJobChat` 找不到会话时改为提示「这家公司有 N 位 HR 可选，点右侧任意一位即可开始沟通」，不再当成错误弹红字。

待办：
- 「自动打招呼」链路已实现（`/api/auto-greet/next`、`/api/auto-greet/one` + 前端 20s 间隔驱动），但还没在真实环境跑通（会真的发消息给 HR，需要人工确认后再测）。
- 用户报告的「第 1 页接口未捕获，DOM 解析 9 条却没入库」：DOM 卡片确实进了 `jobs` 并交给 `on_job`，怀疑卡在 `process_job` —— 详情页没拿到 JD 导致规则分被压低、低于 `min_score` 被排除。待从日志取证。
- 薪资二次校验仍欠修：Boss 卡片薪资是私用区加密字符（`\ue031`=0 … `\ue03a`=9），`parse_salary` 匹配不到；`tighten_ranges` 会把多档并成连续区间；上限 9999 时 `hi-lo` 溢出给满分。

---
## 2026-11-19（下午·二次）消息气泡与列表视图修正

用户反馈：「选中的聊天高亮 boss，聊天内容没有左右区分，消息没有加背景色，消息类型也不对 / 刷新两个字变成了两行，适当加宽boss列表」。

- `web/src/ChatPanel.tsx`
  - 会话行加 `border-l-[3px]`，选中 `border-primary bg-primary/10 ring-1 ring-primary/30 font-semibold`，头像放大到 38。
  - 气泡改微信式配色：我方 `bg-[#95ec69]` 右上小圆角、对方白底描边左上小圆角，`max-w-[80%]`；外层按 `from_me` 对齐到两侧；回执挂在自己气泡右下方。
  - 系统行（附件简历 / 职位卡片 / 类型兜底）改为居中的浅灰小胶囊，不再和对话混在一起。
  - 顶部「刷新」「收起对话框」按钮加 `shrink-0 whitespace-nowrap`，不再挤成两行。
  - 列表与对话框比例从 1:3 调成 1.2:2；`web/src/App.tsx` 右栏展开宽度 52% → 60%。
- `backend/scrapers/boss_chat.py` 的 `normalize_message`
  - `btype` 安全转 int（原来是字符串，`btype == 8` 永远为假，才会出现「[消息类型1]」）。
  - 去掉 `[消息类型N]` 这种兜底，改成「图片 / 附件简历 / 职位卡片 / 语音 / 其他消息」。
  - 新增「正文是否真的存在」判定：只靠类型兜底生成的文字一律当系统行，避免把系统消息画成对方说的话。
  - 16~21 类（纯系统/时间线）不再画气泡。
- 打包版运行时会独占浏览器 profile，dev 后端无法同时用 Chrome。用 mock 接口（`C:\Users\Administrator\AppData\Local\Temp\mock_shot.py`，路由注入 window.fetch 桩）在浏览器里验证渲染，再打包。

## 2026-11-19（下午·三次）「有的会话能分清我和 Boss，有的不能」

根因：`normalize_message` 判断「是不是我发的」只看 `raw.received`。这个字段**并非每条都有**，
缺的时候就退化成 `body.fromId == my_id`，而 `my_id` 要靠猜（拿不到就一直是 None），于是这些会话
里全部消息都被当成对方发的，左右自然就乱了。

- `from_me` 现在允许三态：`True` 我发的 / `False` 对方 / `None` 不确定。
  判定顺序：`received` → `body.fromId/from` 与 `my_id` 比对 → `None`。
- `_merge_status()` 除了合并回执，现在也用 DOM 校正左右：DOM 的 `li.message-item`
  一定有 `item-myself` / `item-friend`，按 `mid` 对齐后以 DOM 为准；实在对不上的再按顺序兜底。
- 正文处理：先 `html.unescape` 再 `_TAG_RE` 去掉 `<copy>…</copy>` 这类标签，
  `_NAV_TEXT_RE` 去掉「拒绝/同意/复制微信号/查看详细分析」这类纯按钮文字；
  正文为空的消息直接丢弃（原来会画出空胶囊）。
- 前端 `ChatPanel.tsx` 增加 `SYS_KINDS = {2,3,5,6,7,8}`：「附件简历 / 图片 / 职位卡片 / 语音」
  一律画居中的浅灰胶囊，不管接口怎么标。
- 验证（用 profile 副本跑 dev 后端，避免占用真实浏览器 profile）：
  周俞 8 条/我 1 条、卞女士 5/2、朱鸿凡 4/1、王女士 4/2、黎女士 14/3、江伟阁 12/3，空文本 0 条。
- 环境提示：打包版运行时独占 `data/browser-profile`，dev 后端必须改用副本目录
  （`AJA_DATA_DIR=...\Temp\devdata`，里面放一份 `browser-profile`）才能和打包版并存。

## 2026-11-19（下午）Boss 消息面板 · 简历发送 · 回执

### 本轮做了什么

**Boss 消息面板（`web/src/ChatPanel.tsx`）**

- 布局：右栏常驻「Boss 消息」列表；展开时列表 `flex-[1]` + 对话框 `flex-[3]`；收起时列表独占右栏、排 2 列；左侧投递列表在收起时可排 3 列。
- 配色统一：列表只保留灰色的选中/悬停态，不再给每个 HR 配不同颜色（`hueOf()` 固定 `#64748b`）。
- 气泡：Boss 消息靠左、我的消息靠右；颜色只用于回执。
- 回执：自己发的气泡下面显示 `已读`（绿）/ `已送达`（灰）/ `未读`（橙）。
- 输入区上方一行：`生成回复`、`发送简历`/`收起简历`。
- 点会话自动展开对话框；未投递岗位点「打招呼」直接跳到对应 HR。

**AI 生成招呼语（`backend/app/greeting.py`）**

- 提示词可在「模型配置」小窗里改（`greet_prompt`），默认值 `DEFAULT_PROMPT`。
- 生成依据：提示词 + 岗位 JD（截前 1200 字）+ 我的技能（`matched_skills()` 取 JD 与画像技能的交集）。
- 没有配模型时用 `rule_greeting()` 兜底，并在界面注明「未配置大模型，使用规则话术」。
- 消耗的 token 累加，显示在「模型配置」左边（`累计 N token（其中生成 M）`）。

**发送简历**

- `GET /api/chat/resumes` 读 Boss 附件简历列表（拦截 `wapi/zpgeek/resume/attachment/checkbox.json`）。
- `POST /api/chat/resume` 给指定 HR 发附件简历。
- 面板默认不请求，点「读取 Boss 上的附件简历并发送」才去读；点「发送」时若还没读过会先自动读取。

### 踩过的坑

1. **historyMsg 里没有已读标识**。`flag` 恒为 0，`uncount` 是未读数，都不代表已读。回执只能从 DOM 的 `.message-status.status-read / status-delivery` 读，再按 `mid` 合并回接口数据（`_merge_status`）。
2. **消息 body.type 是模板 id 不是消息类型**，导致聊天记录显示「[消息类型7]」。必须用 `raw.type`。
3. **Boss 文本里有 HTML 实体**（`&lt;copy&gt;`），要 `html.unescape`。
4. **会话串台**：`_on_history` 原来用 `zp["bossId"]` 做缓存键，但响应里常没有；改为 `_open_uid` 回退 + 点击后用 `.name-text` 校验。
5. **固定延时拖慢读取**：全改成 `_poll()` 轮询，读一次聊天记录从 1.4–7s 降到 0.2–0.4s。
6. **简历列表监听丢失**：只要先调过一次 `friends()`（会 reload 页面），之后再给 `page` 挂 `response` 监听就收不到 `resume/attachment/checkbox` 响应，界面上「发简历」也不弹确认框。目前改用「临时监听 + 用完即摘」仍未根治，**待查**。

### 待办

- [ ] 修「读简历列表」在 UI 里仍报「没有取到 Boss 的附件简历列表」（优先怀疑页面 reload 后监听失效）。
- [ ] 薪资校验修复：PUA 字体解密、`tighten_ranges()` 不再合并区间、`_salary()` 上限 9999 封顶、详情页 `.salary` 选择器。
- [ ] 重新打包（`npm run build` → PyInstaller → `cargo tauri build`）。

---

## 2026-11-19（上午）Boss 消息模块从零到可用

- 新建 `backend/scrapers/boss_chat.py`：会话列表、聊天记录、发送、撤回。
- `backend/app/main.py` 新增 `/api/chat/friends|messages|send|recall|open`。
- 前端新增 `web/src/ChatPanel.tsx`，右栏替换掉原来的「JD 智能对话」占位。

### 踩过的坑

1. **死锁**：`friends()` 用 `threading.Lock()` 且在锁内回调自身 → 改 `RLock`。
2. **输入框不是 textarea**：真实是 `div#chat-input[contenteditable]`，页面里的 `textarea.input` 是 0×0 隐藏镜像。
3. **会话项不能用 `has_text` 过滤**（会超时），改用 `_item_rows()` 取索引再 `nth(idx).click()`。
4. **Boss 求职端网页版没有撤回入口**：自己发的气泡 hover 无操作区、右键菜单为空，「更多」里只有置顶/备注/黑名单/删除/举报。撤回只能在 App 里做（约 10 秒内）。
5. **孤立代理字符导致整页白屏**：Boss 用私用区字体反爬，个别昵称带回 `\udcab` 这类字符，渲染前统一 `clean()` 过滤。

---

## 2026-11-18 打包与稳定性

- `build.bat` 五步：`npm build` → PyInstaller → 复制到 `backend-exe/` → 图标 → `cargo tauri build`。
- 「登录页一直闪烁、日志刷 `_wrap`」：`context_alive()` 用 `ctx.new_page()` 探活，每秒新建一个标签页。改为 `ctx.cookies()`。
- 「程序被直接关闭、无日志」：加了 `crash.log`、`backend.log`、panic hook、单实例锁。
- 「没有可用的登录 cookie」：`/api/expectations` 只读 `app_data_dir()/browser-profile`，而真实登录态在项目根的 `data/browser-profile`。改成多目录回退 + `app_config.data_dir` 记忆真正有 profile 的目录。
