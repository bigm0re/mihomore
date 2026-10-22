//! 验证 `allow-lan` 在 mihomore 配置读写链路中的行为。
//!
//! 背景：mihomo 的 `allow-lan` 默认是 **false**，只有配置文件显式写
//! `allow-lan: true` 才会开启。若 mihomore 写出的运行配置丢失该字段，
//! mihomo 重启后就会回落到 false（用户观察到「重启后 allow-lan 回落」）。

use std::fs;

use air_storage::{AppPaths, CoreConfigStore};

fn store_with(config: &str) -> (tempfile::TempDir, CoreConfigStore, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("临时目录应创建成功");
    let paths = AppPaths::from_base_dirs(
        &temp.path().join("config"),
        &temp.path().join("data"),
        &temp.path().join("cache"),
    );
    paths.init().expect("目录结构应初始化成功");
    let common = temp.path().join("config/core.common.config.yaml");
    fs::write(&common, config).expect("初始配置应可写入");
    (temp, CoreConfigStore::new(paths), common)
}

/// 用户显式设置的 `allow-lan: true` 必须能完整往返。
#[test]
fn explicit_allow_lan_true_survives_save() {
    let (_t, store, common) = store_with("mixed-port: 7890\nallow-lan: true\nbind-address: '*'\n");

    let doc = store.load_user_config().expect("配置应能加载");
    assert_eq!(doc.typed.global.allow_lan, Some(true));

    store.save_user_config(&doc).expect("配置应能保存");
    let saved = fs::read_to_string(&common).expect("应可读取");

    assert!(
        saved.contains("allow-lan: true"),
        "allow-lan: true 被丢弃，重启 mihomo 后会回落为 false\n{saved}"
    );
    assert!(saved.contains("bind-address"), "{saved}");
}

/// 运行配置（真正喂给 mihomo 的那份）同样必须保留 allow-lan。
#[test]
fn explicit_allow_lan_true_reaches_runtime_config() {
    let (_t, store, _common) = store_with("mixed-port: 7890\nallow-lan: true\nbind-address: '*'\n");

    let doc = store.load_user_config().expect("配置应能加载");
    let runtime_path = store
        .write_runtime_config(&doc.typed, &[])
        .expect("运行配置应能写出");
    let runtime = fs::read_to_string(&runtime_path).expect("应可读取");

    assert!(
        runtime.contains("allow-lan: true"),
        "运行配置丢失 allow-lan，mihomo 重启即回落\n{runtime}"
    );
}

/// 内置默认配置必须显式声明 allow-lan，否则 mihomo 会用自己的默认值，
/// 让「本机软件配置」失去权威性（这正是「重启后回落」的根源）。
#[test]
fn default_config_pins_allow_lan_explicitly() {
    let temp = tempfile::tempdir().expect("临时目录应创建成功");
    let paths = AppPaths::from_base_dirs(
        &temp.path().join("config"),
        &temp.path().join("data"),
        &temp.path().join("cache"),
    );
    paths.init().expect("目录结构应初始化成功");
    let store = CoreConfigStore::new(paths);

    let doc = store.load_user_config().expect("应能加载内置默认配置");
    assert_eq!(
        doc.typed.global.allow_lan,
        Some(false),
        "默认配置必须显式写 allow-lan: false，而不是留给 mihomo 默认值"
    );

    let runtime_path = store
        .write_runtime_config(&doc.typed, &[])
        .expect("运行配置应能写出");
    let runtime = fs::read_to_string(&runtime_path).expect("应可读取");

    assert!(
        runtime.contains("allow-lan: false"),
        "运行配置必须携带显式的 allow-lan，否则内核重启会回落\n{runtime}"
    );
    assert!(runtime.contains("bind-address"), "{runtime}");
}
