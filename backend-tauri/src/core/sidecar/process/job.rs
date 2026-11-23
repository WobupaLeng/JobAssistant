//! Windows Job Object：把 sidecar 及其后代（node driver / 真实 Chrome）纳入同一作业，
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 保证**作业句柄一关闭，整棵进程树被系统回收**。
//!
//! 为什么需要它：Chrome 是 `python → node driver → chrome` 的孙进程。旧关停路径
//! （`handle.rs::shutdown`）超时后只 `TerminateProcess(python)`，patchright 的 atexit
//! 清理不运行，node / Chrome 成为**孤儿**，继续持有该 `--user-data-dir` 的 Chromium
//! 单例消息窗口（标题 = UDF 字面路径）。下一次同 UDF `launch_persistent_context`
//! 命中孤儿窗口 → 「正在现有的浏览器会话中打开」exit 0 → `TargetClosedError`。
//! 把进程树纳入 KILL_ON_JOB_CLOSE 作业后，关句柄即杀光成员，孤儿无从产生。
//!
//! 注意：根 `AGENTS.md` §7.1「排除 Job Object」指排除其作为**撞名根因**，
//! 不禁止其作为**回收修法**——这里正是后者。
//!
//! 非 Windows 平台提供空实现（`new()` 恒 `None`），调用方降级为原有「只杀 python」。

/// 关闭即杀光成员的作业句柄。`Drop` 关闭句柄触发 `KILL_ON_JOB_CLOSE`。
pub struct KillOnCloseJob {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// HANDLE 是裸指针（!Send/!Sync），但作业句柄只在本类型内以 unsafe 使用、
// 外层由 `Mutex` 保护，跨线程传递安全——SidecarHandle 需 Send+Sync 供 tauri manage。
unsafe impl Send for KillOnCloseJob {}
unsafe impl Sync for KillOnCloseJob {}

#[cfg(windows)]
impl KillOnCloseJob {
    /// 创建作业并设「关闭即杀光成员」。任一步失败返回 `None`（调用方降级，不致命）。
    pub fn new() -> Option<Self> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, SetInformationJobObject,
            JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                CloseHandle(handle);
                return None;
            }
            Some(Self { handle })
        }
    }

    /// 把子进程纳入作业。必须在它派生孙进程**之前**调用——`spawn` 返回后立即调用，
    /// 远早于 Python 解释器启动 + 导入 patchright + 派生 node driver（数百毫秒级）。
    /// 子进程的后代自动继承作业成员身份。失败返回 `false`（降级，不致命）。
    pub fn assign_child(&self, child: &std::process::Child) -> bool {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        unsafe { AssignProcessToJobObject(self.handle, child.as_raw_handle() as _) != 0 }
    }
}

#[cfg(windows)]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
        }
    }
}

#[cfg(not(windows))]
impl KillOnCloseJob {
    pub fn new() -> Option<Self> {
        None
    }
    pub fn assign_child(&self, _child: &std::process::Child) -> bool {
        false
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::KillOnCloseJob;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// 等到子进程退出（被作业回收）或超时。
    fn wait_dead(child: &mut std::process::Child, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return false,
            }
        }
        false
    }

    /// 用 OpenProcess + GetExitCodeProcess 判断某 PID 是否仍存活。
    /// 不用 tasklist：受限环境下其输出不可靠；直接查进程退出码最稳。
    fn pid_alive(pid: u32) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false; // 进程已彻底消失
            }
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(h, &mut code);
            CloseHandle(h);
            ok != 0 && code == STILL_ACTIVE as u32
        }
    }

    /// KILL_ON_JOB_CLOSE 回收**直接成员**：长命 ping 被纳入作业，关句柄后必须被杀。
    #[test]
    fn kill_on_close_terminates_member() {
        let Some(job) = KillOnCloseJob::new() else {
            // 作业创建失败（极旧系统/受限环境）：跳过而非误报。
            eprintln!("CreateJobObject 不可用，跳过");
            return;
        };
        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 60 127.0.0.1 >nul"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn ping");
        assert!(job.assign_child(&child), "AssignProcessToJobObject 失败");
        // 确认它确实活着（不会自然退出）。
        assert!(matches!(child.try_wait(), Ok(None)));
        drop(job); // 关句柄 → KILL_ON_JOB_CLOSE
        assert!(wait_dead(&mut child, Duration::from_secs(5)), "成员未被作业回收");
    }

    /// KILL_ON_JOB_CLOSE 回收**整棵进程树**：python（成员）派生孙进程 python，
    /// 关句柄后两者都必须被杀——这正是「强杀 python 不留孤儿 Chrome」的机理证明。
    #[test]
    fn kill_on_close_terminates_descendants() {
        let Some(job) = KillOnCloseJob::new() else {
            eprintln!("CreateJobObject 不可用，跳过");
            return;
        };
        // 成员 python 派生一个长命孙进程 python，打印孙进程 pid 后自己也长睡。
        let code = "import subprocess,sys,time; \
                    p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']); \
                    print(p.pid,flush=True); time.sleep(60)";
        let mut child = Command::new("python")
            .args(["-c", code])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn python");
        assert!(job.assign_child(&child), "AssignProcessToJobObject 失败");
        // 读孙进程 pid。
        let gpid: u32 = {
            use std::io::BufRead;
            let stdout = child.stdout.as_mut().expect("stdout");
            let mut line = String::new();
            let mut reader = std::io::BufReader::new(stdout);
            reader.read_line(&mut line).expect("read grandchild pid");
            line.trim().parse().expect("parse grandchild pid")
        };
        assert!(pid_alive(gpid), "孙进程应已启动");
        drop(job); // 关句柄 → 杀光成员 + 后代
        assert!(wait_dead(&mut child, Duration::from_secs(5)), "成员 python 未被回收");
        // 孙进程无句柄被持有，被杀后应彻底消失。
        let deadline = Instant::now() + Duration::from_secs(5);
        while pid_alive(gpid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!pid_alive(gpid), "孙进程成为孤儿（未被作业回收）");
    }
}
