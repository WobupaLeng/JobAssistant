# ADR-002 Playwright 对象线程与 greenlet 收敛

- **状态**：Accepted（2026-11-20 依据存量代码与 `DEVELOPMENT.md` 已修复缺陷回填）

## 背景
历史缺陷三连：
1. `Cannot switch to a different thread`（登录/抓取跨线程复用 Playwright 对象）；
2. 每个任务新建 greenlet 导致已启动浏览器「失效」；
3. 消息面板阻塞采集线程，抓取跑几分钟时无法收发消息。

## 决策
1. `Runtime._worker_loop` 内**只跑一个常驻 greenlet**，所有 patchright 同步调用在该 greenlet 中执行。
2. 采集线程 `aja-worker` 是唯一允许启动/操作 patchright 的线程。
3. 消息能力拆出 `ChatLane`：独立线程 + 独立 Playwright 客户端，通过 CDP（`Runtime.cdp_port`）连接同一台 Chrome，与采集互不阻塞。
4. 跨线程共享只允许纯数据（`Store` 每线程独立 sqlite 连接、`queue`、`StopFlag`、日志缓冲）。

## 后果
- ✅ 采集中可同时聊天/打招呼，暂停与停止秒级生效。
- ⚠️ 两个 Playwright 客户端共用一个 Chrome profile，**同机多实例必然冲突**，因此单实例锁是硬约束（见 ADR-001）。
- ⚠️ 新增任何浏览器能力必须声明归属车道（采集 / 会话），严禁临时起第三个客户端。
