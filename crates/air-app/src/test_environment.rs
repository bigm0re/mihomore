//! 测试环境探测。
//!
//! # 为什么需要运行期探测
//!
//! `#[cfg(test)]` 只在**本 crate 自身的单元测试**里为真。当 `air-ui` 的测试把
//! `air-app` 当作普通依赖链接时，`cfg(test)` 在 `air-app` 内部是 `false`，
//! 所有基于它的守卫会**静默失效**。
//!
//! 这不是理论问题：`air-ui` 的测试会构造 `AppServices::with_paths`，其启动路径
//! 会调用系统代理回收逻辑。历史上一旦守卫失效，跑一次 `cargo test` 就会把开发机
//! 真实的 `ProxyEnable` 写成 0，表现为「跑完测试后浏览器不走代理了」。
//!
//! 因此这里改为运行期判断：只要能确认「当前进程是测试二进制」，就拒绝任何真实
//! 系统副作用。判断必须同时覆盖单元测试与集成测试两种运行方式。

use std::path::Path;

/// 显式强制禁用系统副作用的开关。
///
/// 用于自动化环境（CI、容器）以及无法通过路径识别的特殊测试宿主。
pub const DISABLE_SYSTEM_EFFECTS_ENV: &str = "MIHOMORE_DISABLE_SYSTEM_EFFECTS";

/// 当前进程是否运行在测试环境中。
///
/// 满足以下任一条件即判定为测试环境：
///
/// 1. `cfg!(test)` —— 本 crate 自身的单元测试；
/// 2. 可执行文件位于 Cargo 的 `deps` 目录且文件名带哈希后缀 —— 覆盖
///    `air-ui` 等下游 crate 的单元/集成测试（此时 `cfg!(test)` 为 false）；
/// 3. 显式设置了 [`DISABLE_SYSTEM_EFFECTS_ENV`]。
///
/// 结果不做缓存：测试进程不会频繁切换形态，而缓存会让「同一进程内先跑测试后跑
/// 真实逻辑」的边界变得难以推理。
pub fn is_test_environment() -> bool {
    if cfg!(test) {
        return true;
    }
    if std::env::var_os(DISABLE_SYSTEM_EFFECTS_ENV).is_some() {
        return true;
    }
    current_exe_is_test_harness()
}

/// 依据可执行文件路径判断是否为 Cargo 测试宿主。
///
/// Rust 的测试二进制由 Cargo 放在 `target/<profile>/deps/` 下，文件名为
/// `<crate>-<hash>`。真实应用二进制位于 `target/<profile>/`（例如 `mihomore.exe`），
/// 因此「父目录名为 deps」是一个稳定且无歧义的信号。
fn current_exe_is_test_harness() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    path_looks_like_test_harness(&exe)
}

fn path_looks_like_test_harness(exe: &Path) -> bool {
    let in_deps = exe
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name.eq_ignore_ascii_case("deps"));
    if !in_deps {
        return false;
    }
    // `deps/` 下既有测试二进制也有依赖库产物；测试宿主文件名形如
    // `air_ui-1a2b3c4d5e6f7890`，因此要求存在 `-<hash>` 形式的后缀。
    exe.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit_once('-'))
        .is_some_and(|(crate_name, hash)| {
            !crate_name.is_empty()
                && hash.len() >= 8
                && hash.chars().all(|ch| ch.is_ascii_hexdigit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_tests_are_detected_as_test_environment() {
        // 本 crate 的单元测试必须始终被识别，否则真实注册表会被改写。
        assert!(is_test_environment());
    }

    #[test]
    fn deps_path_with_hash_suffix_is_a_test_harness() {
        assert!(path_looks_like_test_harness(Path::new(
            r"E:\repo\target\debug\deps\air_ui-1a2b3c4d5e6f7890.exe"
        )));
        assert!(path_looks_like_test_harness(Path::new(
            "/repo/target/debug/deps/air_app-abcdef0123456789"
        )));
    }

    #[test]
    fn real_application_binary_is_not_a_test_harness() {
        // 真实产物位于 target/<profile>/ 下，不能被误判，否则系统代理功能会失效。
        assert!(!path_looks_like_test_harness(Path::new(
            r"E:\repo\target\release\mihomore.exe"
        )));
        assert!(!path_looks_like_test_harness(Path::new(
            r"D:\mihomore\mihomore.exe"
        )));
        // `deps/` 下的普通依赖产物没有哈希后缀，也不算测试宿主。
        assert!(!path_looks_like_test_harness(Path::new(
            r"E:\repo\target\debug\deps\air_paths.dll"
        )));
    }
}
