//! 应用目录解析：便携优先。
//!
//! mihomore 的目标是"解压即用"，因此默认把 `config/`、`data/`、`cache/` 放在
//! 可执行文件同级目录，拷贝整个文件夹即可迁移或备份。只有在软件目录不可写
//! （例如被放进 `Program Files`）时才回退到系统用户目录。
//!
//! 该模块必须保持极低依赖：`air-storage`、`air-telemetry`、`air-platform`
//! 都需要解析目录，但它们之间不能互相依赖，否则会形成环。因此目录语义集中
//! 在这里，由各层复用。

use std::path::{Path, PathBuf};

use air_error::{AppResult, StorageError};

/// 便携模式下配置目录的固定名称。
pub const PORTABLE_CONFIG_DIR: &str = "config";
/// 便携模式下数据目录的固定名称。
pub const PORTABLE_DATA_DIR: &str = "data";
/// 便携模式下缓存目录的固定名称。
pub const PORTABLE_CACHE_DIR: &str = "cache";

/// 允许用户强制指定便携根目录的环境变量。
///
/// 用于绿色版分发和自动化测试：设置后不再探测 exe 同级目录，也不再回退系统目录。
pub const PORTABLE_HOME_ENV: &str = "MIHOMORE_HOME";

/// 目录解析结果来源，用于日志和诊断展示。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathMode {
    /// 数据跟随可执行文件，位于软件目录下。
    Portable,
    /// 软件目录不可写，数据位于系统用户目录。
    System,
}

impl PathMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Portable => "便携目录",
            Self::System => "系统目录",
        }
    }

    pub fn is_portable(self) -> bool {
        matches!(self, Self::Portable)
    }
}

/// 应用的三类基础目录。
///
/// 只保存语义根目录，具体子目录（`subscriptions`、`core`、`logs`、`backups`）
/// 由 `air-storage` 派生，避免目录约定在两个 crate 里各写一份。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDirRoots {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub mode: PathMode,
}

impl AppDirRoots {
    pub fn from_base_dirs(
        config_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
        mode: PathMode,
    ) -> Self {
        Self {
            config_dir: simplify_path(&config_dir.into()),
            data_dir: simplify_path(&data_dir.into()),
            cache_dir: simplify_path(&cache_dir.into()),
            mode,
        }
    }

    /// 便携布局：`<base>/config`、`<base>/data`、`<base>/cache`。
    pub fn portable(base_dir: impl Into<PathBuf>) -> Self {
        let base = simplify_path(&base_dir.into());
        Self {
            config_dir: base.join(PORTABLE_CONFIG_DIR),
            data_dir: base.join(PORTABLE_DATA_DIR),
            cache_dir: base.join(PORTABLE_CACHE_DIR),
            mode: PathMode::Portable,
        }
    }

    /// 解析应用目录，便携优先。
    ///
    /// 顺序：显式环境变量 → 可写的可执行文件同级目录 → 系统用户目录。
    pub fn resolve() -> AppResult<Self> {
        if let Some(base) = portable_home_override() {
            // 显式指定时不再探测可写性：用户既然指定了目录，就应当拿到确定的错误，
            // 而不是被静默改写到系统目录，导致数据"消失"。
            let roots = Self::portable(&base);
            roots.init()?;
            tracing::info!(
                base_dir = %base.display(),
                config_dir = %roots.config_dir.display(),
                data_dir = %roots.data_dir.display(),
                cache_dir = %roots.cache_dir.display(),
                "已按环境变量指定便携目录，应用数据位于软件目录"
            );
            return Ok(roots);
        }

        if let Some(exe_dir) = executable_dir()
            && let Some(roots) = try_portable(&exe_dir)
        {
            tracing::info!(
                base_dir = %exe_dir.display(),
                config_dir = %roots.config_dir.display(),
                data_dir = %roots.data_dir.display(),
                cache_dir = %roots.cache_dir.display(),
                "已进入便携模式，应用数据位于软件目录"
            );
            return Ok(roots);
        }

        let roots = Self::system()?;
        tracing::info!(
            config_dir = %roots.config_dir.display(),
            data_dir = %roots.data_dir.display(),
            cache_dir = %roots.cache_dir.display(),
            "软件目录不可写，回退到系统用户目录"
        );
        Ok(roots)
    }

    /// 系统用户目录布局，仅作为不可写场景的回退。
    pub fn system() -> AppResult<Self> {
        let dirs = directories::ProjectDirs::from("org.mihomore", "", "mihomore")
            .ok_or(StorageError::ProjectDirsUnavailable)?;
        Ok(Self::from_base_dirs(
            dirs.config_dir(),
            dirs.data_dir(),
            dirs.cache_dir(),
            PathMode::System,
        ))
    }

    /// 创建全部基础目录。便携探测和正常启动都会走到这里，语义一致。
    pub fn init(&self) -> AppResult<()> {
        for dir in [&self.config_dir, &self.data_dir, &self.cache_dir] {
            std::fs::create_dir_all(dir).map_err(StorageError::Io)?;
        }
        Ok(())
    }
}

/// 去掉 Windows verbatim（`\\?\`）前缀。
///
/// 这类路径会**关闭** Windows 的路径规范化：正斜杠不再被当作分隔符，`.`/`..` 也不再解析。
/// 后果是真实的：mihomo（Go）内部用 `filepath.Join(dir, "cache.db")` 拼缓存路径，
/// 若 `dir` 带 verbatim 前缀，得到的就是 `\\?\D:\...\core/cache.db`，
/// 该文件打不开 → mihomo 无法持久化用户选择的节点 → 重启后回退到第一个节点。
///
/// 因此在所有目录进入应用之前统一剥离该前缀，而不是在各个调用点各自处理。
/// 非 Windows 平台与不带前缀的路径原样返回。
pub fn simplify_path(path: &Path) -> PathBuf {
    dunce::simplified(path).to_path_buf()
}

/// 读取 `MIHOMORE_HOME` 覆盖值；空白值视为未设置。
fn portable_home_override() -> Option<PathBuf> {
    let raw = std::env::var_os(PORTABLE_HOME_ENV)?;
    let value = raw.to_string_lossy();
    if value.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

/// 当前可执行文件所在目录。
fn executable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Windows 上 std 的 `canonicalize` 会**加上** `\\?\` verbatim 前缀（不是去掉）。
    // 该前缀一旦下传给 mihomo 就会破坏其内部路径拼接，因此这里用 dunce 解析符号链接，
    // 同时保证不引入 verbatim 前缀。
    let exe = dunce::canonicalize(&exe).unwrap_or(exe);
    exe.parent().map(Path::to_path_buf)
}

/// 尝试把软件目录作为便携根目录；不可写时返回 `None` 交给上层回退。
fn try_portable(base_dir: &Path) -> Option<AppDirRoots> {
    let roots = AppDirRoots::portable(base_dir);
    match roots.init() {
        Ok(()) => Some(roots),
        Err(error) => {
            // 目录不可写属于预期分支（安装在受保护目录），使用 debug 级别避免污染启动日志。
            tracing::debug!(
                base_dir = %base_dir.display(),
                %error,
                "便携目录不可写，准备回退系统目录"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_layout_derives_fixed_subdirectory_names() {
        let roots = AppDirRoots::portable(Path::new("/opt/mihomore"));

        assert_eq!(roots.config_dir, PathBuf::from("/opt/mihomore/config"));
        assert_eq!(roots.data_dir, PathBuf::from("/opt/mihomore/data"));
        assert_eq!(roots.cache_dir, PathBuf::from("/opt/mihomore/cache"));
        assert_eq!(roots.mode, PathMode::Portable);
        assert!(roots.mode.is_portable());
    }

    #[test]
    fn portable_init_creates_all_three_directories() {
        let temp = tempfile::tempdir().unwrap();
        let roots = AppDirRoots::portable(temp.path());

        roots.init().unwrap();

        assert!(roots.config_dir.is_dir());
        assert!(roots.data_dir.is_dir());
        assert!(roots.cache_dir.is_dir());
    }

    #[test]
    fn mode_labels_are_stable_for_logs() {
        assert_eq!(PathMode::Portable.label(), "便携目录");
        assert_eq!(PathMode::System.label(), "系统目录");
        assert!(!PathMode::System.is_portable());
    }

    #[test]
    fn system_roots_stay_distinct_per_category() {
        let roots = AppDirRoots::system().unwrap();

        assert_eq!(roots.mode, PathMode::System);
        // 系统布局下三类目录不应互相嵌套，否则便携回退会覆盖用户配置。
        assert_ne!(roots.config_dir, roots.data_dir);
        assert_ne!(roots.data_dir, roots.cache_dir);
    }

    /// 回归测试：verbatim 前缀会让 mihomo 无法创建 `cache.db`，
    /// 导致用户选择的节点无法持久化（重启后回退到第一个节点）。
    #[test]
    fn verbatim_prefix_is_stripped_from_resolved_roots() {
        let raw = PathBuf::from(r"\\?\D:\mihomore");

        let portable = AppDirRoots::portable(raw.clone());
        let from_base = AppDirRoots::from_base_dirs(
            raw.join("config"),
            raw.join("data"),
            raw.join("cache"),
            PathMode::Portable,
        );

        for roots in [&portable, &from_base] {
            for dir in [&roots.config_dir, &roots.data_dir, &roots.cache_dir] {
                let text = dir.to_string_lossy();
                assert!(
                    !text.starts_with(r"\\?\"),
                    "目录仍带 verbatim 前缀，会让 mihomo 无法写 cache.db: {text}"
                );
                assert!(
                    !text.contains('/'),
                    "剥离前缀后不应出现正斜杠（mihomo 内部拼接会出问题）: {text}"
                );
            }
        }

        // 前缀被去掉，但实际目录语义必须保持不变。
        assert_eq!(portable.config_dir, PathBuf::from(r"D:\mihomore\config"));
        assert_eq!(portable.cache_dir, PathBuf::from(r"D:\mihomore\cache"));
    }

    #[test]
    fn simplify_path_keeps_ordinary_paths_unchanged() {
        let plain = PathBuf::from(r"D:\mihomore\cache");

        assert_eq!(simplify_path(&plain), plain);
    }

    /// 直接验证真实的 `executable_dir()`：它在 Windows 上会对 `current_exe()` 做解析，
    /// 这是 verbatim 前缀最容易泄漏进来的入口。
    #[test]
    fn executable_dir_never_yields_a_verbatim_prefix() {
        let dir = executable_dir().expect("current_exe should resolve during tests");
        let text = dir.to_string_lossy();

        assert!(
            !text.starts_with(r"\\?\"),
            "executable_dir 返回了 verbatim 前缀路径，会让 mihomo 无法写 cache.db: {text}"
        );

        // 便携根目录下派生的 cores_dir 就是最终传给 mihomo 的 `-d`，同样不能带前缀。
        let roots = AppDirRoots::portable(&dir);
        let cores_dir = roots.cache_dir.join("core");
        assert!(
            !cores_dir.to_string_lossy().starts_with(r"\\?\"),
            "cores_dir 带 verbatim 前缀: {}",
            cores_dir.display()
        );
    }
}
