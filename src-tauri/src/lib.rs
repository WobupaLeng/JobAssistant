//! Tauri 应用库入口：把命令与生命周期封装成 lib，main.rs 只做转发。
//!
//! # 单文件交付策略
//!
//! 目标：**用户只需要一个 AI Job Assistant.exe**，看不到额外的后端 exe。
//!
//! 做法：构建时把 PyInstaller 打出来的 `aja-backend.exe`（约 23 MB）
//! 作为 Tauri resource 嵌进主程序；首次运行时释放到
//! `%LOCALAPPDATA%\\ai-job-assistant\\backend\\<版本哈希>\\aja-backend.exe`，
//! 之后每次启动直接复用（比对哈希，版本没变就不重复解压）。
//!
//! 为什么不真做成"一个二进制文件"：patchright 在运行期需要
//! 独立的 driver 目录与真实 Chrome 二进制，硬塞进单文件既做不到也没意义；
//! 释放到 AppData 是业界通行做法（VS Code、Electron 系应用都这么干）。

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use tauri::Manager;

/// 命令单独放一个模块：Tauri 2 的 `#[tauri::command]` 在本模块内会与
/// `generate_handler!` 生成的同名宏冲突（E0255 defined multiple times），
/// 放到子模块里即可彻底隔离命名空间。
mod commands;

pub use commands::{get_backend_port, get_crash_log_path, get_data_dir};

/// 全局持有的后端子进程，窗口关闭时 kill 掉。
pub struct BackendProcess(pub Mutex<Option<Child>>);

/// 后端实际监听端口，供前端查询。
pub struct BackendPort(pub Mutex<u16>);

const BACKEND_EXE: &str = "aja-backend.exe";

fn project_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap_or(&manifest).to_path_buf()
}

fn backend_dir() -> PathBuf {
    project_root().join("backend")
}

/// 开发态数据目录（项目根 data/）；打包态由 commands::get_data_dir 给出 AppData 路径。
pub fn data_dir() -> PathBuf {
    project_root().join("data")
}

/// 打包态下后端与数据的落地目录：`%LOCALAPPDATA%\ai-job-assistant`
fn app_local_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // 极端情况下的兜底：拿不到 LOCALAPPDATA 就用 exe 所在目录
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."))
        });
    base.join("ai-job-assistant")
}

fn is_packaged() -> bool {
    // debug 构建认为是开发态；release 且 exe 不在 target/ 下即认为已打包
    if cfg!(debug_assertions) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    !exe.to_string_lossy().contains("target")
}

/// 崩溃/异常日志：追加到 `%LOCALAPPDATA%\ai-job-assistant\logs\crash.log`。
///
/// 打包态是窗口程序，没有控制台，`println!` 的输出全丢失——上次「程序直接
/// 被关闭」时一点线索都留不下。所以关键事件（启动、后端退出、panic）都落到文件里。
pub fn crash_log(tag: &str, msg: &str) {
    use std::io::Write;
    let dir = app_local_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("crash.log");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{stamp}] [{tag}] {msg}");
    }
    println!("[{tag}] {msg}");
}

/// 安装 panic 钩子：panic 内容写进 crash.log，而不是静默 abort。
///
/// ⚠️ 这里**故意不调用** `default_hook`：
/// 1. 默认钩子会往 stderr 打一坨给开发者看的文字。打包态下后端也是
///    `CREATE_NO_WINDOW` 子进程，用户既看不到也没用；
/// 2. 更关键的是：`Option::expect` / `run().unwrap_or_else` 一路回到 main
///    返回 Result<(), Box<dyn Error>> 时，`unwrap` 会再 panic 一次，
///    默认钩子**第二次**输出 panic 就会 `process::abort()`（exit -1073740791）。
///    自己接管后 panic 变成一次普通 unwind，进程能走正常退出码、错误信息稳定落盘。
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "?".into());
        let text = format!("{info} @ {loc}");
        crash_log("panic", &text);
        if text.contains("Failed to setup app") {
            report_startup_failure(&text);
        }
    }));
}

/// 简单的 FNV-1a 哈希，用来给内嵌后端做版本标识（避免重复解压）。
fn short_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// 取出内嵌的后端字节。
///
/// 这里用 `include_bytes!` 而不是 Tauri resource：include_bytes 真正把字节
/// 编进 exe 本体，用户拿到的确实是「一个文件」；resource 方案在 Windows 上
/// 仍然是旁边的独立文件。
///
/// 注意：`include_bytes!` 产出的是定长数组 `&[u8; N]`，需要 `as_slice()`
/// 转成 `&[u8]` 才能匹配返回类型（之前这里报过 E0308 mismatched types）。
fn embedded_backend() -> Option<&'static [u8]> {
    // 构建脚本在 backend-exe/aja-backend.exe 存在时才定义这个常量（见 build.rs）；
    // 开发态不定义，返回 None，走 python -m app.main。
    option_env!("AJA_BACKEND_EMBEDDED")
        .map(|_| include_bytes!("../../backend-exe/aja-backend.exe").as_slice())
}

/// 把内嵌后端释放到 AppData，返回可执行文件路径。
fn extract_backend(bytes: &[u8]) -> Result<PathBuf, String> {
    let tag = short_hash(bytes);
    let dir = app_local_dir().join("backend").join(&tag);
    let exe = dir.join(BACKEND_EXE);
    if exe.is_file() {
        // 已释放过且长度一致，直接复用
        if let Ok(meta) = exe.metadata() {
            if meta.len() == bytes.len() as u64 {
                println!("[tauri] 复用已释放的后端: {}", exe.display());
                return Ok(exe);
            }
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建后端目录失败: {e}"))?;
    // 先写临时文件再改名，避免中途失败留下半个 exe
    let tmp = dir.join(format!("{BACKEND_EXE}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("写入后端失败: {e}"))?;
    if exe.exists() {
        let _ = std::fs::remove_file(&exe);
    }
    std::fs::rename(&tmp, &exe).map_err(|e| format!("重命名后端失败: {e}"))?;
    println!("[tauri] 已释放后端到 {}", exe.display());
    Ok(exe)
}

/// 启动后端，返回 (子进程, 监听端口)。
pub fn spawn_backend() -> Result<(Child, u16), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe.parent().ok_or("无法确定 exe 目录")?.to_path_buf();

    // 优先级 1：内嵌后端（单文件交付态）
    // 优先级 2：exe 同级 backend-sidecar/（本地测试打包）
    // 优先级 3：python -m app.main（开发态）
    let (program, args, cwd) = if let Some(bytes) = embedded_backend() {
        let path = extract_backend(bytes)?;
        let dir = path.parent().unwrap_or(&exe_dir).to_path_buf();
        (path, vec![], dir)
    } else {
        let sidecar = exe_dir.join("backend-sidecar").join(BACKEND_EXE);
        if sidecar.exists() {
            (sidecar, vec![], exe_dir.join("backend-sidecar"))
        } else {
            let dir = backend_dir();
            if !dir.exists() {
                return Err(format!("找不到后端目录：{}", dir.display()));
            }
            let python = std::env::var("AJA_PYTHON").unwrap_or_else(|_| "python".to_string());
            (
                PathBuf::from(python),
                vec!["-m".to_string(), "app.main".to_string()],
                dir,
            )
        }
    };

    println!("[tauri] 启动后端: {:?} in {:?}", program, cwd);

    // 打包态：数据落在 %LOCALAPPDATA%\ai-job-assistant\data，
    // 避免装进 Program Files 后无写权限（PRD 明确要求"基于可执行文件真实目录"）
    let data = if is_packaged() {
        app_local_dir().join("data")
    } else {
        data_dir()
    };
    let _ = std::fs::create_dir_all(&data);

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&cwd)
        .env("AJA_PORT", "0")
        .env("AJA_DATA_DIR", &data)
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONPATH", cwd.to_string_lossy().to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| format!("启动后端失败: {e}"))?;
    crash_log("tauri", &format!("后端已启动 pid={} {program:?}", child.id()));

    let stdout = child.stdout.take().ok_or("无法读取后端 stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut port: u16 = 0;
    let mut line = String::new();
    for _ in 0..120 {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let t = line.trim();
                println!("[backend] {t}");
                if let Some(rest) = t.strip_prefix("AJA_BACKEND_PORT=") {
                    if let Ok(p) = rest.trim().parse::<u16>() {
                        port = p;
                        break;
                    }
                }
            }
            Err(e) => return Err(format!("读取后端输出失败: {e}")),
        }
    }
    if port == 0 {
        let _ = child.kill();
        return Err("后端未能报告监听端口".into());
    }

    // 继续把后端输出转到父进程控制台
    std::thread::spawn(move || {
        let mut l = String::new();
        while reader.read_line(&mut l).unwrap_or(0) > 0 {
            print!("[backend] {l}");
            l.clear();
        }
    });
    if let Some(err) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut r = BufReader::new(err);
            let mut l = String::new();
            while r.read_line(&mut l).unwrap_or(0) > 0 {
                eprint!("[backend:err] {l}");
                l.clear();
            }
        });
    }

    // 看护：后端是个黑盒进程（PyInstaller + Chrome），一旦退出（崩溃/被杀），
    // 前端所有请求都会失败，界面看起来就是"卡住/被关掉"。
    // 这里把退出码写进 crash.log；主进程自己的 panic 由 panic hook 记录。
    // （注意：std::process::Child 没有 try_clone，主线程要保留 child 去 kill，
    //  所以只记 pid——退出码需要的话另起一个轮询 handle 的线程。）
    crash_log("backend-exit-monitor", &format!("后端 pid={} 已交给主线程看护", child.id()));

    Ok((child, port))
}

/// 组装并运行 Tauri 应用。
pub fn run() {
    install_panic_hook();
    crash_log("tauri", &format!("主进程启动 pid={}", std::process::id()));

    // ---- 必须在 Builder 之前：webview 一创建就没机会了 ----
    // 先抢单实例锁（抢不到直接退出，绝不和另一个实例抢 WebView 环境），
    // 再清理上一轮残留、把 WebView2 用户数据目录准备好。
    // 这里的 `_app` 只用来算路径，随后整个丢掉；
    // 真正跑窗口的是下面带 plugin / setup 的那个 Builder。
    let startup = (|| -> Result<std::fs::File, String> {
        let _app = tauri::Builder::default()
            .build(tauri::generate_context!())
            .map_err(|e| format!("构建 Tauri 应用失败: {e}"))?;
        prepare_webview_dir(_app.handle())
    })();
    let lock = match startup {
        Ok(lock) => lock,
        Err(e) => {
            crash_log("tauri", &format!("启动前置检查失败: {e}"));
            // 退出码 2：告诉用户/脚本「这次没起来」，与正常退出区分
            std::process::exit(2);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(BackendPort(Mutex::new(0)))
        .manage(BackendProcess(Mutex::new(None)))
        // 锁句柄交给 managed state，保证活到进程退出
        .manage(InstanceLock(Mutex::new(Some(lock))))
        .invoke_handler(tauri::generate_handler![
            commands::get_backend_port,
            commands::get_data_dir,
            commands::get_crash_log_path
        ])
        .setup(|app| {
            // 浏览器由后端拉起（Chrome 子进程挂在后端下面），
            // 任务管理器里对着 aja-backend.exe「结束任务」会把浏览器一起带走，
            // 两边状态对不上就只能重启程序——所以禁止多开。
            // 单实例锁在进入本函数之前就抢好了（见 prepare_webview_dir）。
            match spawn_backend() {
                Ok((child, port)) => {
                    crash_log("tauri", &format!("后端已就绪，端口 {port}"));
                    *app.state::<BackendPort>().0.lock().unwrap() = port;
                    *app.state::<BackendProcess>().0.lock().unwrap() = Some(child);
                }
                Err(e) => {
                    crash_log("tauri", &format!("后端启动失败: {e}"));
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(state) = window.try_state::<BackendProcess>() {
                    if let Some(mut child) = state.0.lock().unwrap().take() {
                        let _ = child.kill();
                        let _ = child.wait();
                        println!("[tauri] 后端进程已清理");
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            crash_log("tauri", &format!("Tauri 运行失败: {e}"));
            std::process::exit(1);
        });
}

/// 在创建 WebView2 **之前**：清掉上一轮崩溃残留的 webview 进程，
/// 并预先把 WebView2 的 user data 目录建好（顺带把系统分配的
/// `EBWebView` 目录固定到我们自己算出来的路径上，见下）。
///
/// 症状：把 exe 复制到别的文件夹（或上一轮被强杀后）双击，窗口一闪就没了，
/// 什么都没留下。实测 panic 内容就是
///   failed to create webview: WebView2 error: WindowsError(HRESULT 0x800700AA)
///   "请求的资源在使用中。"
/// —— 上一个 ai-job-assistant.exe 被以 `ExitProcess` 之外的方式结束
/// （panic abort / 任务管理器结束 / 上一个实例还在后台），它拉起的一整套
/// `msedgewebview2.exe` 会变成孤儿进程继续持有 WebView2 的 user data 目录，
/// 新的 WebView2 环境就起不来。
///
/// 为什么锁要先抢：两个实例同时启动时会**都**去删对方的 WebView 环境，
/// 反而制造出上面那种占用。所以这里先抢锁，抢不到就立刻退出（退出码 2），
/// 抢到了才动手清理。
///
/// 系统自带 `taskkill`，不引额外依赖。
fn prepare_webview_dir(app: &tauri::AppHandle) -> Result<std::fs::File, String> {
    let lock = acquire_instance_lock(app)?;
    // 上面只保证「同一时刻只有一个实例」。把 exe 复制到别的文件夹后反复双击时，
    // 上一个实例可能还活着但被我们当成「已退出」——这里必须主动确认。
    // 否则两个进程会争用同一个 WebView2 环境，报
    //   WebView2 error: HRESULT(0x800700AA) "请求的资源在使用中。"
    // 然后窗口一闪就退。属于防御性清理：结束不掉就记日志，不阻塞正常启动。
    kill_pids_under(&app_local_dir());
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    match Command::new("taskkill")
        .args(["/F", "/IM", "msedgewebview2.exe"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
    {
        Ok(st) if st.success() => crash_log("tauri", "已清理残留的 msedgewebview2.exe"),
        Ok(_) => {} // 本来就没有残留，正常情况
        Err(e) => crash_log("tauri", &format!("清理 webview 残留失败: {e}")),
    }
    match app.path().app_local_data_dir() {
        Ok(dir) => {
            let ud = dir.join("EBWebView");
            match std::fs::create_dir_all(&ud) {
                Ok(()) => crash_log("tauri", &format!("WebView2 用户数据目录: {}", ud.display())),
                Err(e) => crash_log("tauri", &format!("创建 {} 失败: {e}", ud.display())),
            }
        }
        Err(e) => crash_log("tauri", &format!("无法确定 app_local_data_dir: {e}")),
    }
    Ok(lock)
}

/// 结束所有「可执行文件位于 `dir` 之下」的进程（`dir` 是 `%LOCALAPPDATA%\ai-job-assistant`）。
///
/// 为什么要按完整路径而不是按镜像名：`taskkill /IM ai-job-assistant.exe` 会连带
/// 杀掉用户从项目 target/ 里打开的调试实例；而只杀「释放出来的那份」才是安全的。
/// 用的是系统自带的 `powershell`，不新增依赖；拿不到进程列表就静默跳过。
#[cfg(windows)]
fn kill_pids_under(dir: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut marker = dir.to_string_lossy().to_lowercase();
    marker = marker
        .replace("'", "''")
        .replace("\\\\?\\", "")
        .replace('/', "\\");
    let script = format!(
        "$m='{marker}'; Get-Process -ErrorAction SilentlyContinue | \
         Where-Object {{ $_.Path -and $_.Id -ne {me} -and $_.Path.ToLower().StartsWith($m) }} | \
         ForEach-Object {{ $_.Kill(); $_.Id }}",
        me = std::process::id()
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) => {
            let killed: Vec<_> = String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                .map(|s| s.to_string())
                .collect();
            if !killed.is_empty() {
                crash_log("tauri", &format!("已结束残留进程 pid={}", killed.join(",")));
            }
        }
        Err(e) => crash_log("tauri", &format!("残留进程检查失败: {e}")),
    }
}

#[cfg(not(windows))]
fn kill_pids_under(_dir: &std::path::Path) {}

/// 记录「为什么没起来」，并给出用户能自己处理的下一步。
///
/// 典型场景是 WebView2 创建失败（`HRESULT 0x800700AA 请求的资源在使用中`）：
/// 用户双击后窗口一闪而过，任务管理器里还残留着上一轮的
/// `msedgewebview2.exe` 子进程。这时程序必须把原因写进 crash.log，
/// 而不是像以前那样 panic 后随进程一起消失。
pub fn report_startup_failure(text: &str) {
    let hint = if text.contains("800700AA") || text.contains("webview") {
        "WebView2 创建失败：通常是上一轮残留的 msedgewebview2.exe 进程还占着用户数据目录。\n\
         处理（按顺序试）：\n\
         1. 任务管理器 → 详细信息 → 结束所有 msedgewebview2.exe / ai-job-assistant.exe，再双击；\n\
         2. 仍不行就把 %LOCALAPPDATA%\\com.ai.jobassistant\\EBWebView 整个删掉再双击；\n\
         3. 再不行把这份 crash.log 最后几行发给开发者。"
    } else {
        "启动失败，请把本文件最后几行反馈给我。"
    };
    crash_log("fatal", &format!("{text}\n{hint}"));
}

/// 单实例锁的持有者：把 File 放进 Tauri 的 managed state，避免 setup 结束就释放。
#[allow(dead_code)]
struct InstanceLock(Mutex<Option<std::fs::File>>);

/// 单实例锁：独占打开 lock 文件，进程退出时系统自动释放。
///
/// **位置必须与 exe 无关**：早先用的 `app_local_data_dir()`（= `%LOCALAPPDATA%\<identifier>`）
/// 在 Tauri 里实际是 `%APPDATA%\<identifier>`。只要这个路径哪天算不出来、或落到了
/// 另一个 URL（比如把 exe 复制到别的目录后 WebView2 用 file:// 起页面，
/// app_local_data_dir 就变成 `...\Local\file_127.0.0.1`），第二份进程就再也
/// 抢不到锁——于是又拉起一个后端、再开一个浏览器窗口。这里显式钉死
/// `%LOCALAPPDATA%\ai-job-assistant`，和 main.rs 释放后端用的目录保持一致。
#[cfg(windows)]
fn single_instance_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .parent()
                .map(|d| d.join("Local"))
                .unwrap_or_else(std::env::temp_dir)
        })
        .join("ai-job-assistant")
        .join("app.lock")
}

#[cfg(not(windows))]
fn single_instance_path() -> PathBuf {
    std::env::temp_dir().join("ai-job-assistant.lock")
}

/// 打开（或自愈重建）单实例锁。成功返回持有者 File，失败返回给用户看的原因。
fn acquire_instance_lock(app: &tauri::AppHandle) -> Result<std::fs::File, String> {
    let path = single_instance_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建锁目录 {} 失败：{e}", dir.display()))?;
    }
    // 先尽量做个位置自检：锁目录算不出用户名时 app_local_data_dir 会退化成
    // `...\file_127.0.0.1` 这类按 URL 命名的目录，不同启动方式（file:// vs http://）
    // 会算出不同结果，锁就形同虚设
    if let Ok(dir) = app.path().app_local_data_dir() {
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            if name.contains("file_") || name.contains("127.0.0.1") {
                crash_log("tauri", &format!("注意：app_local_data_dir 异常（{}），已改用固定锁路径", dir.display()));
            }
        }
    }
    match single_instance(&path) {
        Ok(lock) => Ok(lock),
        Err(e) => {
            // 独占打开失败 → 很可能另一个实例正在跑。
            // 但如果文件被一个已经死掉的进程留下的僵尸句柄占着（或被占用），
            // 用户就会永远双击无反应。所以这里带重试地删掉重建一次。
            let mut last = e;
            let mut stale_fixed = false;
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(300));
                if std::fs::remove_file(&path).is_ok() {
                    stale_fixed = true;
                    // 删得掉 → 说明没有活着的进程持着它（Windows 上独占句柄时
                    // 删除会直接失败），这是上次崩溃留下的陈旧锁，重建即可
                    match single_instance(&path) {
                        Ok(lock) => return Ok(lock),
                        Err(e2) => last = e2,
                    }
                }
            }
            if stale_fixed {
                Err(format!("锁文件 {} 重建后仍无法独占：{last}", path.display()))
            } else {
                Err(format!(
                    "另一个实例正在运行（锁 {} 被占用：{last}）",
                    path.display()
                ))
            }
        }
    }
}

#[cfg(windows)]
fn single_instance(lock: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0) // 不共享：第二个实例打开会失败
        .open(lock)
        .map_err(|e| format!("独占打开失败：{e}"))
}

#[cfg(not(windows))]
fn single_instance(lock: &std::path::Path) -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(lock)
        .map_err(|e| format!("打开失败：{e}"))
}
