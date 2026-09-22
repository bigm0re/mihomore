//! 回归测试：mihomore 不能凭空写出 `lan-allowed-ips: []`。
//!
//! # 背景（真实故障）
//!
//! 用户从其他 Clash 客户端拿到的配置只有 `allow-lan: true`，**没有** `lan-allowed-ips`
//! 字段。旧版 mihomore 每次保存都会把整份配置重新序列化，而 `GlobalConfig.lan_allowed_ips`
//! 是 `Vec<String>` 且缺少 `skip_serializing_if`，于是**凭空写入** `lan-allowed-ips: []`。
//!
//! 在 mihomo 中该字段是**入站 IP 白名单**：
//! - 字段**缺失** → 不限制（默认 `0.0.0.0/0` 与 `::/0`）
//! - **空数组** → 白名单为空 → 拒绝包括 `127.0.0.1` 在内的**所有**入站连接
//!
//! 结果：mixed-port 上的代理整体失效（连接被立即重置），而其他 Clash 客户端读取同一份
//! 原始配置时完全正常——这正是"只有 mihomore 有问题"的原因。
//!
//! 这些测试覆盖：真实配置往返、全新安装 + 导入订阅、以及显式空值的处理。

use std::fs;
use std::path::PathBuf;

use air_config::SubscriptionMergeInput;
use air_storage::{AppPaths, CoreConfigStore};

/// 用户配置的真实形态：有 `allow-lan`，但**没有** `lan-allowed-ips`。
/// 同时包含 emoji 代理组名，用于确认编码不被破坏。
const USER_SHAPED_CONFIG: &str = r#"mixed-port: 7890
allow-lan: true
mode: rule
log-level: info
unified-delay: true
tcp-concurrent: true
find-process-mode: strict
global-client-fingerprint: chrome

dns:
  enable: true
  listen: "127.0.0.1:5335"
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  nameserver: [223.5.5.5, "https://doh.pub/dns-query"]

profile:
  store-selected: true
  store-fake-ip: false

sniffer:
  enable: true
  parse-pure-ip: true

proxies:
  - {name: demo-node, type: hysteria2, server: 203.0.113.10, port: 31443, password: "demo", sni: example.test}

proxy-groups:
  - name: "🚀 节点选择"
    type: select
    proxies: ["⚡ 自动选择", DIRECT, demo-node]
  - name: "⚡ 自动选择"
    type: url-test
    proxies: [demo-node]
    url: "http://cp.cloudflare.com/generate_204"
    interval: 300
  - name: "🐟 漏网之鱼"
    type: select
    proxies: ["🚀 节点选择", DIRECT]

rules:
  - MATCH,🐟 漏网之鱼
"#;

/// 订阅形态的配置：用于验证「全新安装 + 导入订阅」路径。
const SUBSCRIPTION_SHAPED_CONFIG: &str = r#"proxies:
  - {name: sub-node-a, type: hysteria2, server: 203.0.113.20, port: 31443, password: "demo", sni: a.example.test}
  - {name: sub-node-b, type: hysteria2, server: 203.0.113.21, port: 31443, password: "demo", sni: b.example.test}

proxy-groups:
  - name: "🚀 节点选择"
    type: select
    proxies: ["⚡ 自动选择", DIRECT, sub-node-a, sub-node-b]
  - name: "⚡ 自动选择"
    type: url-test
    proxies: [sub-node-a, sub-node-b]
    url: "http://cp.cloudflare.com/generate_204"
    interval: 300

rules:
  - DOMAIN-SUFFIX,example.com,🚀 节点选择
  - MATCH,🚀 节点选择
"#;

/// 建立临时便携目录，并把给定配置写成用户配置。
fn store_with_config(config: &str) -> (tempfile::TempDir, CoreConfigStore, PathBuf) {
    let temp = tempfile::tempdir().expect("临时目录应创建成功");
    let paths = AppPaths::from_base_dirs(
        &temp.path().join("config"),
        &temp.path().join("data"),
        &temp.path().join("cache"),
    );
    paths.init().expect("目录结构应初始化成功");
    let common = temp.path().join("config/core.common.config.yaml");
    fs::write(&common, config).expect("应能写入初始配置");
    (temp, CoreConfigStore::new(paths), common)
}

/// 全新安装：配置目录为空，走 mihomore 内置默认配置。
fn store_fresh() -> (tempfile::TempDir, CoreConfigStore) {
    let temp = tempfile::tempdir().expect("临时目录应创建成功");
    let paths = AppPaths::from_base_dirs(
        &temp.path().join("config"),
        &temp.path().join("data"),
        &temp.path().join("cache"),
    );
    paths.init().expect("目录结构应初始化成功");
    (temp, CoreConfigStore::new(paths))
}

/// 核心回归：用户形态配置（无 `lan-allowed-ips`）保存后不能凭空多出该字段。
#[test]
fn user_config_without_lan_allowed_ips_never_gains_it() {
    let (_temp, store, common) = store_with_config(USER_SHAPED_CONFIG);

    let document = store.load_user_config().expect("配置应能加载");
    assert!(
        document.typed.global.lan_allowed_ips.is_empty(),
        "前提：缺失字段应反序列化为空列表"
    );

    store.save_user_config(&document).expect("配置应能保存");
    let saved = fs::read_to_string(&common).expect("保存后的配置应可读取");

    assert!(
        !saved.contains("lan-allowed-ips"),
        "凭空写出 lan-allowed-ips 会让 mihomo 拒绝所有入站连接，使代理整体失效\n\
         --- 落盘内容 ---\n{saved}"
    );
}

/// 用户自己的设置必须原样保留，不能被写回过程丢掉或改写。
#[test]
fn user_config_keeps_its_own_settings() {
    let (_temp, store, common) = store_with_config(USER_SHAPED_CONFIG);

    let document = store.load_user_config().expect("配置应能加载");
    store.save_user_config(&document).expect("配置应能保存");
    let saved = fs::read_to_string(&common).expect("保存后的配置应可读取");

    for expected in [
        "allow-lan: true",
        "find-process-mode: strict",
        "log-level: info",
        "unified-delay: true",
        "tcp-concurrent: true",
        "global-client-fingerprint: chrome",
        "mixed-port: 7890",
    ] {
        assert!(
            saved.contains(expected),
            "用户配置项 `{expected}` 在保存后丢失\n--- 落盘内容 ---\n{saved}"
        );
    }
    // emoji 代理组名与节点/规则结构必须完好。
    assert!(saved.contains("🚀 节点选择"), "代理组名被破坏\n{saved}");
    assert!(saved.contains("demo-node"), "节点被丢失\n{saved}");

    // 保存结果必须仍是合法 YAML 且可重新解析。
    let reparsed = store.load_user_config().expect("保存后的配置应仍可解析");
    assert!(!reparsed.typed.proxies.is_empty(), "节点列表不应被清空");
    assert!(!reparsed.typed.rules.is_empty(), "规则列表不应被清空");
}

/// 显式写下 `lan-allowed-ips: []` 时也要省略，而不是原样写成空数组。
#[test]
fn explicit_empty_lan_allowed_ips_is_dropped_on_save() {
    let config = format!("{USER_SHAPED_CONFIG}\nlan-allowed-ips: []\n");
    let (_temp, store, common) = store_with_config(&config);

    let document = store.load_user_config().expect("配置应能加载");
    store.save_user_config(&document).expect("配置应能保存");
    let saved = fs::read_to_string(&common).expect("保存后的配置应可读取");

    assert!(!saved.contains("lan-allowed-ips"), "{saved}");
}

/// 非空白名单必须保留，避免修复逻辑误删用户的访问控制配置。
#[test]
fn non_empty_lan_allowed_ips_is_preserved() {
    let (_temp, store, common) = store_with_config(USER_SHAPED_CONFIG);

    let mut document = store.load_user_config().expect("配置应能加载");
    document.typed.global.lan_allowed_ips = vec!["0.0.0.0/0".to_string(), "::/0".to_string()];
    store.save_user_config(&document).expect("配置应能保存");
    let saved = fs::read_to_string(&common).expect("保存后的配置应可读取");

    assert!(saved.contains("lan-allowed-ips"), "{saved}");
    assert!(saved.contains("0.0.0.0/0"), "{saved}");
    assert!(saved.contains("::/0"), "{saved}");
}

/// 全新安装 + 导入订阅后，**运行配置**也不能含 `lan-allowed-ips`。
///
/// 这是用户即将执行的流程（删除配置后重新导入），因此单独覆盖。
#[test]
fn fresh_install_with_subscription_runtime_config_has_no_lan_allowed_ips() {
    let (_temp, store) = store_fresh();
    let base = store.load_user_config().expect("应能加载内置默认配置");

    // 内置默认配置自身也不能含该字段。
    let default_yaml = serde_yaml::to_string(&base.typed).expect("内置默认配置应能序列化");
    assert!(
        !default_yaml.contains("lan-allowed-ips"),
        "内置默认配置不应含 lan-allowed-ips\n{default_yaml}"
    );

    let subscription =
        air_config::ConfigDocument::parse(SUBSCRIPTION_SHAPED_CONFIG).expect("订阅应能解析");
    let subscriptions = vec![SubscriptionMergeInput {
        id: "subscription-1".to_string(),
        display_name: "demo".to_string(),
        enabled: true,
        document: subscription.typed,
    }];

    let runtime_path = store
        .write_runtime_config(&base.typed, &subscriptions)
        .expect("运行配置应能写出");
    let runtime = fs::read_to_string(&runtime_path).expect("运行配置应可读取");

    assert!(
        !runtime.contains("lan-allowed-ips"),
        "运行配置含 lan-allowed-ips 会让 mihomo 拒绝所有入站连接\n\
         --- 运行配置 ---\n{runtime}"
    );
    // 订阅内容必须真的合并进运行配置，否则本测试无意义。
    assert!(
        runtime.contains("sub-node-a"),
        "订阅节点未被合并\n{runtime}"
    );
    assert!(runtime.contains("🚀 节点选择"), "代理组名被破坏\n{runtime}");
}

/// 纯默认运行配置（全新安装、尚未导入订阅）同样不能含该字段。
#[test]
fn pure_default_runtime_config_has_no_lan_allowed_ips() {
    let (_temp, store) = store_fresh();
    let base = store.load_user_config().expect("应能加载内置默认配置");

    let runtime_path = store
        .write_runtime_config(&base.typed, &[])
        .expect("运行配置应能写出");
    let runtime = fs::read_to_string(&runtime_path).expect("运行配置应可读取");

    assert!(!runtime.contains("lan-allowed-ips"), "{runtime}");
}

/// 兜底：即使绕过模型层直接构造 YAML 值树，写入管道也要剔除空白的 lan-allowed-ips。
#[test]
fn raw_value_tree_pruning_removes_empty_lan_allowed_ips() {
    // 通过真实保存路径验证兜底：手工构造含空数组的文档再落盘。
    let config = format!("{USER_SHAPED_CONFIG}\nlan-allowed-ips: []\nlan-disallowed-ips: []\n");
    let (_temp, store, common) = store_with_config(&config);

    let document = store.load_user_config().expect("配置应能加载");
    store.save_user_config(&document).expect("配置应能保存");
    let saved = fs::read_to_string(&common).expect("保存后的配置应可读取");

    assert!(!saved.contains("lan-allowed-ips"), "{saved}");
    assert!(!saved.contains("lan-disallowed-ips"), "{saved}");
    assert!(saved.contains("mixed-port: 7890"), "{saved}");
}
