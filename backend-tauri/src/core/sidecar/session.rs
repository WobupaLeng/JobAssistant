//! sidecar 会话：把「进程」与「入库判定」缝在一起。
//!
//! [`SidecarSession`] 持有一个已就绪的 [`SidecarHandle`] 和一个后台消费线程：
//! 线程逐行读事件，`job.scraped` 立刻走 [`crate::core::sidecar::ingest::prepare`]
//! 判定，达标就 `insert_job`，并把每一条结论转成 [`SessionUpdate`] 交给调用方
//! （应用层再推给前端）。
//!
//! **为什么判定放在消费线程里**：抓一页可能来 15 条岗位，每条都要 `insert_job`
//! （SQLite 写锁）。如果放到命令线程，用户在抓取中打开列表就会跟写入抢
//! 同一把锁；放到消费线程后，命令线程只做「非阻塞取进度」。
//!
//! 依赖注入用闭包而不是直接收 `SqliteStore`：本模块因此不依赖 `rusqlite`，
//! 单测可以用内存计数器替代真实数据库，验证「达标才写、重复不写」的判定链。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::core::domain::job::{Job, SensitiveEntry, SensitiveKind};
use crate::core::domain::profile::Profile;
use crate::core::sidecar::ingest::{
    decision_note, prescan, prepare, DecisionNote, IngestConfig, IngestVerdict, PreScan,
};
use crate::core::sidecar::process::{SidecarConfig, SidecarError, SidecarEvent, SidecarHandle};
use crate::core::sidecar::protocol::{Command, Event, ScrapedJob};

/// 用户点「停止」后写入进度的结束原因。与 `Exited` 分支的文案必须一致，
/// 否则界面会在「已停止」和「已按用户请求停止」之间跳一下。
pub const STOPPED_BY_USER: &str = "已按用户请求停止";

/// 抓取进度。由消费线程更新，命令线程只读。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrapeProgress {
    /// sidecar 报出的原始条数（含被筛掉的）
    pub scraped: u32,
    /// 真正写入数据库的条数
    pub inserted: u32,
    /// 重复 `job_id` 跳过的条数
    pub duplicates: u32,
    /// 低于门槛 / 缺字段被丢掉的条数
    pub below_threshold: u32,
    /// 被硬规则否决的条数
    pub rejected: u32,
    /// 登录状态：`unknown` | `waiting` | `logged_in` | `closed` | `error`
    pub login_state: String,
    /// 登录态所属平台（来自 `login.state` 事件的 `platform`，或 `login_status`
    /// 查询写回）。空串 = 尚未收到任何登录事件；lib.rs 的 login-state watcher
    /// 以「平台空且 unknown」判定「还没有可广播的登录态」。
    pub login_platform: String,
    /// 最近一条日志（给界面做实时回显）
    pub last_log: Option<String>,
    /// 最近一次入库的岗位标题，界面上「刚刚抓到」的提示用
    pub last_title: Option<String>,
    /// sidecar 是否仍在运行
    pub running: bool,
    /// 结束原因（自然结束 / 崩溃 / 用户停止）
    pub finished_reason: Option<String>,
}

impl ScrapeProgress {
    pub fn new() -> Self {
        Self {
            login_state: "unknown".to_string(),
            ..Default::default()
        }
    }
}

/// 消费线程向外抛的增量。
#[derive(Debug, Clone)]
pub enum SessionUpdate {
    /// 一条岗位已处理（无论入库与否），带一句人话说明
    Job {
        verdict: IngestVerdict,
        summary: String,
    },
    Log {
        level: String,
        msg: String,
    },
    Login {
        state: String,
        msg: String,
    },
    /// 抓取因安全验证（风控）停止。界面据此提示手动过验证后恢复。
    Risk {
        platform: String,
        reason: String,
        url: String,
    },
    /// 无法解析的行（含 Python traceback 混入 stdout 的情况）
    Malformed {
        line: String,
    },
    /// sidecar 进程退出
    Exited {
        code: Option<i32>,
    },
}

/// 判定 + 入库的注入点。返回 `true` 表示真的新写入了一行。
pub trait JobSink: Send + 'static {
    fn insert(&mut self, job: &Job) -> Result<bool, String>;
}

/// 敏感库查询的注入点。
///
/// sidecar 在**打开详情页之前**先用它筛一遍：命中就整条跳过，既省下 3~6 秒
/// 的详情页，也不会把用户已经处理过的岗位再推一遍。
///
/// 返回 `Some((原因, 是否已入库))`。默认实现全不命中，供不关心筛选的调用方使用。
pub trait SensitiveLookup: Send + 'static {
    fn hit(&mut self, platform: &str, company: Option<&str>, title: Option<&str>)
        -> Option<(String, bool)>;
}

/// 「什么都没有」的查询器：预筛退化成只跑不依赖 JD 的规则。
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSensitive;

impl SensitiveLookup for NoSensitive {
    fn hit(
        &mut self,
        _platform: &str,
        _company: Option<&str>,
        _title: Option<&str>,
    ) -> Option<(String, bool)> {
        None
    }
}

impl<F> SensitiveLookup for F
where
    F: FnMut(&str, Option<&str>, Option<&str>) -> Option<(String, bool)> + Send + 'static,
{
    fn hit(
        &mut self,
        platform: &str,
        company: Option<&str>,
        title: Option<&str>,
    ) -> Option<(String, bool)> {
        self(platform, company, title)
    }
}



impl<F> JobSink for F
where
    F: FnMut(&Job) -> Result<bool, String> + Send + 'static,
{
    fn insert(&mut self, job: &Job) -> Result<bool, String> {
        self(job)
    }
}

/// 入库端口：写岗位 + 记敏感库 + 记决策日志。
///
/// 拆成三个方法而不是一个，是因为「已入库」「被否决」在敏感库里是两种
/// `kind`，而 session 层只知道本次结论、不该知道表结构；决策日志又是第三张表。
///
/// **三个方法都没有默认实现，这是刻意的。** 决策日志曾经整条写入链缺失
/// （`jd_decisions` 里只有 `rescore` 一种 kind 有数据，界面一半是空的），
/// 修法若留一个「默认什么都不做」的方法，任何新的实现忘写都照样编译通过、
/// 测试照样绿灯，故障只是从「写错」变成「静默恒空」——同一个坑换个位置再踩一次。
/// 忘写必须编译失败，这是唯一可靠的护栏。
pub trait IngestPort: Send + 'static {
    /// 写一条达标岗位，返回 `false` 表示 `job_id` 已存在。
    fn put(&mut self, job: &Job) -> Result<bool, String>;
    /// 把一条被否决的岗位记进敏感库，下次卡片阶段直接跳过。
    fn remember(&mut self, entry: &SensitiveEntry<'_>);
    /// 记一条决策日志（`jd_decisions`），让界面能回答「这个岗位为什么被排除」。
    ///
    /// 真不想记也必须显式写空实现，让 code review 看得见这是**做出的决定**，
    /// 而不是**忘记的接线**。
    fn record(&mut self, note: &DecisionNote);
}

/// 用三个闭包拼出 [`IngestPort`]，避免调用方为每种组合定义结构体。
///
/// 三个字段都是必填的：没有 `Default`，没有「少传一个就静默不记」的便利构造。
/// 曾经有一个 `Ports::new(put, remember)` 省掉 `record`，那正是静默失效的入口。
pub struct Ports<P, R, D> {
    pub put: P,
    pub remember: R,
    pub record: D,
}

impl<P, R, D> Ports<P, R, D> {
    /// 三个闭包一个都不能少。参数顺序即字段顺序，漏写在编译期就报。
    pub fn new(put: P, remember: R, record: D) -> Self {
        Ports {
            put,
            remember,
            record,
        }
    }
}

impl<P, R, D> IngestPort for Ports<P, R, D>
where
    P: FnMut(&Job) -> Result<bool, String> + Send + 'static,
    R: FnMut(&SensitiveEntry<'_>) + Send + 'static,
    D: FnMut(&DecisionNote) + Send + 'static,
{
    fn put(&mut self, job: &Job) -> Result<bool, String> {
        (self.put)(job)
    }

    fn remember(&mut self, entry: &SensitiveEntry<'_>) {
        (self.remember)(entry)
    }

    fn record(&mut self, note: &DecisionNote) {
        (self.record)(note)
    }
}

/// 一次 sidecar 会话。
pub struct SidecarSession {
    /// 命令写入句柄。**事件接收端不在这里**——`mpsc::Receiver` 不是 `Sync`，
    /// 把它塞进 `Arc<SidecarHandle>` 会让句柄无法送进消费线程。
    handle: SidecarHandle,
    progress: Arc<Mutex<ScrapeProgress>>,
    updates: Receiver<SessionUpdate>,
    /// 用户是否已请求停止（用于区分「用户停」与「崩溃退出」）
    stop_requested: Arc<AtomicBool>,
    /// 占位：SPEC-002 第 7 节要求「崩溃后自动重启一次」，重启策略当前由
    /// app 层决定（core 不擅自复活子进程），此计数保留给 app 层回写。
    restarts: Arc<AtomicU64>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl SidecarSession {
    /// 启动 sidecar 并开始消费事件。
    ///
    /// `sink` 每次调用只处理一条岗位；`profile` / `cfg` 决定筛选口径。
    /// `lookup` 用于在打开详情页之前做敏感库预筛（见 [`SensitiveLookup`]）。
    pub fn start(
        scfg: SidecarConfig,
        profile: Profile,
        icfg: IngestConfig,
        sink: impl IngestPort,
        lookup: impl SensitiveLookup,
    ) -> Result<Self, SidecarError> {
        let (handle, events) = SidecarHandle::spawn(
            &scfg,
            crate::core::sidecar::process::DEFAULT_READY_TIMEOUT,
        )?;

        // 起来≠在抓取。这里硬编码 `running: true` 曾与缺陷 1 同源：只点
        // 「扫码登录」或「启动浏览器」也会让界面显示「抓取中」。真正切到
        // 抓取中由 `send(ScrapeStart)` 负责；重启后续跑由 `adopt_progress()`。
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, rx) = mpsc::channel::<SessionUpdate>();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let restarts = Arc::new(AtomicU64::new(0));

        let join = {
            let progress = Arc::clone(&progress);
            let stop = Arc::clone(&stop_requested);
            std::thread::Builder::new()
                .name("sidecar-session".into())
                .spawn(move || {
                    consume(events, progress, tx, profile, icfg, sink, lookup, stop);
                })
                .map_err(|e| SidecarError::Spawn {
                    program: "sidecar-session".into(),
                    source: e,
                })?
        };

        Ok(Self {
            handle,
            progress,
            updates: rx,
            stop_requested,
            restarts,
            join: Some(join),
        })
    }

    pub fn pid(&self) -> u32 {
        self.handle.pid()
    }

    /// 发命令（`scrape.start` / `scrape.stop` / `login.start` / `session.open`）。
    ///
    /// 只有 `scrape.start` 会把进度切成「抓取中」。这条边界是补出来的：
    /// 早先这里对**所有**命令都置 `running = true`，于是用户只点了一次
    /// 「扫码登录」（一个岗位都没在抓），界面却显示「抓取中」，还会让
    /// `stop()` 把 `finished_reason` 写成「用户停止」——诊断信息全是假的。
    pub fn send(&self, cmd: &Command) -> Result<String, SidecarError> {
        if matches!(cmd, Command::ScrapeStart { .. }) {
            if let Ok(mut p) = self.progress.lock() {
                p.running = true;
                p.finished_reason = None;
                // 新一次抓取从零开始计数，否则界面上的进度会累加
                p.scraped = 0;
                p.inserted = 0;
                p.duplicates = 0;
                p.below_threshold = 0;
                p.rejected = 0;
            }
        }
        self.handle.send(cmd)
    }

    /// 发一条**需要回执**的命令并等结果（聊天类）。
    ///
    /// 与 [`send`](Self::send) 的区别：抓取是长任务，进度靠事件流推进，不能阻塞
    /// 在一条回执上；聊天是请求/应答，命令发出去必须等到同一个 id 的回执。
    ///
    /// 这里**不碰 `progress`**：聊天不是抓取，把它标记成 running 会让界面上的
    /// 「抓取中」状态在用户只是打开会话列表时闪现。
    pub fn request(
        &self,
        cmd: &Command,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        self.handle.request(cmd, timeout)
    }

    /// 拿一份可以脱离会话本身使用的句柄克隆。
    ///
    /// 命令层需要它来做一件很具体的事：聊天请求可能要等几十秒，如果期间一直
    /// 占着 `Mutex<Option<SidecarSession>>`，界面上的 `scrape_progress` 轮询
    /// 会被饿死——表现为「抓取中」的状态数字突然不动了。先克隆句柄、
    /// 立刻还锁、再在锁外等回执，就不会有这个问题。
    pub fn handle_clone(&self) -> SidecarHandle {
        self.handle.clone()
    }

    /// 请求停止。
    ///
    /// 这里**立刻**把 `running` 置 false：`scrape.stop` 只结束 sidecar 内部的
    /// 抓取线程，Python 主进程仍在等 stdin，永远不会发 `Exited`。早先的写法
    /// 只在收到 `Exited` 时清 `running`，实测点「停止」后界面一直停在
    /// 「抓取中…」。`Exited` 之后到达时会用更精确的原因覆盖这里写的值。
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        if let Ok(mut p) = self.progress.lock() {
            if p.running {
                p.running = false;
                p.finished_reason = Some(STOPPED_BY_USER.to_string());
            }
        }
        let _ = self.handle.send(&Command::ScrapeStop);
    }

    /// 非阻塞取一条增量。
    pub fn poll(&self) -> Option<SessionUpdate> {
        self.updates.try_recv().ok()
    }

    /// 取进度快照。
    pub fn progress(&self) -> ScrapeProgress {
        self.progress
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|_| ScrapeProgress::new())
    }

    /// 是否已自动重启过。
    pub fn restarts(&self) -> u64 {
        self.restarts.load(Ordering::Relaxed)
    }

    /// 记一次自动重启。由应用层在重启成功后调用（core 不擅自复活子进程）。
    pub fn bump_restarts(&self) {
        self.restarts.fetch_add(1, Ordering::Relaxed);
    }

    /// 取进度结构的共享句柄，供应用层在重启前后改写 `finished_reason` 等字段。
    pub fn progress_handle(&self) -> Arc<Mutex<ScrapeProgress>> {
        Arc::clone(&self.progress)
    }

    /// 继承上一次会话的抓取计数。
    ///
    /// 崩溃重启后若从零开始计数，界面上刚抓的几十条会「凭空消失」，
    /// 用户会以为数据丢了。登录状态同理：重启前已登录就不该退回 `unknown`
    /// ——扫描扫码登录的用户会被这条错误状态逼着重新扫码。
    /// 结束原因**不继承**：新进程刚起来，它当然没结束。
    pub fn adopt_progress(&self, prev: &ScrapeProgress) {
        if let Ok(mut p) = self.progress.lock() {
            p.scraped = prev.scraped;
            p.inserted = prev.inserted;
            p.duplicates = prev.duplicates;
            p.below_threshold = prev.below_threshold;
            p.rejected = prev.rejected;
            p.last_title = prev.last_title.clone();
            if prev.login_state != "unknown" {
                p.login_state = prev.login_state.clone();
                p.login_platform = prev.login_platform.clone();
            }
            p.running = true;
            p.finished_reason = None;
        }
    }

    /// 最近一次 stderr 快照（退出后用于诊断崩溃原因）。
    pub fn stderr_tail(&self) -> String {
        self.handle.stderr_tail()
    }

    /// 关闭会话：停抓取、关进程、等消费线程收尾。
    pub fn shutdown(&mut self) {
        self.stop();
        self.handle.shutdown(Duration::from_secs(3));
        if let Some(j) = self.join.take() {
            // 消费线程靠 stdout EOF 醒来；极少数情况下它卡在 recv_timeout 上，
            // 这里不 join 而是 detach，避免退出流程被拖住
            let _ = j;
        }
    }
}

impl Drop for SidecarSession {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }
}

/// 消费线程主体。**唯一负责**把事件变成入库动作的地方。
fn consume(
    events: Receiver<SidecarEvent>,
    progress: Arc<Mutex<ScrapeProgress>>,
    tx: Sender<SessionUpdate>,
    profile: Profile,
    icfg: IngestConfig,
    mut sink: impl IngestPort,
    mut lookup: impl SensitiveLookup,
    stop: Arc<AtomicBool>,
) {
    // 用户点「停止」后，抓取线程最多 0.2 s 收尾，之后 sidecar 还会补发
    // 一两条日志。这里不靠它结束循环——结束只认 Exited，否则崩在半路的
    // 状态永远不会被报告。stop 只用于把「用户停」写进结束原因。
    let mut user_stopped = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            user_stopped = true;
        }
        let ev = match events.recv_timeout(Duration::from_millis(300)) {
            Ok(e) => e,
            // 超时只是「暂时没消息」，拿它当结束条件会让抓取刚开始就断开
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            // 读线程已收工（stdout EOF 且退出已上报），这里跟着收
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };

        match ev {
            SidecarEvent::Ready { .. } => {
                // 这里**不置** `running`。早先的注释假设「spawn 阶段已经消费过
                // ready，再来一次只可能是重启后的」，但每次 sidecar 启动都会发
                // `ready`——包括只点「扫码登录」或「启动浏览器」的时候，于是界面
                // 凭空显示「抓取中」（缺陷 1 的另一半）。重启后要继续显示抓取中，
                // 由 `adopt_progress()` 负责；首次抓取由 `send()` 负责。
                if let Ok(mut p) = progress.lock() {
                    p.finished_reason = None;
                }
            }
            SidecarEvent::Event(Event::JobScraped(sj)) => {
                handle_job(&progress, &tx, &profile, &icfg, &mut sink, &mut lookup, sj);
            }
            SidecarEvent::Event(Event::Log { level, msg }) => {
                if let Ok(mut p) = progress.lock() {
                    p.last_log = Some(format!("[{level}] {msg}"));
                }
                let _ = tx.send(SessionUpdate::Log { level, msg });
            }
            SidecarEvent::Event(Event::LoginState {
                platform, state, msg,
            }) => {
                if let Ok(mut p) = progress.lock() {
                    p.login_platform = platform;
                    p.login_state = state.clone();
                }
                let _ = tx.send(SessionUpdate::Login { state, msg });
            }
            SidecarEvent::Event(Event::ScrapeRisk { platform, reason, url }) => {
                // 风控停止：与 ScrapeFinished 同一个 running 守卫，原因写成可辨识的文案，
                // 界面据此区分「被风控停」与「正常抓完」。sidecar 随后仍会发 scrape.finished
                // 收尾，但守卫保证这里先写的风控原因不会被覆盖成「抓取完成」。
                if let Ok(mut p) = progress.lock() {
                    if p.running {
                        p.running = false;
                        p.finished_reason = Some(format!("触发风控已停止：{reason}"));
                    }
                }
                let _ = tx.send(SessionUpdate::Risk { platform, reason, url });
            }
            SidecarEvent::Event(Event::ScrapeFinished { stopped, error, .. }) => {
                // 自然完成也要清 running，否则界面永停「抓取中」（sidecar 抓完
                // 仍守在 stdin 上，永远不会发 Exited）。`p.running` 守卫是与 stop()
                // 的竞态契约：谁先到谁写原因，后到者不覆盖。这里只看帧里的
                // stopped，不用 user_stopped——后者跨轮次粘滞（stop_requested
                // 从不复位），会把上一轮的用户停止误记到新一轮的自然完成上。
                if let Ok(mut p) = progress.lock() {
                    if p.running {
                        p.running = false;
                        p.finished_reason = Some(if stopped {
                            STOPPED_BY_USER.to_string()
                        } else if error.is_empty() {
                            format!("抓取完成，共 {} 条", p.scraped)
                        } else {
                            format!("抓取异常结束：{error}")
                        });
                    }
                }
            }
            SidecarEvent::Malformed { line } => {
                let _ = tx.send(SessionUpdate::Malformed { line });
            }
            SidecarEvent::Exited { code } => {
                let mut p = match progress.lock() {
                    Ok(p) => p,
                    Err(_) => return,
                };
                p.running = false;
                p.finished_reason = Some(if user_stopped {
                    STOPPED_BY_USER.to_string()
                } else {
                    match code {
                        Some(0) => "抓取进程正常退出".to_string(),
                        Some(c) => format!("抓取进程异常退出，退出码 {c}"),
                        None => "抓取进程已断开".to_string(),
                    }
                });
                drop(p);
                let _ = tx.send(SessionUpdate::Exited { code });
                // 退出信号只发一次，消费线程跟着收工
                return;
            }
            SidecarEvent::Event(Event::Ready { .. }) => {}
        }
    }
}

/// 处理一条抓取结果：判定 → 入库 → 记账 → 通知。
///
/// 单独抽成公开函数，是为了让「达标才写 / 重复不写 / 否决不写」这条链
/// 能在没有 Python、没有 Tauri 的环境下直接单测。
pub fn handle_job(
    progress: &Arc<Mutex<ScrapeProgress>>,
    tx: &Sender<SessionUpdate>,
    profile: &Profile,
    icfg: &IngestConfig,
    sink: &mut impl IngestPort,
    lookup: &mut dyn SensitiveLookup,
    sj: ScrapedJob,
) {
    // 0) 卡片阶段的预筛：命中敏感库 / 卡片重复 / 不依赖 JD 的硬规则
    //    —— 这一步**发生在详情页之前**，命中就直接结束，省掉 3~6 秒。
    //
    let pre = prescan(&sj, icfg, |p, c, t| lookup.hit(p, c, t));
    if let Some(pre) = pre {
        let summary = pre.summary();
        let is_dup = matches!(pre, PreScan::Duplicate);
        if let Ok(mut p) = progress.lock() {
            p.scraped += 1;
            if is_dup {
                p.duplicates += 1;
            } else {
                // 敏感库命中和规则否决都记在 rejected 里：对用户来说
                // 两者都是「这条我不要」，分开计只会让人困惑
                p.rejected += 1;
            }
        }
        let verdict = IngestVerdict::Rejected {
            // 规则标识由 `PreScan` 自己给出：敏感库（`sensitive:*`）与卡片规则
            // （`prescan:<规则名>`）是两个不同的口径，用户裁决里分开统计。
            // 曾经这里一律写死 `"prescan"`，决策日志里看不出命中的是哪条规则。
            rule: pre.rule_ident(),
            reason: summary.clone(),
        };
        // 卡在预筛的岗位同样要留决策记录：用户点「排除」筛选时，命中的
        // 大多是这条路径（敏感库 / 卡片规则），不记就等于筛选档恒空。
        // 重复命中不记——那是「已经有的岗位」，不是一次排除决策。
        if !is_dup {
            sink.record(&decision_note(&sj, &verdict));
        }
        let _ = tx.send(SessionUpdate::Job { verdict, summary });
        return;
    }

    let verdict = prepare(&sj, profile, icfg);
    let mut summary = verdict.summary();
    // 记账分支：0=入库 1=重复 2=门槛 3=规则否决 4=写库失败
    let mut bucket = match &verdict {
        IngestVerdict::Accepted(_) => 0,
        IngestVerdict::Rejected { .. } => 3,
        IngestVerdict::BelowThreshold { .. } => 2,
    };
    let mut title: Option<String> = None;

    if let IngestVerdict::Accepted(detail) = &verdict {
        title = detail.job.title.clone();
        match sink.put(&detail.job) {
            // insert_job 是 INSERT OR IGNORE，false 表示 job_id 已存在
            Ok(true) => {}
            Ok(false) => {
                bucket = 1;
                summary = format!("{summary}（已存在，跳过）");
            }
            Err(e) => {
                // 写库失败既不算入库也不算被筛掉，单独记在 last_log 里，
                // 否则用户会以为「抓到了 10 条却一条没进」
                bucket = 4;
                summary = format!("入库失败：{e}");
            }
        }
    }

    // 被规则否决的岗位记进敏感库：下次抓取在卡片阶段就能跳过，
    // 不必再花 3~6 秒开详情页。重复（bucket=1）与写库失败不记。
    // 落库由 sink 的 add_sensitive 完成——session 层不碰数据库。
    if bucket == 3 {
        if let IngestVerdict::Rejected { rule, reason } = &verdict {
            let entry = SensitiveEntry {
                platform: &sj.platform,
                company: sj.company.as_deref(),
                title: sj.title.as_deref(),
                reason: &format!("{rule}：{reason}"),
                kind: SensitiveKind::Rejected,
                job_id: Some(sj.job_id.as_str()),
                salary: sj.salary.as_deref(),
                location: sj.location.as_deref(),
                job_url: sj.job_url.as_deref(),
            };
            sink.remember(&entry);
            // 同一次规则否决既进敏感库（下次卡片阶段直接跳过），
            // 也进决策日志（用户回看「为什么这个岗位被排除」）。
            // 两处受众不同，不能只写一处。
            sink.record(&decision_note(&sj, &verdict));
        }
    }

    if let Ok(mut p) = progress.lock() {
        p.scraped += 1;
        match bucket {
            0 => {
                p.inserted += 1;
                p.last_title = title;
            }
            1 => {
                p.duplicates += 1;
                p.last_title = title;
            }
            2 => p.below_threshold += 1,
            3 => p.rejected += 1,
            _ => {}
        }
        if bucket == 4 {
            p.last_log = Some(summary.clone());
        }
    }

    // 决策日志：入库（0）与门槛不足（2）都在这里记；规则否决（3）在上面
    // `remember` 分支已经记过，重复（1）与写库失败（4）不是决策，不记。
    // 放在记账之后：即便界面刷新慢一拍，统计与日志也不会互相矛盾。
    match bucket {
        0 | 2 => sink.record(&decision_note(&sj, &verdict)),
        _ => {}
    }
    let _ = tx.send(SessionUpdate::Job { verdict, summary });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::profile::DEFAULT_PROFILE;
    use crate::core::domain::rules::ExcludeRules;

    fn high_score_job(id: &str) -> ScrapedJob {
        ScrapedJob {
            job_id: id.into(),
            platform: "boss".into(),
            title: Some("Java 后端".into()),
            company: Some("上海某某信息科技有限公司".into()),
            salary: Some("25-40K".into()),
            experience: Some("5-10年".into()),
            jd_text: Some(format!("熟悉 {}", DEFAULT_PROFILE.core_skills.join("、"))),
            hr_active_status: Some("今日活跃".into()),
            ..Default::default()
        }
    }

    #[test]
    fn accepted_job_is_written_and_counted() {
        // 计数用 Arc<Mutex<_>>：sink 的返回类型带 'static + Send，闭包不能按引用借用局部量
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut sink = {
            let seen = Arc::clone(&seen);
            Ports::new(
                move |job: &Job| {
                    seen.lock().unwrap().push(job.job_id.clone());
                    Ok(true)
                },
                |_: &SensitiveEntry<'_>| {},
                // 本用例只验入库与记账；决策日志另有专测
                |_: &DecisionNote| {},
            )
        };
        let icfg = IngestConfig {
            min_score: 0,
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, rx) = mpsc::channel();

        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &icfg,
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:1"),
        );

        assert_eq!(*seen.lock().unwrap(), vec!["boss:1".to_string()]);
        let snap = progress.lock().unwrap().clone();
        assert_eq!(snap.scraped, 1);
        assert_eq!(snap.inserted, 1);
        assert_eq!(snap.duplicates, 0);
        assert_eq!(snap.last_title.as_deref(), Some("Java 后端"));
        assert!(matches!(rx.try_recv(), Ok(SessionUpdate::Job { .. })));
    }

    #[test]
    fn duplicate_is_counted_separately_from_inserted() {
        // insert_job 返回 false = job_id 已存在；此时不能计入 inserted，
        // 否则界面会显示「入库 10 条」而数据库一条没多
        let mut sink = Ports::new(
            |_job: &Job| Ok(false),
            |_: &SensitiveEntry<'_>| {},
            |_: &DecisionNote| {},
        );
        let icfg = IngestConfig {
            min_score: 0,
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &icfg,
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:1"),
        );
        let snap = progress.lock().unwrap().clone();
        assert_eq!(snap.scraped, 1);
        assert_eq!(snap.inserted, 0);
        assert_eq!(snap.duplicates, 1);
        match rx.try_recv() {
            Ok(SessionUpdate::Job { summary, .. }) => assert!(summary.contains("已存在")),
            other => panic!("实际 {other:?}"),
        }
    }

    #[test]
    fn store_error_does_not_count_as_inserted() {
        let mut sink = Ports::new(
            |_job: &Job| Err("数据库只读".to_string()),
            |_: &SensitiveEntry<'_>| {},
            |_: &DecisionNote| {},
        );
        let icfg = IngestConfig {
            min_score: 0,
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &icfg,
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:3"),
        );
        let snap = progress.lock().unwrap().clone();
        assert_eq!(snap.inserted, 0, "写库失败不能算入库");
        assert_eq!(snap.scraped, 1);
        assert!(snap.last_log.unwrap_or_default().contains("入库失败"));
        match rx.try_recv() {
            Ok(SessionUpdate::Job { summary, .. }) => assert!(summary.contains("入库失败")),
            other => panic!("实际 {other:?}"),
        }
    }

    #[test]
    fn rejected_job_is_never_sent_to_sink() {
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut sink = {
            let called = Arc::clone(&called);
            Ports::new(
                move |_job: &Job| {
                    called.store(true, Ordering::SeqCst);
                    Ok(true)
                },
                |_: &SensitiveEntry<'_>| {},
                |_: &DecisionNote| {},
            )
        };
        let sj = ScrapedJob {
            job_id: "boss:2".into(),
            platform: "boss".into(),
            title: Some("Java 外包开发".into()),
            jd_text: Some("驻场".into()),
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig::default(),
            &mut sink,
            &mut NoSensitive,
            sj,
        );
        assert!(!called.load(Ordering::SeqCst), "被否决的岗位绝不能写库");
        let snap = progress.lock().unwrap().clone();
        assert_eq!(snap.rejected, 1);
        assert_eq!(snap.inserted, 0);
    }

    #[test]
    fn progress_defaults_are_unknown() {
        let p = ScrapeProgress::new();
        assert_eq!(p.login_state, "unknown");
        assert!(p.login_platform.is_empty());
        assert!(!p.running);
        assert_eq!(p.scraped, 0);
    }

    /// `ready` 事件不把状态切成「抓取中」。
    ///
    /// 起因是一次真实回归：用户只点了「扫码登录」，一个岗位都没在抓，界面却
    /// 显示「抓取中」（缺陷 1）。`ready` 每次 sidecar 启动都会发，早先的写法
    /// 在这里无条件置 `running`，靠 `consume` 的循环直接喂事件把这条钉死。
    #[test]
    fn ready_event_does_not_mark_running() {
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        // 先塞 ready，再断开：consume 收完后遇 Disconnected 自行返回
        events_tx.send(SidecarEvent::Ready { pid: 1 }).unwrap();
        drop(events_tx);
        consume(
            events_rx,
            Arc::clone(&progress),
            tx,
            DEFAULT_PROFILE.clone(),
            IngestConfig::default(),
            Ports::new(
                |_: &Job| Ok(false),
                |_: &SensitiveEntry<'_>| {},
                |_: &DecisionNote| {},
            ),
            NoSensitive,
            Arc::new(AtomicBool::new(false)),
        );
        assert!(
            !progress.lock().unwrap().running,
            "ready 不该把状态切成「抓取中」"
        );
    }

    /// `login.state` 事件必须把 platform 连同 state 一起记进 progress：
    /// lib.rs 的 login-state watcher 按 (platform, state) 去重广播，
    /// 丢了 platform 就分不清是哪个平台的登录态（契约字段会恒为空）。
    #[test]
    fn login_event_records_platform_and_state() {
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        events_tx
            .send(SidecarEvent::Event(Event::LoginState {
                platform: "boss".into(),
                state: "logged_in".into(),
                msg: "登录成功".into(),
            }))
            .unwrap();
        drop(events_tx);
        consume(
            events_rx,
            Arc::clone(&progress),
            tx,
            DEFAULT_PROFILE.clone(),
            IngestConfig::default(),
            Ports::new(
                |_: &Job| Ok(false),
                |_: &SensitiveEntry<'_>| {},
                |_: &DecisionNote| {},
            ),
            NoSensitive,
            Arc::new(AtomicBool::new(false)),
        );
        let snap = progress.lock().unwrap().clone();
        assert_eq!(snap.login_state, "logged_in");
        assert_eq!(snap.login_platform, "boss");
        assert!(matches!(rx.try_recv(), Ok(SessionUpdate::Login { .. })));
    }

    /// 被规则否决的岗位要进敏感库，这样下次抓取能省掉一整轮详情页。
    #[test]
    fn rejected_job_is_remembered_for_next_run() {
        let kept = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut sink = {
            let kept = Arc::clone(&kept);
            Ports::new(
                |_: &Job| Ok(true),
                move |e: &SensitiveEntry<'_>| {
                    let line = format!(
                        "{}|{}|{}",
                        e.kind.as_str(),
                        e.company.unwrap_or("-"),
                        e.title.unwrap_or("-")
                    );
                    kept.lock().unwrap().push(line);
                },
                |_: &DecisionNote| {},
            )
        };
        // 必须用「只有看了 JD 才判得出来」的规则（外包/驻场关键词在 JD 里）：
        // 只靠卡片信息的规则会在 prescan 阶段就命中，根本走不到这里。
        let sj = ScrapedJob {
            job_id: "boss:3".into(),
            platform: "boss".into(),
            title: Some("Java 后端开发".into()),
            company: Some("某某信息科技有限公司".into()),
            jd_text: Some("本岗位为外包驻场，需长期派驻客户现场".into()),
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig::default(),
            &mut sink,
            &mut NoSensitive,
            sj,
        );
        let kept = kept.lock().unwrap();
        let rejected = progress.lock().unwrap().rejected;
        assert_eq!(kept.len(), 1, "被否决一次就应记一条敏感库（rejected={rejected}）");
        assert!(kept[0].starts_with("rejected|某某信息科技有限公司|Java 后端开发"));
    }

    /// 入库成功不该走 rejected 分支（它在 put 里自己记 stored）。
    #[test]
    fn accepted_job_is_not_remembered_as_rejected() {
        let rejected = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut sink = {
            let rejected = Arc::clone(&rejected);
            Ports::new(
                |_: &Job| Ok(true),
                move |_: &SensitiveEntry<'_>| {
                    rejected.fetch_add(1, Ordering::SeqCst);
                },
                |_: &DecisionNote| {},
            )
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig {
                min_score: 0,
                ..Default::default()
            },
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:9"),
        );
        assert_eq!(rejected.load(Ordering::SeqCst), 0);
    }

    // ---- 决策日志写入链 ----
    //
    // 这一组是本轮的核心验收：此前 `jd_decisions` 在生产路径上**只有** `rescore`
    // 一种 kind 有数据，`reject` / `accept` 恒空，界面上「为什么这个岗位被排除」
    // 无从回答。下面几条分别钉住入库、规则否决、门槛不足、重复四种结论。

    /// 入库 → 一条 `accept`，带入库分与公司/岗位。
    #[test]
    fn accepted_job_records_accept_decision() {
        let notes = Arc::new(Mutex::new(Vec::<DecisionNote>::new()));
        let mut sink = {
            let notes = Arc::clone(&notes);
            Ports {
                put: |_: &Job| Ok(true),
                remember: |_: &SensitiveEntry<'_>| {},
                record: move |n: &DecisionNote| notes.lock().unwrap().push(n.clone()),
            }
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig {
                min_score: 0,
                ..Default::default()
            },
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:acc"),
        );
        let notes = notes.lock().unwrap();
        assert_eq!(notes.len(), 1, "入库必须留一条决策日志");
        let n = &notes[0];
        assert_eq!(n.decision, "accept");
        assert_eq!(n.platform, "boss");
        assert_eq!(n.company_job, "上海某某信息科技有限公司/Java 后端");
        assert!(n.new_score.unwrap_or(0) > 0, "入库记录必须带入库分");
    }

    /// 规则否决 → 一条 `reject`，reason 里带规则名。
    #[test]
    fn rejected_job_records_reject_decision() {
        let notes = Arc::new(Mutex::new(Vec::<DecisionNote>::new()));
        let mut sink = {
            let notes = Arc::clone(&notes);
            Ports {
                put: |_: &Job| Ok(true),
                remember: |_: &SensitiveEntry<'_>| {},
                record: move |n: &DecisionNote| notes.lock().unwrap().push(n.clone()),
            }
        };
        // 外包关键词只出现在 JD 里：卡片阶段判不出来，必然走到 prepare 的规则否决
        let sj = ScrapedJob {
            job_id: "boss:rej".into(),
            platform: "boss".into(),
            title: Some("Java 后端开发".into()),
            company: Some("某某信息科技有限公司".into()),
            jd_text: Some("本岗位为外包驻场，需长期派驻客户现场".into()),
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig::default(),
            &mut sink,
            &mut NoSensitive,
            sj,
        );
        let notes = notes.lock().unwrap();
        assert_eq!(notes.len(), 1, "规则否决必须留一条决策日志（且只有一条）");
        let n = &notes[0];
        assert_eq!(n.decision, "reject");
        assert_eq!(n.company_job, "某某信息科技有限公司/Java 后端开发");
        // 规则名必须是**具体那条**（JD 命中外包），不是笼统的 "exclude"
        assert_eq!(n.rule, "outsource", "reason 必须带具体规则名：{}", n.reason);
        assert!(n.reason.starts_with("outsource："), "{}", n.reason);
        assert_eq!(notes[0].new_score, None, "被否决的岗位没有入库分");
    }

    /// 卡片阶段被硬规则否决 → 决策日志里要能看出**具体是哪条卡片规则**。
    ///
    /// 回归：这里原先一律写死 `rule = "prescan"`，决策日志只说得清「被预筛拒了」，
    /// 说不清被哪条规则拒了——用户问「为什么这个岗位被排除」仍然答不上来。
    #[test]
    fn prescan_rule_records_the_concrete_card_rule() {
        let notes = Arc::new(Mutex::new(Vec::<DecisionNote>::new()));
        let mut sink = {
            let notes = Arc::clone(&notes);
            Ports {
                put: |_: &Job| Ok(true),
                remember: |_: &SensitiveEntry<'_>| {},
                record: move |n: &DecisionNote| notes.lock().unwrap().push(n.clone()),
            }
        };
        // 薪资上限 20K：卡片字段就够判，不必开详情页
        let cfg = IngestConfig {
            min_score: 0,
            rules: ExcludeRules {
                exclude_salary_max: Some(20),
                ..Default::default()
            },
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &cfg,
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:prescan-rule"),
        );
        let notes = notes.lock().unwrap();
        assert_eq!(notes.len(), 1, "卡片规则否决也要留决策记录");
        let n = &notes[0];
        assert_eq!(n.decision, "reject", "规则否决是排除，不是 skip");
        // 稳定标识只到「预筛」这一档（要落库、要能稳定匹配）；
        // 具体哪条卡片规则必须能在 reason 里读到。
        assert_eq!(n.rule, "prescan", "{}", n.reason);
        assert!(
            n.reason.starts_with("prescan:salary_ceiling："),
            "{}",
            n.reason
        );
        assert!(n.reason.contains("高于上限"), "{}", n.reason);
        assert_eq!(
            progress.lock().unwrap().rejected,
            1,
            "卡片规则否决计入「规则否决」，方案口径见 ScrapeProgress"
        );
    }

    /// 敏感库命中与卡片规则否决是**两个口径**，决策日志里必须能分开。
    #[test]
    fn prescan_sensitive_keeps_its_own_rule_ident() {
        struct Hit;
        impl SensitiveLookup for Hit {
            fn hit(&mut self, _p: &str, _c: Option<&str>, _t: Option<&str>) -> Option<(String, bool)> {
                Some(("之前被排除过".into(), false))
            }
        }

        let notes = Arc::new(Mutex::new(Vec::<DecisionNote>::new()));
        let mut sink = {
            let notes = Arc::clone(&notes);
            Ports {
                put: |_: &Job| Ok(true),
                remember: |_e: &SensitiveEntry<'_>| {},
                record: move |n: &DecisionNote| notes.lock().unwrap().push(n.clone()),
            }
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig::default(),
            &mut sink,
            &mut Hit,
            high_score_job("boss:sensitive"),
        );
        let notes = notes.lock().unwrap();
        assert_eq!(notes.len(), 1);
        let n = &notes[0];
        assert_eq!(n.rule, "sensitive", "{}", n.reason);
        assert!(
            !n.rule.starts_with("prescan"),
            "敏感库不能被归成卡片规则"
        );
        assert!(n.reason.contains("敏感库"), "{}", n.reason);
    }

    /// 门槛不足 → 一条 `skip`（前端归到「其它」档），不能记成 reject。
    #[test]
    fn below_threshold_job_records_skip_decision() {
        let notes = Arc::new(Mutex::new(Vec::<DecisionNote>::new()));
        let mut sink = {
            let notes = Arc::clone(&notes);
            Ports {
                put: |_: &Job| Ok(true),
                remember: |_: &SensitiveEntry<'_>| {},
                record: move |n: &DecisionNote| notes.lock().unwrap().push(n.clone()),
            }
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig {
                // 门槛拉到满分以上：任何岗位都进不来，但走的是「分数不够」而非规则否决
                min_score: 100,
                ..Default::default()
            },
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:low"),
        );
        let notes = notes.lock().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].decision, "skip");
        assert!(notes[0].reason.contains("门槛"), "{}", notes[0].reason);
    }

    /// 重复岗位不是决策：不写决策日志，否则日志会被每次重抓的旧岗位灌满。
    #[test]
    fn duplicate_job_records_no_decision() {
        let notes = Arc::new(Mutex::new(Vec::<DecisionNote>::new()));
        let mut sink = {
            let notes = Arc::clone(&notes);
            Ports {
                put: |_: &Job| Ok(false),
                remember: |_: &SensitiveEntry<'_>| {},
                record: move |n: &DecisionNote| notes.lock().unwrap().push(n.clone()),
            }
        };
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        let (tx, _rx) = mpsc::channel();
        handle_job(
            &progress,
            &tx,
            &DEFAULT_PROFILE,
            &IngestConfig {
                min_score: 0,
                ..Default::default()
            },
            &mut sink,
            &mut NoSensitive,
            high_score_job("boss:dup"),
        );
        assert!(notes.lock().unwrap().is_empty(), "重复不是决策，不该记日志");
        assert_eq!(progress.lock().unwrap().duplicates, 1);
    }

    // ---- scrape.finished：自然完成清 running ----
    //
    // 缺陷现场：sidecar 抓完只 log「抓取结束」，仍守在 stdin 上不发 Exited，
    // Rust 侧 running 永远 true，界面永停「抓取中」直到用户手动停止。

    /// 直接喂事件给 consume（与 ready/login 用例同风格）。
    fn consume_events(
        events: Vec<SidecarEvent>,
        progress: &Arc<Mutex<ScrapeProgress>>,
        put_ok: bool,
        stop: Arc<AtomicBool>,
    ) {
        let (tx, _rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        for ev in events {
            events_tx.send(ev).unwrap();
        }
        drop(events_tx);
        consume(
            events_rx,
            Arc::clone(progress),
            tx,
            DEFAULT_PROFILE.clone(),
            IngestConfig {
                min_score: 0,
                ..Default::default()
            },
            Ports::new(
                move |_: &Job| Ok(put_ok),
                |_: &SensitiveEntry<'_>| {},
                |_: &DecisionNote| {},
            ),
            NoSensitive,
            stop,
        );
    }

    #[test]
    fn scrape_finished_clears_running_and_keeps_counts() {
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        // send(ScrapeStart) 已把状态切成「抓取中」
        progress.lock().unwrap().running = true;
        consume_events(
            vec![
                SidecarEvent::Event(Event::JobScraped(high_score_job("boss:fin1"))),
                SidecarEvent::Event(Event::ScrapeFinished {
                    count: 1,
                    stopped: false,
                    error: String::new(),
                }),
            ],
            &progress,
            true,
            Arc::new(AtomicBool::new(false)),
        );
        let snap = progress.lock().unwrap().clone();
        assert!(!snap.running, "自然完成必须退出「抓取中」");
        assert_eq!(snap.scraped, 1, "完成不得清计数");
        assert_eq!(snap.inserted, 1, "完成不得清计数");
        assert_eq!(snap.finished_reason.as_deref(), Some("抓取完成，共 1 条"));
    }

    #[test]
    fn scrape_finished_with_stopped_flag_reports_user_stop() {
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        progress.lock().unwrap().running = true;
        consume_events(
            vec![SidecarEvent::Event(Event::ScrapeFinished {
                count: 0,
                stopped: true,
                error: String::new(),
            })],
            &progress,
            true,
            Arc::new(AtomicBool::new(false)),
        );
        let snap = progress.lock().unwrap().clone();
        assert!(!snap.running);
        assert_eq!(snap.finished_reason.as_deref(), Some(STOPPED_BY_USER));
    }

    #[test]
    fn scrape_finished_with_error_reports_abnormal_end() {
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        progress.lock().unwrap().running = true;
        consume_events(
            vec![SidecarEvent::Event(Event::ScrapeFinished {
                count: 0,
                stopped: false,
                error: "RuntimeError: 页面崩溃".into(),
            })],
            &progress,
            true,
            Arc::new(AtomicBool::new(false)),
        );
        let snap = progress.lock().unwrap().clone();
        assert!(!snap.running);
        assert_eq!(
            snap.finished_reason.as_deref(),
            Some("抓取异常结束：RuntimeError: 页面崩溃")
        );
    }

    /// stop_requested 从不复位：上一轮用户停止后，新一轮自然完成不得被
    /// 粘滞的 user_stopped 误记成「已按用户请求停止」。只信帧里的 stopped。
    #[test]
    fn scrape_finished_ignores_sticky_stop_flag_from_previous_run() {
        let progress = Arc::new(Mutex::new(ScrapeProgress::new()));
        progress.lock().unwrap().running = true;
        consume_events(
            vec![SidecarEvent::Event(Event::ScrapeFinished {
                count: 0,
                stopped: false,
                error: String::new(),
            })],
            &progress,
            true,
            Arc::new(AtomicBool::new(true)),
        );
        let snap = progress.lock().unwrap().clone();
        assert!(!snap.running);
        assert_eq!(snap.finished_reason.as_deref(), Some("抓取完成，共 0 条"));
    }

    // ---- 手动 stop 与自然完成的竞态（真 sidecar 进程 + 真 stop()）----

    fn wait_until(mut pred: impl FnMut() -> bool, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if pred() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        pred()
    }

    fn start_cmd() -> Command {
        Command::ScrapeStart {
            platform: "boss".into(),
            keywords: vec!["Java".into()],
            max_jd: 10,
            min_score: 70,
            city: None,
            salary: Vec::new(),
            experience: Vec::new(),
            degree: Vec::new(),
            scale: Vec::new(),
            stage: Vec::new(),
            industry: Vec::new(),
            job_type: None,
            max_pages: None,
        }
    }

    fn fake_session(tag: &str, script: &str) -> (SidecarSession, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("aja-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        std::fs::write(dir.join("sidecar.py"), script).expect("写假 sidecar");
        let session = SidecarSession::start(
            SidecarConfig::new(&dir),
            DEFAULT_PROFILE.clone(),
            IngestConfig::default(),
            Ports::new(
                |_: &Job| Ok(true),
                |_: &SensitiveEntry<'_>| {},
                |_: &DecisionNote| {},
            ),
            NoSensitive,
        )
        .expect("spawn 假 sidecar");
        (session, dir)
    }

    /// 完成先到、停止后到：stop() 不得把「抓取完成」改写成「用户停止」，
    /// 随后到达的 stopped=true 完成帧也不得二次触发。
    #[test]
    fn finished_then_stop_keeps_completion_reason() {
        let (mut session, dir) = fake_session(
            "finish-first",
            r#"import sys, os, json
def emit(p):
    print(json.dumps(p), flush=True)
emit({'event': 'ready', 'data': {'pid': os.getpid()}})
for line in sys.stdin:
    try:
        cmd = json.loads(line).get('cmd')
    except Exception:
        continue
    if cmd == 'scrape.start':
        emit({'event': 'job.scraped', 'data': {'job_id': 'boss:f1', 'title': 'Java'}})
        emit({'event': 'scrape.finished', 'data': {'count': 1, 'stopped': False, 'error': ''}})
    elif cmd == 'scrape.stop':
        emit({'event': 'scrape.finished', 'data': {'count': 1, 'stopped': True, 'error': ''}})
"#,
        );
        session.send(&start_cmd()).expect("scrape.start");
        assert!(
            wait_until(|| !session.progress().running, Duration::from_secs(10)),
            "scrape.finished 应把 running 清掉"
        );
        let snap = session.progress();
        assert_eq!(snap.finished_reason.as_deref(), Some("抓取完成，共 1 条"));
        assert_eq!(snap.scraped, 1);
        session.stop();
        // 等第二条 stopped=true 的完成帧被消费：不得覆盖已完成的原因
        std::thread::sleep(Duration::from_millis(500));
        let snap = session.progress();
        assert!(!snap.running);
        assert_eq!(snap.finished_reason.as_deref(), Some("抓取完成，共 1 条"));
        assert_eq!(snap.scraped, 1);
        session.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 停止先到、完成后到：完成帧不得把「已按用户请求停止」改写成「抓取完成」。
    #[test]
    fn stop_then_finished_keeps_user_stop_reason() {
        let (mut session, dir) = fake_session(
            "stop-first",
            r#"import sys, os, json
def emit(p):
    print(json.dumps(p), flush=True)
emit({'event': 'ready', 'data': {'pid': os.getpid()}})
for line in sys.stdin:
    try:
        cmd = json.loads(line).get('cmd')
    except Exception:
        continue
    if cmd == 'scrape.start':
        emit({'event': 'job.scraped', 'data': {'job_id': 'boss:s1', 'title': 'Java'}})
    elif cmd == 'scrape.stop':
        emit({'event': 'scrape.finished', 'data': {'count': 1, 'stopped': True, 'error': ''}})
"#,
        );
        session.send(&start_cmd()).expect("scrape.start");
        assert!(
            wait_until(|| session.progress().scraped >= 1, Duration::from_secs(10)),
            "假 sidecar 应先推一条岗位"
        );
        assert!(session.progress().running);
        session.stop();
        // 等 stopped=true 的完成帧到达：必须被 p.running 守卫忽略
        std::thread::sleep(Duration::from_millis(500));
        let snap = session.progress();
        assert!(!snap.running);
        assert_eq!(snap.finished_reason.as_deref(), Some(STOPPED_BY_USER));
        assert_eq!(snap.scraped, 1, "停止不得清计数");
        session.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
