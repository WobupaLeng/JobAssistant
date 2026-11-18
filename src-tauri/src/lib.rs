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

pub use commands::{get_backend_port, get_data_dir};

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

    Ok((child, port))
}

/// 组装并运行 Tauri 应用。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(BackendPort(Mutex::new(0)))
        .manage(BackendProcess(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![commands::get_backend_port, commands::get_data_dir])
        .setup(|app| {
            match spawn_backend() {
                Ok((child, port)) => {
                    println!("[tauri] 后端已就绪，端口 {port}");
                    *app.state::<BackendPort>().0.lock().unwrap() = port;
                    *app.state::<BackendProcess>().0.lock().unwrap() = Some(child);
                }
                Err(e) => {
                    eprintln!("[tauri] 后端启动失败: {e}");
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
        .expect("Tauri 应用启动失败");
}
