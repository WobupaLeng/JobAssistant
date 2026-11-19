# 开发进度记录

> 本文件记录 JobAssistant 每一轮的改动、踩过的坑和待办。追加式维护，最新的在最上面。

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
