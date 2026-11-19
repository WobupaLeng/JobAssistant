# 开发进度记录

> 本文件记录 JobAssistant 每一轮的改动、踩过的坑和待办。追加式维护，最新的在最上面。

---

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
