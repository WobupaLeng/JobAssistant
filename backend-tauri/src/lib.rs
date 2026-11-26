//! Tauri 壳：窗口、毛玻璃、命令注册。
//!
//! 目录约定：
//! - [`core`] 是业务核心（领域规则 / 存储 / sidecar 协议），**不得依赖 `tauri`**；
//! - `commands_*` 只做「收参数 → 调 core → 错误转字符串」，不含业务规则。
//!
//! 分层靠模块边界与 code review 维持，不再有独立 crate 的编译期强制。
//! 改动 `core/` 时仍然禁止 `use tauri::`——那是 `commands_*` 的职责。

use crate::core::scoring::{SCORE_MAX, SCORE_MIN_DEFAULT};
use serde::Serialize;

pub mod core;

mod commands_db;
mod commands_chat;
mod commands_greet;
mod commands_company;
mod commands_scrape;
mod commands_auto;
mod commands_config;
mod commands_logs;
mod commands_sensitive;

pub use commands_db::{LlmState, StoreState};
pub use commands_scrape::ScrapeState;

/// 打分口径：通过本命令下发给前端，**前端禁止硬编码**这两个值。
///
/// SPEC-002 第 3.2 节：同一常量曾散落在 5 处（Python 2 处 + TS 3 处），
/// 漏改任一处都会产生"门槛 60 分却筛出 6 分岗位"的静默错误。
#[derive(Serialize)]
struct ScoringSpec {
    score_max: u8,
    score_min_default: u8,
}

#[tauri::command]
fn get_scoring_spec() -> ScoringSpec {
    ScoringSpec {
        score_max: SCORE_MAX,
        score_min_default: SCORE_MIN_DEFAULT,
    }
}

/// 骨架自检：返回构建信息，用于确认前端↔Rust 桥接是否打通。
///
/// `data_dir` 是**配置页「数据目录」显示用的绝对路径**：由 `AppPaths` 状态
/// 在 setup 阶段填好，命令层只读。让 UI 自己拼路径是不行的——开发态与打包态
/// 的 `identifier` 不同，拼错时用户看到的是一份空库的目录。
#[tauri::command]
fn app_info(paths: tauri::State<'_, commands_config::AppPaths>) -> serde_json::Value {
    serde_json::json!({
        "name": "AI 求职助手",
        "version": env!("CARGO_PKG_VERSION"),
        "shell": "rust",
        "stage": "skeleton",
        "data_dir": paths.data_dir_text(),
    })
}

/// 无边框窗口的最小化 / 最大化切换 / 关闭。前端只经 invoke 调用（红线 1）。
#[tauri::command]
fn window_minimize(window: tauri::WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_toggle_maximize(window: tauri::WebviewWindow) -> Result<(), String> {
    let max = window.is_maximized().map_err(|e| e.to_string())?;
    if max { window.unmaximize() } else { window.maximize() }.map_err(|e| e.to_string())
}

#[tauri::command]
fn window_close(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

/// 主题切换后按新主题重刷窗口模糊底色。
///
/// 只有 accent 链会带 tint；Haze 的 `HostBackdropBrush` 是纯模糊、不叠色，
/// 挂上时什么都不做——重刷 accent 会把 `HostBackdropBrush` 覆盖掉。
fn refresh_window_tint(window: &tauri::WebviewWindow, theme: &str) {
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager as _;
        if let Some(st) = window.app_handle().try_state::<HazeState>() {
            let attached = st.0.lock().map(|g| g.is_some()).unwrap_or(false);
            if attached {
                return;
            }
        }
    }
    apply_blur(window, theme);
}

/// 前端挂载与切主题时调用：让窗口模糊底色跟 `data-theme` 走。
///
/// 深色档是深蓝黑；浅色主题若不换成近白档，整窗会被压成灰蓝（「浅色不够透亮」）。
#[tauri::command]
fn set_window_theme(window: tauri::WebviewWindow, theme: String) -> Result<(), String> {
    refresh_window_tint(&window, theme.as_str());
    Ok(())
}

/// 前端确认「这个登录态我消费了」。
///
/// 谁的确认：`platform` + `state` 必须与当前这一轮广播完全一致才算数，
/// 晚到的旧确认直接丢弃，不会把新一轮广播提前掐掉。
/// 全员确认后 watcher 停止补发；在此之前每 5 秒重发一次，最多 3 次。
#[tauri::command]
fn login_ack(platform: String, state: String) -> Result<usize, String> {
    Ok(handle_ack(&platform, &state))
}

/// 应用毛玻璃效果。
///
/// **为什么不用 `window_vibrancy::apply_acrylic`**：
/// `window-vibrancy-0.8.1/src/windows.rs:62` 在 Windows 11 上只调
/// `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW)`，
/// **完全不碰 `ACCENT_POLICY`，且把 `color` 参数丢掉**。该后景类型要客户区透明才可见，
/// 而 wry 的 `set_background_color` 在运行期会把 `a != 0` 的 alpha 强制改成 255
/// （`wry-0.57.3/src/webview2/mod.rs:2016`），WebView2 客户区永远不透明。
/// 结果就是「`apply_acrylic` 返回 Ok、`apply_mica` 也返回 Ok、屏幕上却什么都没有」。
///
/// 该函数未文档化，`windows-sys` 不导出，所以按 `window-vibrancy` 的同样做法用
/// `GetProcAddress` 取指针。降级链：系统亚克力 → Win11 亚克力材质 → 纯色。
/// **任何一步失败都不报错**，绝不能白屏或透明穿透（SPEC-002 V5）。
fn apply_blur(window: &tauri::WebviewWindow, theme: &str) {
    if apply_system_glass(window, theme) {
        return;
    }
    eprintln!("[window] 系统玻璃不可用，降级为纯色背景");
}

/// 持有 Haze 句柄，防止挂载后的合成器对象被提前释放。
#[cfg(target_os = "windows")]
#[allow(dead_code)]
struct HazeState(std::sync::Mutex<Option<haze::Haze>>);

/// 在窗口内容下方挂载 HostBackdropBrush 圆角模糊。
///
/// 必须在拥有窗口的线程调用；`Haze` 自己跟随窗口尺寸与 DPI。
/// 返回 `false` 表示系统不支持或挂载失败，调用方回退到旧的 accent 链。
#[cfg(target_os = "windows")]
fn attach_haze(window: &tauri::WebviewWindow) -> bool {
    use tauri::Manager as _;

    if !haze::Haze::supported() {
        eprintln!("[window] Haze 不支持当前 Windows，回退 accent");
        return false;
    }
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    // 只做纯模糊；底色仍由 CSS 的 --shell-tint 控制，避免系统 tint 与主题叠色。
    let style = haze::Style::dark()
        .radius(8.0)
        .strength(20.0)
        .tint(0.0);
    match haze::Haze::attach_hwnd(hwnd.0, style) {
        Ok(handle) => {
            window
                .app_handle()
                .manage(HazeState(std::sync::Mutex::new(Some(handle))));
            eprintln!("[window] Haze HostBackdropBrush 已挂载");
            true
        }
        Err(e) => {
            eprintln!("[window] Haze 挂载失败，回退 accent: {e}");
            false
        }
    }
}

/// 亚克力底色：深蓝黑，alpha 越低桌面越透。`SetWindowCompositionAttribute` 的
/// `GradientColor` 是 **AABBGGRR**（不是 RGBA），所以这里按字节序拼。
const ACRYLIC_TINT: (u8, u8, u8, u8) = (18, 24, 38, 125);

/// 给高斯模糊的底色。`ACCENT_ENABLE_BLURBEHIND` 在 Win11 上做**纯模糊、不加色调**
/// （Win10 才是白雾），所以底色交给它的 `TintColor`：alpha 越低桌面越透。
const BLUR_TINT: (u8, u8, u8, u8) = (12, 16, 26, 115);

/// 浅色主题的亚克力底色：近白，透明度与深色档一致。
///
/// 深色档是深蓝黑，浅色主题下会把整窗压成灰蓝——用户观感就是「浅色不够透亮」。
const ACRYLIC_TINT_LIGHT: (u8, u8, u8, u8) = (250, 252, 255, 118);

/// 浅色主题的高斯模糊底色：近白。浅底透上来的是桌面亮部，白 tint 只提亮不加灰。
const BLUR_TINT_LIGHT: (u8, u8, u8, u8) = (255, 255, 255, 108);

/// 按主题挑两侧的窗口底色。未知主题一律按深色处理（深色是默认）。
fn tints_for(theme: &str) -> ((u8, u8, u8, u8), (u8, u8, u8, u8)) {
    if theme == "light" {
        (BLUR_TINT_LIGHT, ACRYLIC_TINT_LIGHT)
    } else {
        (BLUR_TINT, ACRYLIC_TINT)
    }
}

/// 开系统「磨砂玻璃」。
///
/// 顺序有讲究：
/// 1. 两种 accent 都受 Windows「个性化 → 颜色 → 透明效果」开关约束；关闭时
///    `SetWindowCompositionAttribute` 仍返回成功，但屏幕上没有模糊。不能把「返回成功」
///    当成「已经出效果」。
/// 2. `ACCENT_ENABLE_BLURBEHIND` 是 Win7 起就有的高斯模糊，不依赖透明客户区；
///    Win11 去掉了 Win10 的白雾，并**自动叠加系统设置里的「透明效果」色**。
/// 3. 都不行时用 Win11 的 `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW`（亚克力材质）
///    兜底；它只保证非客户区，属于最后手段。
///
/// 返回 `false` 表示三条都不成立，**不是错误**（调用方降级为纯色）。
fn apply_system_glass(window: &tauri::WebviewWindow, theme: &str) -> bool {
    let (blur_tint, acrylic_tint) = tints_for(theme);
    if apply_accent_effect(window, ACCENT_ENABLE_BLURBEHIND, blur_tint) {
        return true;
    }
    if apply_accent_effect(window, ACCENT_ENABLE_ACRYLICBLURBEHIND, acrylic_tint) {
        return true;
    }
    apply_material_backdrop(window)
}

/// `ACCENT_ENABLE_ACRYLICBLURBEHIND`：Win10 1803+ 的亚克力。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;
/// `ACCENT_ENABLE_BLURBEHIND`：Win7+ 的高斯模糊，Win11 上是无色调的真模糊。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
/// `WCA_ACCENT_POLICY`：唯一的 composition attribute。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const WCA_ACCENT_POLICY: u32 = 19;

/// `DWMWA_SYSTEMBACKDROP_TYPE`（Win11 22H2+）与亚克力材质的取值。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const DWMSBT_TRANSIENTWINDOW: i32 = 3;

/// `DWMWA_WINDOW_CORNER_PREFERENCE` 的属性号（Win11 22H2+）。
/// 取值常量在 `apply_round_corners` 旁边，那里是唯一消费点。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;

/// 用未文档化的 `SetWindowCompositionAttribute` 写入 `WCA_ACCENT_POLICY`。
/// `accent_state` 取 `ACCENT_ENABLE_BLURBEHIND` 或 `ACCENT_ENABLE_ACRYLICBLURBEHIND`。
///
/// 返回 `false` 表示「本机没有这个 API 或调用失败」，交给调用方走降级链，**不是错误**。
fn apply_accent_effect(
    window: &tauri::WebviewWindow,
    accent_state: u32,
    tint: (u8, u8, u8, u8),
) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
        #[repr(C)]
        struct AccentPolicy {
            accent_state: u32,
            accent_flags: u32,
            gradient_color: u32,
            animation_id: u32,
        }

        #[repr(C)]
        struct WindowCompositionAttribData {
            attribute: u32,
            data: *mut std::ffi::c_void,
            size: usize,
        }

        type SetWindowCompositionAttributeFn =
            unsafe extern "system" fn(*mut std::ffi::c_void, *mut WindowCompositionAttribData) -> i32;

        let Ok(hwnd) = window.hwnd() else {
            return false;
        };
        let raw = hwnd.0;

        let user32 = unsafe { LoadLibraryA(c"user32.dll".as_ptr() as *const u8) };
        if user32.is_null() {
            return false;
        }
        let proc = unsafe {
            GetProcAddress(user32, c"SetWindowCompositionAttribute".as_ptr() as *const u8)
        };
        let Some(proc) = proc else {
            return false;
        };
        let set_attr: SetWindowCompositionAttributeFn = unsafe { std::mem::transmute(proc) };

        let (r, g, b, a) = tint;
        // GradientColor 是 AABBGGRR
        let gradient = (u32::from(a) << 24)
            | (u32::from(b) << 16)
            | (u32::from(g) << 8)
            | u32::from(r);
        let mut policy = AccentPolicy {
            accent_state,
            // 2 = 在窗口边框上绘制，避免圆角处出现硬边
            accent_flags: 2,
            gradient_color: gradient,
            animation_id: 0,
        };
        let mut data = WindowCompositionAttribData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut policy as *mut AccentPolicy as *mut std::ffi::c_void,
            size: std::mem::size_of::<AccentPolicy>(),
        };
        let ok = unsafe { set_attr(raw, &mut data) } != 0;
        ok
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        let _ = accent_state;
        let _ = tint;
        false
    }
}

/// Win11 的 `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW`（亚克力材质）。
/// 只保证非客户区可见，作最后兜底。
fn apply_material_backdrop(window: &tauri::WebviewWindow) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

        let Ok(hwnd) = window.hwnd() else {
            return false;
        };
        let value = DWMSBT_TRANSIENTWINDOW;
        let hr = unsafe {
            DwmSetWindowAttribute(
                hwnd.0,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &value as *const i32 as *const std::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            )
        };
        hr == 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        false
    }
}

/// `DWMWA_WINDOW_CORNER_PREFERENCE` 的取值：直角 / 系统圆角。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const DWMWCP_ROUND: i32 = 2;

/// `DWMWA_WINDOW_CORNER_PREFERENCE`（Win11 22H2+）：本窗口四角交给 DWM 画圆角。
///
/// **为什么必须让系统画**：窗口是 `decorations: false` + `transparent: true`，WebView
/// 客户区是透明矩形，桌面会从矩形的四个角透进来。前端 `.app-shell` 的 `border-radius`
/// 只裁它自己那一层，裁不到窗口矩形 —— 结果是「弧线画出来了，弧外仍是一块灰色方角」。
/// 能裁窗口外形的只有 DWM，所以这里必须交给它（用户 m03488 的角点截图）。
///
/// 弧径是 Win11 的固定值 8px。前端 `--shell-radius` 取同一个值并按系统弧内缩，
/// 两层弧才同心；外壳的 16px 内部圆角留给卡片，不与窗口外形争。
///
/// 调用失败（Win10 没有这个属性）返回非 0，忽略即可：Win10 本来就不给无边框窗口画弧。
fn apply_round_corners(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let pref = DWMWCP_ROUND;
        let _ = unsafe {
            DwmSetWindowAttribute(
                hwnd.0,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &pref as *const i32 as *const std::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            )
        };
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
    }
}

/// 验证脚本用的临时诊断开关：设 `AJA_DIAG_WINDOW=1` 时启动即打印窗口样式位与特效调用结果。
/// 它不改变任何业务行为，只输出读数，所以可以长期留在代码里。
fn window_diag(window: &tauri::WebviewWindow, glass_attached: bool) {
    #[cfg(target_os = "windows")]
    {
        if std::env::var("AJA_DIAG_WINDOW").is_err() {
            return;
        }
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, GWL_EXSTYLE, GWL_STYLE,
        };
        let Ok(hwnd) = window.hwnd() else {
            eprintln!("[diag] 取不到 hwnd");
            return;
        };
        let raw = hwnd.0;
        unsafe {
            let style = GetWindowLongPtrW(raw, GWL_STYLE);
            let ex = GetWindowLongPtrW(raw, GWL_EXSTYLE);
            eprintln!(
                "[diag] hwnd={raw:?} style=0x{style:08X} exstyle=0x{ex:08X} layered={}",
                ex & 0x0008_0000 != 0, // WS_EX_LAYERED，仅作诊断记录
            );
        }
        // 这里只读样式位，绝不能再调 accent / Mica：那会把 Haze 的
        // ACCENT_ENABLE_HOSTBACKDROP 覆盖掉，诊断本身反而毁掉效果。
        eprintln!(
            "[diag] glass = {}",
            if glass_attached { "haze" } else { "legacy/fallback" }
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        let _ = glass_attached;
    }
}

/// 在 WebView 创建**之前**准备 WebView2 环境。
///
/// 三步缺一不可：① 抢单实例锁 → ② 清理上一轮残留的 WebView2 子进程 → ③ 校验数据目录可写。
///
/// **第 ③ 步的报文必须是可执行的**：2026-11-21 定位交接清单第 1 项时实证，
/// WebView2 环境创建失败的两种报文——`os error 5 拒绝访问` 与
/// `HRESULT(0x800700AA) 请求的资源在使用中`——都可能只是「当前进程是 Low 完整性级别」
/// 的转述，二者与真实病因毫无字面关系。详见 [`not_writable_hint`]。
fn prepare_webview_dir(app: &tauri::AppHandle) -> Result<InstanceLock, String> {
    use tauri::Manager;

    let lock = acquire_instance_lock(app)?;

    // 只预建**父目录**（app_local_data_dir），`EBWebView` 必须留给 WebView2 运行时
    // 自己创建并初始化。实测教训：若我们先建出空的 `EBWebView` 目录，
    // WebView2 检测到目录存在却非它初始化，启动会退化为
    //   failed to create webview: WindowsError(0x800700AA) 请求的资源在使用中
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("无法解析应用数据目录: {e}"))?;
    let created = std::fs::create_dir_all(&dir);

    // 清理上一轮残留：只杀「命令行里带本项目数据目录」的 WebView2 子进程。
    // 旧实现是 `taskkill /F /IM msedgewebview2.exe`，会连 Windows Widgets 的
    // WebView2 一起杀掉（实测本机常驻的 6~12 个都属于 MicrosoftWindows.Client.CBS）。
    kill_stale_webviews(&dir);

    // 可写性探针必须放在 WebView2 之前：环境的创建失败报文无法反推病因，
    // 而一次 `File::create` 就能把「Low 完整性级别」当场抓住。
    if let Err(e) = probe_writable(&dir) {
        return Err(not_writable_hint(&dir, &e.to_string(), created.err()));
    }
    Ok(lock)
}

/// 数据目录可写性探针：建一个哨兵文件再删掉。
///
/// 只依赖 `std::fs`，不引入 `windows-sys`（`Cargo.toml` 属架构单点，编码模式不改）。
/// 探针文件以 pid 命名，避免与并行运行的另一个实例互相踩。
fn probe_writable(dir: &std::path::Path) -> std::io::Result<()> {
    let probe = dir.join(format!(".aja-write-probe-{}", std::process::id()));
    std::fs::File::create(&probe)?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// 数据目录不可写时的中文诊断。
///
/// 根因（2026-11-21 实证，交接清单第 1 项）：Windows 会把可执行文件带
/// `Mandatory Label\Low Mandatory Level:(I)(NW)` 的进程降为 **Low 完整性级别**
/// （`S-1-16-4096`）。Low 进程禁止向上写（no-write-up），于是：
/// - 建不了 `%LOCALAPPDATA%` 下的目录 → WebView2 归一化成 `os error 5 拒绝访问`；
/// - 开不了 WebView2 环境 → `HRESULT(0x800700AA) 请求的资源在使用中`。
///
/// 实证手法：把同一个 `whoami.exe` 分别拷到工作区内、外，`whoami /groups` 分别报
/// `S-1-16-4096` 与 `S-1-16-8192`；把同一个 release exe 从工作区内换到工作区外，
/// 启动结果从 100% 失败变成 100% 成功。故这是**运行位置**问题，不是构建档位、
/// 不是目录 ACL、也不是 WebView2 残留（三者均已逐个排除）。
fn not_writable_hint(
    dir: &std::path::Path,
    err: &str,
    create_err: Option<std::io::Error>,
) -> String {
    let mut msg = format!(
        "WebView2 数据目录不可写：{}（{err}）。\n\
         这几乎总是「进程被降为 Low 完整性级别（S-1-16-4096）」而非权限配置问题：\n\
         exe 所在目录若带 `Mandatory Label\\Low Mandatory Level:(NW)` 标签\
         （例如 DSH/沙箱工作区），Windows 就会把该进程降为 Low IL，Low 进程不能向上写，\n\
         WebView2 于是只会吐出 `os error 5 拒绝访问` 或 `0x800700AA 请求的资源在使用中` 这类误导报文。\n\
         处置：把构建产物或 CARGO_TARGET_DIR 放到工作区外再运行，例如\n\
           $env:CARGO_TARGET_DIR=\"D:\\jt\"; cargo build --release\n\
           然后运行 \"D:\\jt\\release\\jobassistant-app.exe\"。",
        dir.display()
    );
    if let Some(e) = create_err {
        msg.push_str(&format!("\n预建目录同时失败：{e}"));
    }
    msg
}

/// 只杀掉命令行里带本项目数据目录的 WebView2 子进程。
///
/// 不清残留的风险：上一轮实例的 `msedgewebview2.exe` 仍持有 `EBWebView`，
/// 新实例会撞上 `0x800700AA`。清理失败不阻断启动——最坏是这次打不开窗口，但至少有日志。
#[cfg(windows)]
fn kill_stale_webviews(udf: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // 单引号是 PowerShell 的字面量字符串，路径里的反斜杠无需转义，只需把 ' 双写。
    let needle = udf.display().to_string().replace('\'', "''");
    let script = format!(
        "Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | \
         Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains('{needle}') }} | \
         ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}"
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

#[cfg(not(windows))]
fn kill_stale_webviews(_udf: &std::path::Path) {}

/// 单实例锁。
///
/// 实现说明：这里用 **Windows 命名互斥体**，而不是旧应用的锁文件方案。
/// 原因是实测锁文件在本机 `%LOCALAPPDATA%` 下不可用——即便目录本身可写，
/// 以 `share_mode(0)` 排他打开 `instance.lock` 仍返回 `os error 5`，
/// 会把「另一个实例在跑」和「权限不足」混成同一个错误，误导排查。
/// 命名互斥体由 OS 内核维护、随进程退出自动释放，不受文件 ACL 影响。
struct InstanceLock(#[allow(dead_code)] Option<()>);

#[cfg(windows)]
fn acquire_instance_lock(_app: &tauri::AppHandle) -> Result<InstanceLock, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    // 必须写 `::core::ffi`（绝对路径）。本 crate 有一个顶层 `core` 模块
    // （业务核心），裸写 `core::ffi` 会被解析到它、报 `cannot find \`ffi\` in \`core\``。
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            lp_mutex_attributes: *mut ::core::ffi::c_void,
            b_initial_owner: i32,
            lp_name: *const u16,
        ) -> *mut ::core::ffi::c_void;
        fn GetLastError() -> u32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;
    const ERROR_ACCESS_DENIED: u32 = 5;

    // 名字带应用标识，避免与旧应用的互斥体冲突（两者可并行运行做对比）。
    let name: Vec<u16> = OsStr::new("Local\\com.ai.jobassistant.rust.single")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr()) };
    if handle.is_null() {
        // 这两种失败在旧报文里都只显示「GetLastError=5/183」，没有任何可行动信息。
        let code = unsafe { GetLastError() };
        return Err(match code {
            ERROR_ACCESS_DENIED => "另一个实例正以更高权限运行（CreateMutexW 返回 ERROR_ACCESS_DENIED，\
                                    这是完整性级别不匹配，不是「没在跑」）。\
                                    请先结束它：taskkill /F /IM jobassistant-app.exe"
                .to_string(),
            _ => format!("创建单实例互斥体失败，GetLastError={code}"),
        });
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Err("另一个实例已在运行".to_string());
    }
    // 句柄故意泄漏到进程退出：Windows 会在进程结束时回收，这正是我们要的语义。
    Ok(InstanceLock(Some(())))
}

#[cfg(not(windows))]
fn acquire_instance_lock(_app: &tauri::AppHandle) -> Result<InstanceLock, String> {
    Ok(InstanceLock(Some(())))
}

/// 登录态快照：`login-state` 事件的去重键。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LoginSnapshot {
    platform: String,
    state: String,
}

/// 广播的重发间隔与次数上限（用户 m02416：5 秒一次、最大三次）。
///
/// 为什么需要重发：事件是 fire-and-forget 的，WebView 没在听（未挂载 / 后台节流 /
/// 正在重建）时那一拍就白丢了，界面会停在旧快照上。只重发**状态变化后的那一次**，
/// 不重发「一直没变」的稳态，所以不会变成常驻轮询。
pub(crate) const LOGIN_REBROADCAST_MS: u64 = 5_000;
/// 最多发几次（首次 1 次 + 重发 2 次 = 3）。
pub(crate) const LOGIN_BROADCAST_MAX: u32 = 3;

/// 一次待播广播：载荷 + 已发次数 + 下一次该发的时刻。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingBroadcast {
    snap: LoginSnapshot,
    /// 已经发出去几次（含首次）。
    sent: u32,
    /// 下一次重发的最早时刻（毫秒，单调时钟换算后的绝对值）。
    due_ms: u64,
}

/// 前端确认消费（`login_ack` 命令）后要汇报的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AckOutcome {
    /// 还有成员没确认，继续按 5 秒重发。
    Pending,
    /// 全员确认 / 已达上限，这一轮广播结束。
    Done,
}

/// 记一笔「某个窗口确认消费了这个登录态」。
///
/// 只有**同一次**广播（platform + state 都相同）的确认才算数：晚到的旧确认
/// 不能把新一轮广播提前掐掉。
fn apply_ack(
    pending: &mut Option<PendingBroadcast>,
    acked: &mut std::collections::HashSet<String>,
    platform: &str,
    state: &str,
) -> AckOutcome {
    let Some(p) = pending.as_ref() else {
        return AckOutcome::Done;
    };
    if p.snap.platform != platform || p.snap.state != state {
        return AckOutcome::Done;
    }
    acked.insert(platform.to_string());
    AckOutcome::Pending
}

/// 广播 + 确认的共享状态：watcher 线程按 5 秒重发，`login_ack` 命令在这里记账。
///
/// 字段全部由一个 `Mutex` 保护：读改写都是短临界区，不会和抓取/发消息争锁。
#[derive(Debug, Default)]
struct LoginBroadcast {
    /// 当前这一轮待播广播（`None` = 没有正在重发的状态）。
    pending: Option<PendingBroadcast>,
    /// 这一轮已经确认消费的成员。
    acked: std::collections::HashSet<String>,
}

impl LoginBroadcast {
    /// 开始新一轮：状态变化时调用，`sent` 记 1（首次已发）。
    fn begin(&mut self, snap: LoginSnapshot) {
        // 首次已经发出去，但**还没人确认**：`acked` 必须清空。
        // 曾在这里预置 `acked_platform(&snap)`，于是 `handle_ack` 拿到确认时
        // `waiting == member_count() - 1 == 0`，立刻 `finish()` —— 每一轮都被当成
        // 「全员已消费」，5 秒补发从未发生，ack 形同虚设（用户 2026-11-25：
        // 「boss 列表 ack 失效了」）。「发出去」与「被消费」是两件事，不能在这里合并。
        self.acked.clear();
        self.pending = Some(PendingBroadcast {
            snap,
            sent: 1,
            due_ms: now_ms() + LOGIN_REBROADCAST_MS,
        });
    }

    /// 是否该补发：有任务、没到上限、且到了时间点。
    fn due(&self) -> bool {
        match self.pending.as_ref() {
            Some(p) => p.sent < LOGIN_BROADCAST_MAX && now_ms() >= p.due_ms,
            None => false,
        }
    }

    /// 这一次 emit 放不放行（**首次广播与补发共用**，watcher 每拍只问它一次）。
    ///
    /// 与 `due()` 同源，差别只在首次：`begin()` 刚把 `due_ms` 拨到 5 秒后，
    /// 若用 `due()` 判断首次会被挡掉。`sent == 1` 正是「首次还没发」的标记。
    fn due_now(&self) -> bool {
        match self.pending.as_ref() {
            Some(p) => p.sent <= 1 || self.due(),
            None => false,
        }
    }

    /// 补发成功后推进计数与下次时刻。
    fn mark_sent(&mut self) {
        if let Some(p) = self.pending.as_mut() {
            p.sent += 1;
            p.due_ms = now_ms() + LOGIN_REBROADCAST_MS;
        }
    }

    /// 全员确认后收工：清掉这一轮，watcher 不再重发。
    fn finish(&mut self) {
        self.pending = None;
        self.acked.clear();
    }
}

/// 单调时钟毫秒：只用来算「离下次重发还有多久」，不受系统改时间影响。
fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// 广播状态的进程级单例：watcher 线程写，`login_ack` 命令读改写。
///
/// 用全局而不是 `State`：watcher 在 setup 阶段起线程时还没有可供 `manage` 的
/// 句柄，而这个状态的生命周期与进程等同，不需要参与 Tauri 的注入体系。
fn broadcast() -> &'static std::sync::Mutex<LoginBroadcast> {
    static BROADCAST: std::sync::OnceLock<std::sync::Mutex<LoginBroadcast>> =
        std::sync::OnceLock::new();
    BROADCAST.get_or_init(|| std::sync::Mutex::new(LoginBroadcast::default()))
}

/// 处理一次前端确认：记账，并在「全员已消费」时收工。
///
/// 返回还缺几个成员。`0` = 这一轮结束了（全员确认，或这次确认属于旧轮次），
/// `>0` = 还有人没回话，watcher 继续按 5 秒补发，最多发满 3 次。
fn handle_ack(platform: &str, state: &str) -> usize {
    let mut guard = broadcast().lock().unwrap_or_else(|e| e.into_inner());
    let outcome = {
        // 拆成两块：`guard` 的两个字段不能同时可变借用。
        let LoginBroadcast { pending, acked } = &mut *guard;
        apply_ack(pending, acked, platform, state)
    };
    if outcome == AckOutcome::Done {
        // 旧轮次的迟到确认：不结算新一轮，也不会把它提前掐掉。
        return 0;
    }
    let waiting = member_count().saturating_sub(guard.acked.len());
    if waiting == 0 {
        guard.finish();
    }
    waiting
}

/// 成员数：本应用只有一个 WebView，登录态按平台播，所以成员就是平台占位。
///
/// 窗口内两个订阅者（`ChatPage` 的 `onLoginState` 与 `DashboardPage` 的
/// `subscribeLoginState`）共用一个 ack 出口，多窗口同步是后续议题
/// （当前 `tauri.conf.json` 只有 `main` 一个窗口）。
fn member_count() -> usize {
    1
}

/// `login-state` 事件的载荷（**蛇形命名，冻结契约**）。
///
/// 前端用 `listen('login-state', e => e.payload)` 接收；
/// `logged_in` 是 `state == "logged_in"` 的派生便利字段。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LoginStatePayload {
    platform: String,
    state: String,
    logged_in: bool,
}

/// 从会话进度里提取值得广播的登录态快照。
///
/// 刚创建、还没收到任何登录事件的会话（platform 空且 unknown）不播：
/// 那不是「登录态变化」，只是「会话存在」，播出去只会让界面闪一下未知态。
fn login_snapshot(progress: &core::sidecar::ScrapeProgress) -> Option<LoginSnapshot> {
    if progress.login_platform.is_empty() && progress.login_state == "unknown" {
        return None;
    }
    Some(LoginSnapshot {
        platform: progress.login_platform.clone(),
        state: progress.login_state.clone(),
    })
}

/// 广播决策：只在 (platform, state) 变化时发事件。
///
/// 会话消失（logout / scrape_close / 崩溃后未重启）时补发一次 `closed`，
/// 让界面有机会把「已登录」徽标收掉；此后再无会话就不再重复播。
/// 启动时本来就没有会话、也从未播过任何状态 → 什么都不发。
fn login_emit_decision(
    last: Option<&LoginSnapshot>,
    current: Option<LoginSnapshot>,
) -> Option<LoginSnapshot> {
    match current {
        Some(next) => {
            if last == Some(&next) {
                None
            } else {
                Some(next)
            }
        }
        None => match last {
            Some(prev) if prev.state != "closed" => Some(LoginSnapshot {
                platform: prev.platform.clone(),
                state: "closed".to_string(),
            }),
            _ => None,
        },
    }
}

/// 登录态 watcher：把 sidecar 的登录态变化 emit 给 WebView。
///
/// 为什么是轮询而不是在会话创建处装回调：会话有 6 个创建触点
/// （scrape_start / maybe_restart / login_start / login_status / session_open /
/// auto_greet），逐处装 observer 漏一处就是静默失效——正是本工程反复踩过的
/// 坑。watcher 只读 `ScrapeState` 这一个集中点，天然覆盖全部触点与崩溃重启。
/// 500ms 的粒度对「扫码登录」这种秒级状态足够；登录态的权威来源仍是
/// `login_status` 命令与 `scrape_drain` 轮询，本事件只是补充推送。
fn spawn_login_state_watcher(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name("login-state-watcher".into())
        .spawn(move || {
            let mut last: Option<LoginSnapshot> = None;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let current = {
                    let state = handle.state::<ScrapeState>();
                    // 锁中毒也继续播：watcher 是旁路观察者，不该因一次 panic 永久失声
                    let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
                    guard.as_ref().and_then(|s| login_snapshot(&s.progress()))
                };
                if let Some(next) = login_emit_decision(last.as_ref(), current) {
                    // 状态真的变了：登记新一轮，**随后统一走下面「这一拍该不该发」那段**。
                    // 原先这里自己 emit 一次再 `begin()`：首次广播绕过了 `due()` 的上限
                    // 判定，`begin()` 又把发起方预置成「已确认」，两条一起把 ack 变成空壳
                    // （用户 2026-11-25「boss 列表 ack 失效了」）。
                    // 先记账再推进 `last`：emit 失败时 `last` 不动，下一拍仍会进这个分支重来。
                    broadcast().lock().unwrap_or_else(|e| e.into_inner()).begin(next.clone());
                    last = Some(next);
                }
                // 这一拍该不该发：首次广播与「没人确认」的补发共用同一段。
                // 发满 LOGIN_BROADCAST_MAX 次后 `due_now()` 恒为 false；全员确认后 `pending`
                // 被清空，同样不再发。
                let (due, snap) = {
                    let guard = broadcast().lock().unwrap_or_else(|e| e.into_inner());
                    (guard.due_now(), guard.pending.as_ref().map(|p| p.snap.clone()))
                };
                let Some(snap) = snap.filter(|_| due) else { continue };
                let payload = LoginStatePayload {
                    logged_in: snap.state == "logged_in",
                    platform: snap.platform.clone(),
                    state: snap.state.clone(),
                };
                match handle.emit("login-state", &payload) {
                    Ok(()) => broadcast().lock().unwrap_or_else(|e| e.into_inner()).mark_sent(),
                    Err(e) => {
                        // 发失败不计数：5 秒后再来一次，别把次数白扔掉。
                        eprintln!("[login-watch] 发 login-state 失败：{e}");
                    }
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("[login-watch] 登录态 watcher 启动失败（登录事件不再推送，轮询仍可用）：{e}");
    }
}

pub fn run() {
    use tauri::Manager;

    // 必须在 Builder 之前完成：WebView 一旦开始创建就没有补救机会。
    let startup = (|| -> Result<InstanceLock, String> {
        let pre = tauri::Builder::default()
            .build(tauri::generate_context!())
            .map_err(|e| format!("构建 Tauri 应用失败: {e}"))?;
        prepare_webview_dir(pre.handle())
    })();

    let lock = match startup {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("[startup] 启动前置检查失败: {e}");
            // 退出码 2 与正常退出、panic(101) 区分，便于自动化脚本判定
            std::process::exit(2);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // 锁句柄必须活到进程退出，否则单实例形同虚设
        .manage(lock)
        // 存储句柄由 setup 阶段填进去（那时才知道数据目录）
        .manage(StoreState(std::sync::Mutex::new(None)))
        // 大模型句柄同理：密钥存在 app_config 表里，得先有数据库
        .manage(LlmState::empty())
        // 抓取会话句柄：用户点「开始抓取」时才有值
        .manage(ScrapeState::empty())
        // 自动投递运行态（前缀驱动状态机，见 commands_auto 的模块说明）
        .manage(commands_auto::AutoGreetState::empty())
        // 应用路径：setup 阶段把数据目录填进去，供 app_info / 配置页显示
        .manage(commands_config::AppPaths::empty())
        .setup(|app| {
            // 必须在主线程、窗口创建完成后才能应用特效。
            if let Some(win) = app.get_webview_window("main") {
                // Haze 自己写 ACCENT_ENABLE_HOSTBACKDROP；成功时不能再走 accent，
                // 否则会把 HostBackdropBrush 覆盖掉。
                #[cfg(target_os = "windows")]
                let glass_attached = attach_haze(&win);
                #[cfg(not(target_os = "windows"))]
                let glass_attached = false;
                if !glass_attached {
                    // 主题存在前端 localStorage，这里还不知道，先按深色档起窗口；
                    // 前端挂载后经 `set_window_theme` 换成当前主题的底色。
                    apply_blur(&win, "dark");
                }
                // 诊断放在特效之后：先打印会看不到特效是否真正落地。
                window_diag(&win, glass_attached);
                // 窗口四角由 DWM 按 8px 画弧：透明客户区的方角只能由系统裁掉，
                // 前端外壳与它同心内缩（--shell-radius），不再自己另画一套外形。
                apply_round_corners(&win);
            }

            // 打开数据库：失败不阻断启动，否则用户连界面都看不到。
            // 命令层会在未初始化时返回「数据库尚未初始化」，前端据此提示。
            match app.path().app_local_data_dir() {
                Ok(dir) => {
                    // 先落路径：即使数据库打不开，配置页也能显示「数据目录在哪」
                    app.state::<commands_config::AppPaths>().set(dir.clone());
                    let db_path = dir.join("job_assistant.db");
                    match commands_db::open_store(db_path.clone()) {
                        Ok(store) => {
                            // 先读配置再交出所有权：读不到密钥只是「未配置」，
                            // 应用继续以纯规则打分运行
                            match store.config_pairs() {
                                Ok(pairs) => {
                                    // 密钥、模型、两份温度一次读完再构造：温度要在启动时就生效
                                    match commands_db::build_llm_client_with_temperature(&pairs) {
                                        Some(client) => {
                                            if let Ok(mut g) = app.state::<LlmState>().0.lock() {
                                                *g = Some(client);
                                            }
                                            eprintln!("[startup] 大模型已配置");
                                        }
                                        None => {
                                            eprintln!("[startup] 未配置大模型密钥，使用规则打分")
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[startup] 读取配置失败（使用规则打分）：{e}");
                                    commands_logs::push("warn", format!("读取配置失败（使用规则打分）：{e}"));
                                }
                            }
                            if let Ok(mut guard) = app.state::<StoreState>().0.lock() {
                                *guard = Some(store);
                            }
                            eprintln!("[startup] 数据库已打开：{}", db_path.display());
                        }
                        Err(e) => {
                            eprintln!("[startup] 打开数据库失败（应用继续运行）：{e}");
                            commands_logs::push("error", format!("打开数据库失败（应用继续运行）：{e}"));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[startup] 无法解析数据目录（应用继续运行）：{e}");
                    commands_logs::push("error", format!("无法解析数据目录（应用继续运行）：{e}"));
                }
            }
            // 登录态推送：sidecar → progress → watcher → WebView（契约见
            // LoginStatePayload）。放在 setup 末尾：此时 ScrapeState 已 manage。
            spawn_login_state_watcher(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_scoring_spec,
            app_info,
            window_minimize,
            window_toggle_maximize,
            window_close,
            set_window_theme,
            login_ack,
            commands_db::list_jobs,
            commands_db::get_job,
            commands_db::db_overview,
            commands_db::migrate_scores,
            commands_db::score_job,
            commands_db::passes_threshold,
            commands_db::update_score,
            commands_db::rescore_jobs,
            commands_db::llm_status,
            commands_db::job_tab_counts,
            commands_db::reject_jobs,
            commands_db::allow_jobs,
            commands_db::mark_delivered,
            commands_db::unmark_delivered,
            commands_db::list_decisions,
        commands_db::count_decisions,
        commands_db::decision_counts,
            commands_sensitive::sensitive_list,
            commands_sensitive::sensitive_remove,
            commands_config::get_app_config,
            commands_config::set_app_config,
            commands_config::set_limit,
            commands_config::set_scrape_defaults,
            commands_auto::auto_greet_next,
            commands_auto::auto_greet_pause,
            commands_auto::auto_greet_resume,
            commands_auto::auto_greet_status,
            commands_auto::auto_greet_reset,
            commands_scrape::scrape_start,
            commands_scrape::scrape_stop,
            commands_scrape::scrape_progress,
            commands_scrape::scrape_drain,
            commands_scrape::login_start,
            commands_scrape::login_status,
            commands_scrape::login_save,
            commands_scrape::logout,
            commands_scrape::scrape_close,
            commands_scrape::session_open,
            commands_logs::list_logs,
            commands_logs::clear_logs,
            commands_chat::chat_friends,
            commands_chat::chat_messages,
            commands_chat::chat_open,
            commands_chat::chat_friends_local,
            commands_chat::chat_messages_local,
            commands_chat::chat_send,
            commands_chat::chat_resumes,
            commands_chat::chat_send_resume,
    commands_chat::chat_recall,
        commands_chat::job_fetch_detail,
        commands_company::company_jobs,
        commands_company::company_job_preview,
        commands_company::job_import,
            commands_greet::greet_draft,
            commands_greet::greet_one,
            commands_chat::list_expectations,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 启动失败");
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use std::path::Path;

    /// 报文必须同时给出「病因关键词」与「可执行的处置」，否则等于没报。
    #[test]
    fn hint_names_root_cause_and_action() {
        let msg = not_writable_hint(
            Path::new("C:\\Users\\x\\AppData\\Local\\com.ai.jobassistant"),
            "拒绝访问。 (os error 5)",
            None,
        );
        for needle in [
            "Low",
            "S-1-16-4096",
            "0x800700AA",
            "CARGO_TARGET_DIR",
            "工作区外",
        ] {
            assert!(msg.contains(needle), "报文缺少「{needle}」：{msg}");
        }
    }

    /// 目录不存在时探针必须报错，不能因为「写入被吞」而放行到 WebView2。
    #[test]
    fn probe_rejects_missing_directory() {
        let missing = std::env::temp_dir().join("aja-probe-definitely-missing-dir");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(probe_writable(&missing).is_err());
    }
}

#[cfg(test)]
mod login_watch_tests {
    use super::*;

    fn snap(platform: &str, state: &str) -> LoginSnapshot {
        LoginSnapshot {
            platform: platform.into(),
            state: state.into(),
        }
    }

    /// 载荷形状是冻结契约：蛇形三字段，前端按此监听，不得漂移。
    #[test]
    fn payload_shape_is_frozen_snake_case() {
        let v = serde_json::to_value(LoginStatePayload {
            platform: "boss".into(),
            state: "logged_in".into(),
            logged_in: true,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"platform": "boss", "state": "logged_in", "logged_in": true})
        );
    }

    #[test]
    fn blank_unknown_session_is_not_broadcast() {
        let mut p = core::sidecar::ScrapeProgress::new();
        assert_eq!(login_snapshot(&p), None, "新会话不该播未知态");
        p.login_state = "waiting".into();
        p.login_platform = "boss".into();
        assert_eq!(login_snapshot(&p), Some(snap("boss", "waiting")));
        // login_status 写回 no_login 时同样可播
        p.login_state = "no_login".into();
        assert_eq!(login_snapshot(&p), Some(snap("boss", "no_login")));
    }

    #[test]
    fn first_known_state_is_emitted_once() {
        let cur = snap("boss", "waiting");
        assert_eq!(login_emit_decision(None, Some(cur.clone())), Some(cur.clone()));
        assert_eq!(login_emit_decision(Some(&cur), Some(cur.clone())), None, "重复状态不重播");
    }

    #[test]
    fn state_change_is_emitted() {
        let prev = snap("boss", "waiting");
        let next = snap("boss", "logged_in");
        assert_eq!(login_emit_decision(Some(&prev), Some(next.clone())), Some(next));
    }

    #[test]
    fn session_gone_emits_closed_exactly_once() {
        let prev = snap("boss", "logged_in");
        let closed = snap("boss", "closed");
        assert_eq!(login_emit_decision(Some(&prev), None), Some(closed.clone()));
        assert_eq!(login_emit_decision(Some(&closed), None), None, "closed 不重复播");
        assert_eq!(login_emit_decision(None, None), None, "从未有过会话就不播");
    }

    /// 一轮广播：首次发 1 次，没人确认就 5 秒一次补发，发满 3 次就停（用户 m02416）。
    #[test]
    fn broadcast_sends_at_most_three_times() {
        let mut b = LoginBroadcast::default();
        b.begin(snap("boss", "logged_in"));
        // 首次要立刻放行（`due_ms` 已拨到 5 秒后，只有 `due_now` 认这个 sent == 1）。
        assert!(b.due_now(), "登记后首次广播必须立刻发");
        b.mark_sent(); // 1 次
        // 刚发完首次：500ms 的 watcher 拍子还没到 5 秒，不该补发
        assert!(!b.due_now(), "刚发完首次不该立刻补发");
        // `begin` 已经把首次记进 sent（=1），上面 mark_sent 之后是 2 次
        assert_eq!(b.pending.as_ref().unwrap().sent, 2, "首次 + 一次补发 = 2 次");
        // 3 次：还没到上限，到点就发
        b.pending.as_mut().unwrap().due_ms = 0;
        assert!(b.due_now());
        b.mark_sent(); // 3 次
        // 上限：`sent == LOGIN_BROADCAST_MAX` 之后无论到没到点都不再发
        b.pending.as_mut().unwrap().due_ms = 0;
        assert!(!b.due_now(), "发满 3 次后不再补发");
    }

    /// 有人确认就立刻收工：不等发满 3 次（用户 2026-11-25「ack 失效」的回归）。
    #[test]
    fn ack_from_all_members_stops_broadcast() {
        let mut b = LoginBroadcast::default();
        b.begin(snap("boss", "logged_in"));
        // 关键前置：`begin()` 之后**没有任何人**确认过。曾经这里被预置成已确认，
        // 于是下面第一笔确认直接把这一轮结算掉，5 秒补发永不发生。
        assert!(b.acked.is_empty(), "登记新一轮不等于发起方已确认");
        assert_eq!(
            apply_ack(&mut b.pending, &mut b.acked, "boss", "logged_in"),
            AckOutcome::Pending
        );
        assert_eq!(b.acked.len(), member_count(), "全员确认后该轮应结算");
        b.finish();
        assert!(b.pending.is_none(), "结算后不再重发");
        assert!(!b.due_now(), "结算后这一拍也不该再发");
        assert!(b.acked.is_empty());
    }

    /// 没人确认时补发确实会发生：这正是 ack 存在的意义。
    #[test]
    fn unacked_round_is_rebroadcast() {
        let mut b = LoginBroadcast::default();
        b.begin(snap("boss", "logged_in"));
        assert!(b.due_now());
        b.mark_sent();
        // 5 秒到了还没人确认 ⇒ 必须再发。
        b.pending.as_mut().unwrap().due_ms = 0;
        assert!(b.due_now(), "没收到 ack 就该补发");
        b.mark_sent(); // 3 次：已到上限
        assert_eq!(b.pending.as_ref().unwrap().sent, LOGIN_BROADCAST_MAX);
        b.pending.as_mut().unwrap().due_ms = 0;
        assert!(!b.due_now(), "没人确认也只补到上限为止，不会变成常驻轮询");
    }

    /// 旧轮次的迟到确认不能把新一轮提前掐掉。
    #[test]
    fn stale_ack_does_not_end_new_round() {
        let mut b = LoginBroadcast::default();
        b.begin(snap("boss", "logged_in"));
        assert_eq!(
            apply_ack(&mut b.pending, &mut b.acked, "boss", "waiting"),
            AckOutcome::Done
        );
        assert!(b.pending.is_some(), "旧轮次确认不该清掉新一轮");
        // 平台对不上同理
        assert_eq!(
            apply_ack(&mut b.pending, &mut b.acked, "zhilian", "logged_in"),
            AckOutcome::Done
        );
        assert!(b.pending.is_some());
        // 没有待播广播时的确认直接 Done，不 panic
        let mut empty = LoginBroadcast::default();
        assert_eq!(
            apply_ack(&mut empty.pending, &mut empty.acked, "boss", "logged_in"),
            AckOutcome::Done
        );
    }
}
