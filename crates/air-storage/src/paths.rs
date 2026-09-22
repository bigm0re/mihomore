use std::path::{Path, PathBuf};

use air_error::{AppResult, StorageError};
use air_paths::{AppDirRoots, PathMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub subscription_cache_dir: PathBuf,
    pub cores_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub backups_dir: PathBuf,
    /// 目录来源，用于日志和诊断区分便携模式与系统回退。
    pub mode: PathMode,
}

impl AppPaths {
    pub fn resolve() -> AppResult<Self> {
        // 目录解析集中在 air-paths：便携优先，不可写时回退系统目录。
        let roots = AppDirRoots::resolve()?;
        let paths = Self::from_roots(roots);
        tracing::info!(
            mode = paths.mode.label(),
            config_dir = %paths.config_dir.display(),
            data_dir = %paths.data_dir.display(),
            cache_dir = %paths.cache_dir.display(),
            "resolved application paths"
        );
        Ok(paths)
    }

    pub fn from_roots(roots: AppDirRoots) -> Self {
        let mut paths = Self::from_base_dirs(&roots.config_dir, &roots.data_dir, &roots.cache_dir);
        paths.mode = roots.mode;
        paths
    }

    pub fn from_base_dirs(config_dir: &Path, data_dir: &Path, cache_dir: &Path) -> Self {
        // Windows/macOS/Linux 的系统目录不同，但业务层只依赖这些语义化子目录。
        Self {
            config_dir: config_dir.to_path_buf(),
            data_dir: data_dir.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            subscription_cache_dir: config_dir.join("subscriptions"),
            cores_dir: cache_dir.join("core"),
            logs_dir: data_dir.join("logs"),
            backups_dir: data_dir.join("backups"),
            mode: PathMode::System,
        }
    }

    pub fn is_portable(&self) -> bool {
        self.mode.is_portable()
    }

    pub fn init(&self) -> AppResult<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.subscription_cache_dir,
            &self.cores_dir,
            &self.logs_dir,
            &self.backups_dir,
        ] {
            std::fs::create_dir_all(dir).map_err(StorageError::Io)?;
            tracing::debug!(path = %dir.display(), "ensured application directory exists");
        }
        tracing::info!("initialized application directory layout");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_semantic_subdirectories_from_platform_roots() {
        let paths = AppPaths::from_base_dirs(
            Path::new("/config/mihomore"),
            Path::new("/data/mihomore"),
            Path::new("/cache/mihomore"),
        );

        assert_eq!(
            paths.subscription_cache_dir,
            PathBuf::from("/config/mihomore/subscriptions")
        );
        assert_eq!(paths.cores_dir, PathBuf::from("/cache/mihomore/core"));
        assert_eq!(paths.backups_dir, PathBuf::from("/data/mihomore/backups"));
    }

    #[test]
    fn portable_roots_are_reported_as_portable() {
        // 便携模式必须能从 AppPaths 观察到，日志和设置页都依赖这个投影。
        let paths = AppPaths::from_roots(air_paths::AppDirRoots::portable("/opt/mihomore"));

        assert!(paths.is_portable());
        assert_eq!(paths.config_dir, PathBuf::from("/opt/mihomore/config"));
        assert_eq!(paths.cores_dir, PathBuf::from("/opt/mihomore/cache/core"));
    }

    #[test]
    fn keeps_same_layout_for_common_platform_roots() {
        // 目录库会按平台返回不同根目录；这里验证业务子目录在三类根目录下保持一致。
        for (config, data, cache) in [
            (
                r"C:\Users\Alice\AppData\Roaming\org.mihomore\mihomore\config",
                r"C:\Users\Alice\AppData\Roaming\org.mihomore\mihomore\data",
                r"C:\Users\Alice\AppData\Local\org.mihomore\mihomore\cache",
            ),
            (
                "/Users/alice/Library/Application Support/org.mihomore.mihomore",
                "/Users/alice/Library/Application Support/org.mihomore.mihomore",
                "/Users/alice/Library/Caches/org.mihomore.mihomore",
            ),
            (
                "/home/alice/.config/mihomore",
                "/home/alice/.local/share/mihomore",
                "/home/alice/.cache/mihomore",
            ),
        ] {
            let paths =
                AppPaths::from_base_dirs(Path::new(config), Path::new(data), Path::new(cache));

            assert!(paths.subscription_cache_dir.ends_with("subscriptions"));
            assert!(paths.cores_dir.ends_with("core"));
            assert!(paths.logs_dir.ends_with("logs"));
            assert!(paths.backups_dir.ends_with("backups"));
        }
    }
}
