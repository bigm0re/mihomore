use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use air_error::AppResult;
pub const CORE_SERVICE_NAME: &str = "MihomoreCore";
pub const CORE_SERVICE_DISPLAY_NAME: &str = "mihomore Core Service";
pub(super) const CORE_SERVICE_ARG: &str = "--mihomore-core-service";
pub(super) const ELEVATED_SERVICE_HELPER_ARG: &str = "--mihomore-elevated-service-helper";
pub(super) const SERVICE_OWNER_PID_ARG: &str = "--owner-pid";
pub(super) const SERVICE_ADMIN_RIGHTS_SDDL: &str = "CCDCLCSWRPWPDTLOCRSDRCWDWO";
pub(super) const SERVICE_INTERACTIVE_USER_RIGHTS_SDDL: &str = "LCRPWP";

// Windows 标准访问位不属于服务模块本身；这里显式保留数值，避免为了少量 ACL 掩码引入额外
// windows-sys feature。它们分别对应 DELETE / READ_CONTROL / WRITE_DAC / WRITE_OWNER。
#[cfg(windows)]
pub(super) const STANDARD_DELETE: u32 = 0x0001_0000;
#[cfg(windows)]
pub(super) const STANDARD_READ_CONTROL: u32 = 0x0002_0000;
#[cfg(windows)]
pub(super) const STANDARD_WRITE_DAC: u32 = 0x0004_0000;
#[cfg(windows)]
pub(super) const STANDARD_WRITE_OWNER: u32 = 0x0008_0000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CoreServiceSnapshot {
    pub installed: bool,
    pub running: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreServiceAction {
    Install,
    Uninstall,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreServicePaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub cores_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl CoreServicePaths {
    pub fn from_base_dirs(config_dir: &Path, data_dir: &Path, cache_dir: &Path) -> Self {
        // 与 AppPaths 一致：剥离 Windows verbatim 前缀，否则内核服务模式下 mihomo
        // 同样无法写 `cache.db`，用户选择的节点不能持久化。
        let config_dir = air_paths::simplify_path(config_dir);
        let data_dir = air_paths::simplify_path(data_dir);
        let cache_dir = air_paths::simplify_path(cache_dir);
        Self {
            cores_dir: cache_dir.join("core"),
            logs_dir: data_dir.join("logs"),
            config_dir,
            data_dir,
            cache_dir,
        }
    }

    pub fn resolve_default() -> AppResult<Self> {
        // 与 AppPaths 保持一致：便携优先，软件目录不可写时回退系统目录。
        let roots = air_paths::AppDirRoots::resolve()?;
        Ok(Self::from_base_dirs(
            &roots.config_dir,
            &roots.data_dir,
            &roots.cache_dir,
        ))
    }

    pub(super) fn init(&self) -> AppResult<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.cores_dir,
            &self.logs_dir,
        ] {
            std::fs::create_dir_all(dir).map_err(air_error::StorageError::Io)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归测试：内核服务模式下 `cores_dir` 同样会成为 mihomo 的 `-d`，
    /// 带 verbatim 前缀时无法写 `cache.db`，用户选择的节点不能持久化。
    #[test]
    fn verbatim_prefix_never_reaches_the_service_core_working_dir() {
        let raw = Path::new(r"\\?\D:\mihomore");

        let paths = CoreServicePaths::from_base_dirs(
            &raw.join("config"),
            &raw.join("data"),
            &raw.join("cache"),
        );

        for dir in [&paths.cores_dir, &paths.config_dir, &paths.logs_dir] {
            let text = dir.to_string_lossy();
            assert!(!text.starts_with(r"\\?\"), "仍带 verbatim 前缀: {text}");
            assert!(!text.contains('/'), "不应出现正斜杠: {text}");
        }
        assert_eq!(paths.cores_dir, PathBuf::from(r"D:\mihomore\cache\core"));
    }
}

impl CoreServiceAction {
    pub(super) fn as_arg(self) -> &'static str {
        match self {
            Self::Install => "--install",
            Self::Uninstall => "--uninstall",
        }
    }

    pub(super) fn from_arg(value: &str) -> Option<Self> {
        match value {
            "--install" => Some(Self::Install),
            "--uninstall" => Some(Self::Uninstall),
            _ => None,
        }
    }
}
