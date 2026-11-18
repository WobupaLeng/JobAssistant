use std::path::Path;

/// 构建脚本：决定是否把 Python 后端嵌进主 exe。
///
/// - 若 `backend-exe/aja-backend.exe` 存在（由 build.bat 先跑 PyInstaller 产出），
///   定义 `AJA_BACKEND_EMBEDDED`，lib.rs 里的 `include_bytes!` 才会生效，
///   最终用户拿到的就是一个单一 exe。
/// - 若不存在（纯前端/Rust 开发阶段），不定义该变量，lib.rs 走
///   `python -m app.main` 的开发态路径，避免编译时报"找不到文件"。
fn main() {
    let backend = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("backend-exe")
        .join("aja-backend.exe");

    // 文件变化要触发重新编译，否则换后端后主 exe 里还是旧字节
    println!("cargo:rerun-if-changed=../backend-exe/aja-backend.exe");

    if backend.is_file() {
        println!("cargo:rustc-env=AJA_BACKEND_EMBEDDED=1");
        let size = std::fs::metadata(&backend).map(|m| m.len()).unwrap_or(0);
        println!("cargo:warning=内嵌后端 {size} 字节: {}", backend.display());
    } else {
        println!("cargo:warning=未找到 backend-exe/aja-backend.exe，本次构建走开发态路径");
    }

    tauri_build::build()
}
