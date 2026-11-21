//! sidecar 的启动配置与查找规则（纯路径计算，无副作用）。
//!
//! 从 `process.rs` 搬出（行为零变化）：`SidecarConfig`、`locate_script_dir`、
//! 解释器环境变量名与 ready 超时。单独一个文件是因为「sidecar.py 到底在哪」
//! 的搜索顺序是打包与开发态唯一的分歧点，改动都该只动这一处。

use std::path::{Path, PathBuf};
use std::time::Duration;
pub const PYTHON_ENV: &str = "AJA_PYTHON";

/// 启动后等待 `ready` 事件的默认上限。
///
/// 实测 `python -c ""` 冷启动约 0.2 s；patchright 要等到第一次
/// `scrape.start` 才导入（sidecar 是懒加载），所以这里只等解释器起来。
/// 20 s 足以覆盖磁盘繁忙 / 杀软扫描期。
pub const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(20);
/// 启动配置。
#[derive(Debug, Clone)]
pub struct SidecarConfig {
    /// Python 解释器；`None` 时取 `AJA_PYTHON`，再退化到 `python`
    pub program: Option<String>,
    /// `scraper/` 目录：sidecar.py 必须在这里（它靠 `__file__` 找兄弟模块）
    pub script_dir: PathBuf,
    /// 浏览器持久化目录。**必须与现行版共用**，否则用户要重新扫码登录
    /// （见 `scraper/sidecar.py::data_dir` 注释）。
    pub data_dir: Option<PathBuf>,
}

impl SidecarConfig {
    pub fn new(script_dir: impl Into<PathBuf>) -> Self {
        Self {
            program: None,
            script_dir: script_dir.into(),
            data_dir: None,
        }
    }

    pub(super) fn program(&self) -> String {
        self.program
            .clone()
            .or_else(|| std::env::var(PYTHON_ENV).ok())
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| "python".to_string())
    }

    pub(super) fn script(&self) -> PathBuf {
        self.script_dir.join("sidecar.py")
    }
}

/// 打包后 `scraper/` 会被释放到 exe 旁边（SPEC-002 V4），开发态则在
/// 仓库根。全部不命中时返回 `None`，由命令层报可读错误。
pub fn locate_script_dir(exe_dir: Option<&Path>, manifest_dir: &Path) -> Option<PathBuf> {

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = exe_dir {
        candidates.push(dir.join("scraper"));
        candidates.push(dir.to_path_buf());
    }
    candidates.push(manifest_dir.join("scraper"));
    candidates.push(manifest_dir.join("..").join("scraper"));
    candidates.push(manifest_dir.join("..").join("..").join("scraper"));
    candidates
        .into_iter()
        .find(|p| p.join("sidecar.py").is_file())
}
