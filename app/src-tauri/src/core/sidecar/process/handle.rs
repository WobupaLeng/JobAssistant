//! 句柄：启动、发命令、等到 ready、优雅关停。
//!
//! 从 `process.rs` 搬出（行为零变化）：`SidecarError` / `SidecarEvent` /
//! `SidecarHandle` 及其实例方法。事件类型与错误类型跟着句柄走——它们是这个
//! 句柄对上层暴露的接口面，读代码时不该跳文件。

use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::sidecar::protocol::{Command as ProtocolCommand, Event};

use super::codec::{protocol_line, spawn_stderr_reader, spawn_stdout_reader, spawn_waiter};
use super::config::SidecarConfig;
/// sidecar 进程生命周期与协议事件：读线程向上层冒泡的唯一通道。
///
/// `Debug` / `Clone` 不能省：`codec` 的读线程要在分支间移动它，测试也要打印它。
#[derive(Debug, Clone)]
pub enum SidecarEvent {
    /// 子进程已启动、报出自己的 pid
    Ready { pid: u32 },
    /// 协议事件（`job.scraped` / `log` / `login.state`）
    Event(Event),
    /// 收到一行无法解析的内容。**不静默丢弃**：Python traceback 会混进
    /// stdout，丢掉它就没法排查抓取失败。
    Malformed { line: String },
    /// 子进程已退出
    Exited { code: Option<i32> },
}

/// 启动 / 通信错误。
#[derive(Debug)]
pub enum SidecarError {
    /// `python` 不存在或无法执行
    Spawn {
        program: String,
        source: std::io::Error,
    },
    /// 等不到 `ready`：大概率是 sidecar.py 导入即崩，`stderr` 里有关键信息
    NotReady {
        waited: Duration,
        stderr: String,
    },
    /// 写 stdin 失败：子进程已退出或管道已关闭
    Write(std::io::Error),
    /// 进程意外消失
    Crashed { code: Option<i32> },
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SidecarError::Spawn { program, source } => {
                write!(f, "无法启动 Python（{program}）：{source}")
            }
            SidecarError::NotReady { waited, stderr } => write!(
                f,
                "sidecar 启动后 {:.1}s 内未就绪；stderr：{}",
                waited.as_secs_f32(),
                if stderr.trim().is_empty() {
                    "（空）"
                } else {
                    stderr.trim()
                }
            ),
            SidecarError::Write(e) => write!(f, "写入 sidecar 失败：{e}"),
            SidecarError::Crashed { code } => match code {
                Some(c) => write!(f, "sidecar 已退出，退出码 {c}"),
                None => write!(f, "sidecar 已退出（退出码未知）"),
            },
        }
    }
}

impl std::error::Error for SidecarError {}
pub struct SidecarHandle {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// 子进程 stderr 的尾部快照（`ready` 前用于诊断）
    stderr_tail: Arc<Mutex<String>>,
    next_id: Arc<AtomicU64>,
    pid: u32,
    /// 在途请求：命令 id → 回执投递口。
    ///
    /// 抓取命令不需要它（进度走 event 流），但聊天是**请求/应答**：
    /// 发出去必须能等到同一个 id 的回执。读线程收到一帧无 `event` 的
    /// 回执时先查这张表，命中就投递并从事件流里摘掉——否则会像历史上
    /// 那样被当成 `Ignore` 静默丢弃。
    pending: Arc<Mutex<HashMap<String, Sender<SidecarReply>>>>,
}

/// 手写 `Clone` 而不是 `derive`：`AtomicU64` 自带 `Clone` 但那是**复制值**，
/// 两个句柄会各自从同一个值开始发号，聊天回执就可能被投递给错误的等待者，
/// 而且不报错。这里必须让克隆共享进位计数，所以 `next_id` 用独立 `Arc` 包住。
impl Clone for SidecarHandle {
    fn clone(&self) -> Self {
        Self {
            child: Arc::clone(&self.child),
            stdin: Arc::clone(&self.stdin),
            stderr_tail: Arc::clone(&self.stderr_tail),
            next_id: Arc::clone(&self.next_id),
            pid: self.pid,
            pending: Arc::clone(&self.pending),
        }
    }
}

/// 一条命令的回执。
#[derive(Debug, Clone, Default)]
pub struct SidecarReply {
    pub ok: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

impl SidecarReply {
    /// 回执是否成功。失败分支把 `error` 抬成错误串供上层直接展示。
    pub fn into_result(self) -> Result<serde_json::Value, String> {
        if self.ok {
            Ok(self.data)
        } else {
            Err(self.error.unwrap_or_else(|| "sidecar 未给出失败原因".to_string()))
        }
    }
}

impl SidecarHandle {
    /// 启动 sidecar 并等到 `ready`。失败分支都会杀掉子进程，不留孤儿。
    pub fn spawn(
        cfg: &SidecarConfig,
        ready_timeout: Duration,
    ) -> Result<(Self, Receiver<SidecarEvent>), SidecarError> {
        let script = cfg.script();
        if !script.is_file() {
            return Err(SidecarError::Spawn {
                program: script.display().to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "找不到 sidecar.py，检查 scraper/ 是否随应用发布",
                ),
            });
        }

        let program = cfg.program();
        let mut cmd = Command::new(&program);
        cmd.arg(&script)
            // 强制无缓冲：否则 Python 的 stdout 缓冲会让 Rust 侧一直等不到行
            .arg("-u")
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &cfg.data_dir {
            cmd.env("AJA_DATA_DIR", dir);
        }
        // 子进程绝不能弹控制台窗口：sidecar 是后台执行器
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(|source| SidecarError::Spawn {
            program: program.clone(),
            source,
        })?;
        let pid = child.id();

        let stdin = child.stdin.take();
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                return Err(SidecarError::NotReady {
                    waited: Duration::ZERO,
                    stderr: "无法获取 sidecar 的 stdout".to_string(),
                });
            }
        };
        let stderr = child.stderr.take();

        let (tx, rx) = mpsc::channel::<SidecarEvent>();
        // 回执表必须在读线程启动前建好：读线程会持有它的克隆，
        // 用于把「无 event 的回执帧」路由回各自的等待方。
        let pending: Arc<Mutex<HashMap<String, Sender<SidecarReply>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        spawn_stdout_reader(stdout, tx.clone(), Arc::clone(&pending));

        // stderr 永远要有人读：没人读的管道一旦写满，子进程会卡在 write 上。
        // traceback 是最有价值的排障信息，只保留末尾。
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(err) = stderr {
            spawn_stderr_reader(err, Arc::clone(&stderr_tail));
        }

        let child = Arc::new(Mutex::new(child));
        spawn_waiter(Arc::clone(&child), tx);

        let handle = Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            stderr_tail,
            next_id: Arc::new(AtomicU64::new(1)),
            pid,
            pending,
        };

        // ready 之前的早到事件（例如 sidecar 抢跑打的日志）在这里被丢掉：
        // 调用方拿到 receiver 时应当只看到「ready 之后」的流。
        wait_ready(&handle, &rx, ready_timeout)?;
        Ok((handle, rx))
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// 分配一条命令的关联 ID。用自增而非随机：抓取日志里肉眼能对上号。
    pub fn next_id(&self) -> String {
        format!("c{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// 发送一条命令，返回它的关联 ID。
    pub fn send(&self, cmd: &ProtocolCommand) -> Result<String, SidecarError> {
        let id = self.next_id();
        let line = protocol_line(&id, cmd)
            .map_err(|e| SidecarError::Write(std::io::Error::other(e.to_string())))?;
        self.write_line(&line)?;
        Ok(id)
    }

    /// 发送一条命令并**等待它的回执**。
    ///
    /// 聊天类命令必须用这个方法：它们没有对应的 event 流，只有一条带同一个
    /// `id` 的回执。抓取类命令仍用 [`send`](Self::send)——抓取是长任务，
    /// 进度靠事件流推送，不能阻塞在一条回执上。
    ///
    /// `timeout` 到期会返回错误并从在途表里摘除，避免表无限增长。
    pub fn request(
        &self,
        cmd: &ProtocolCommand,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        let (tx, rx) = mpsc::channel::<SidecarReply>();
        let id = self.next_id();
        {
            let mut guard = self.pending.lock().map_err(|_| "回执表锁已中毒".to_string())?;
            guard.insert(id.clone(), tx);
        }
        let line = match protocol_line(&id, cmd) {
            Ok(l) => l,
            Err(e) => {
                if let Ok(mut g) = self.pending.lock() {
                    g.remove(&id);
                }
                return Err(format!("命令序列化失败：{e}"));
            }
        };
        if let Err(e) = self.write_line(&line) {
            if let Ok(mut g) = self.pending.lock() {
                g.remove(&id);
            }
            return Err(self.explain_write_failure(e));
        }
        match rx.recv_timeout(timeout) {
            Ok(reply) => reply.into_result(),
            Err(_) => {
                // 超时也要摘表：sidecar 可能永远不回了（比如崩溃前那一刻）
                if let Ok(mut g) = self.pending.lock() {
                    g.remove(&id);
                }
                Err(format!(
                    "等待 sidecar 回执超时（{} 秒），命令可能仍在执行",
                    timeout.as_secs()
                ))
            }
        }
    }

    /// 写一行到 sidecar。
    pub fn write_line(&self, line: &str) -> Result<(), SidecarError> {
        let mut guard = self
            .stdin
            .lock()
            .map_err(|_| SidecarError::Write(std::io::Error::other("stdin 锁已中毒")))?;
        let pipe = guard.as_mut().ok_or_else(|| {
            SidecarError::Write(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "sidecar 的 stdin 已关闭",
            ))
        })?;
        // 一次 write_all 写完整行：分两次写会让对端可能读到半行
        let mut buf = String::with_capacity(line.len() + 1);
        buf.push_str(line);
        buf.push('\n');
        pipe.write_all(buf.as_bytes())
            .and_then(|_| pipe.flush())
            .map_err(SidecarError::Write)
    }

    /// 子进程是否还活着。
    pub fn is_alive(&self) -> bool {
        match self.child.lock() {
            Ok(mut c) => matches!(c.try_wait(), Ok(None)),
            Err(_) => false,
        }
    }

    /// 最近一次 stderr 快照。
    pub fn stderr_tail(&self) -> String {
        self.stderr_tail.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 把「写入失败」翻译成一句能排查的话。
    ///
    /// 起因是一次真实故障：用户点「Boss 消息」，界面只得到
    /// `写入 sidecar 失败：sidecar 的 stdin 已关闭`，而 sidecar 其实早已崩掉，
    /// traceback 就存着没人读。写完才死的进程，错误必须指出**为什么**死，
    /// 否则下一个人还得从头猜。
    fn explain_write_failure(&self, err: SidecarError) -> String {
        if self.is_alive() {
            return format!("写入 sidecar 失败：{err}");
        }
        let tail = self.stderr_tail();
        let tail = tail.trim();
        if tail.is_empty() {
            // 没留下 traceback 时给最小可执行信息：进程已退出，重启即可
            format!("写入 sidecar 失败：{err}（sidecar 进程已退出，没有留下错误输出；请重试，仍失败请重启应用）")
        } else {
            format!("写入 sidecar 失败：{err}（sidecar 进程已退出，最后的输出：{tail}）")
        }
    }

    /// 优雅关停：关 stdin → 等待 → kill → 收尾。
    pub fn shutdown(&self, grace: Duration) {
        if let Ok(mut g) = self.stdin.lock() {
            // 关掉 stdin，sidecar 的 `for raw in sys.stdin` 会走到 EOF 并自行退出
            *g = None;
        }
        let deadline = Instant::now() + grace;
        loop {
            {
                let mut c = match self.child.lock() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                match c.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => {}
                    Err(_) => return,
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

}

fn wait_ready(
    handle: &SidecarHandle,
    events: &Receiver<SidecarEvent>,
    timeout: Duration,
) -> Result<(), SidecarError> {
    let start = Instant::now();
    loop {
        let left = timeout.saturating_sub(start.elapsed());
        if left.is_zero() {
            handle.shutdown(Duration::from_millis(200));
            return Err(SidecarError::NotReady {
                waited: start.elapsed(),
                stderr: handle.stderr_tail(),
            });
        }
        match events.recv_timeout(left) {
            Ok(SidecarEvent::Ready { .. }) => return Ok(()),
            // ready 之前的事件不能当成正常事件冒泡出去，否则调用方会看到
            // 「还没 spawn 完就抓到岗位」的时序幻觉；只做丢弃处理。
            Ok(_) => continue,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                handle.shutdown(Duration::from_millis(200));
                return Err(SidecarError::NotReady {
                    waited: start.elapsed(),
                    stderr: handle.stderr_tail(),
                });
            }
        }
    }
}

impl Drop for SidecarHandle {
    fn drop(&mut self) {
        // 应用退出 / 句柄被替换时必须收拾干净，否则 python 会留在后台占着
        // 浏览器 profile，下次启动 patchright 会报「profile 正被使用」
        self.shutdown(Duration::from_millis(800));
    }
}
