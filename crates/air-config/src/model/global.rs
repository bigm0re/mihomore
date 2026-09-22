use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use super::{
    ExtensionMap, StringValueMap, empty_map_if_null, empty_vec_if_null, string_vec_if_null,
};

/// 顶层全局配置。
///
/// 常用字段直接建模，GUI 可安全展示和编辑；实验性字段、平台相关字段和新版字段会进入
/// `extensions`，由高级配置编辑器或 YAML 编辑器处理。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct GlobalConfig {
    pub port: Option<u32>,
    pub socks_port: Option<u32>,
    pub mixed_port: Option<u32>,
    pub redir_port: Option<u32>,
    pub tproxy_port: Option<u32>,
    pub allow_lan: Option<bool>,
    pub bind_address: Option<String>,
    pub authentication: Vec<String>,
    pub skip_auth_prefixes: Vec<String>,
    /// mihomo 把该字段当作**入站 IP 白名单**：字段缺失表示不限制（默认 `0.0.0.0/0` 与 `::/0`），
    /// 而空数组表示"白名单为空"，会拒绝包括 `127.0.0.1` 在内的**所有**连接，
    /// 使 mixed-port 上的代理整体失效。因此空列表必须省略，绝不能序列化成 `lan-allowed-ips: []`。
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "empty_vec_if_null"
    )]
    pub lan_allowed_ips: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "empty_vec_if_null"
    )]
    pub lan_disallowed_ips: Vec<String>,
    pub find_process_mode: Option<String>,
    pub mode: Option<String>,
    pub log_level: Option<String>,
    pub ipv6: Option<bool>,
    pub keep_alive_interval: Option<u64>,
    pub keep_alive_idle: Option<u64>,
    pub disable_keep_alive: Option<bool>,
    pub unified_delay: Option<bool>,
    pub tcp_concurrent: Option<bool>,
    pub geodata_mode: Option<bool>,
    pub geodata_loader: Option<String>,
    pub geox_url: Option<GeoxUrlConfig>,
    pub geo_auto_update: Option<bool>,
    pub geo_update_interval: Option<u64>,
    pub geosite_matcher: Option<String>,
    pub external_controller: Option<String>,
    pub external_controller_cors: Option<ExternalControllerCorsConfig>,
    pub secret: Option<String>,
    pub external_ui: Option<String>,
    pub external_ui_name: Option<String>,
    pub external_ui_url: Option<String>,
    pub external_doh_server: Option<String>,
    pub interface_name: Option<String>,
    pub routing_mark: Option<Value>,
    pub global_ua: Option<String>,
    #[serde(deserialize_with = "empty_map_if_null")]
    pub hosts: StringValueMap,
    #[serde(deserialize_with = "empty_map_if_null")]
    pub experimental: StringValueMap,
}

/// geodata 下载地址配置。字段较稳定，适合作为全局设置中的高级可编辑项。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct GeoxUrlConfig {
    pub geoip: Option<String>,
    pub geosite: Option<String>,
    pub mmdb: Option<String>,
    pub asn: Option<String>,
    #[serde(flatten)]
    pub extensions: ExtensionMap,
}

/// RESTful API CORS 配置。此项影响控制接口暴露范围，后续 UI 应作为高级/安全设置展示。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ExternalControllerCorsConfig {
    pub allow_origins: Vec<String>,
    pub allow_private_network: Option<bool>,
    #[serde(flatten)]
    pub extensions: ExtensionMap,
}

/// API 证书配置。密钥类字段只建模不记录日志，具体脱敏由 telemetry 层负责。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TlsConfig {
    pub certificate: Option<String>,
    pub private_key: Option<String>,
    pub client_auth_type: Option<String>,
    pub client_auth_cert: Option<String>,
    pub ech_key: Option<String>,
    pub custom_certifactes: Vec<String>,
    #[serde(flatten)]
    pub extensions: ExtensionMap,
}

/// 运行记录持久化配置。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ProfileConfig {
    pub store_selected: Option<bool>,
    pub store_fake_ip: Option<bool>,
    #[serde(flatten)]
    pub extensions: ExtensionMap,
}

/// TUN 配置。Linux/Android 等平台差异字段很多，因此只把常用字段类型化，其余交给扩展映射。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TunConfig {
    pub enable: Option<bool>,
    pub stack: Option<String>,
    pub device: Option<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub dns_hijack: Vec<String>,
    pub auto_detect_interface: Option<bool>,
    pub auto_route: Option<bool>,
    pub auto_redirect: Option<bool>,
    pub strict_route: Option<bool>,
    pub mtu: Option<u32>,
    pub gso: Option<bool>,
    pub gso_max_size: Option<u32>,
    pub inet6_address: Option<String>,
    pub udp_timeout: Option<u64>,
    pub iproute2_table_index: Option<u32>,
    pub iproute2_rule_index: Option<u32>,
    pub endpoint_independent_nat: Option<bool>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub route_address: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub route_exclude_address: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub inet4_address: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub route_address_set: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub route_exclude_address_set: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub include_interface: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub exclude_interface: Vec<String>,
    #[serde(flatten)]
    pub extensions: ExtensionMap,
}

/// 域名嗅探配置。协议细节随 mihomo 版本扩展，`sniff` 内部保持 YAML 值映射。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SnifferConfig {
    pub enable: Option<bool>,
    pub force_dns_mapping: Option<bool>,
    pub parse_pure_ip: Option<bool>,
    pub override_destination: Option<bool>,
    #[serde(deserialize_with = "empty_map_if_null")]
    pub sniff: StringValueMap,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub force_domain: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub skip_domain: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub skip_src_address: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub skip_dst_address: Vec<String>,
    #[serde(deserialize_with = "empty_vec_if_null")]
    pub sniffing: Vec<String>,
    #[serde(deserialize_with = "string_vec_if_null")]
    pub port_whitelist: Vec<String>,
    #[serde(flatten)]
    pub extensions: ExtensionMap,
}

/// tunnel 支持一行字符串和完整 YAML 两种写法，必须保留原始形状。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TunnelConfig {
    Shorthand(String),
    Structured(TunnelObject),
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self::Shorthand(String::new())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TunnelObject {
    pub network: Value,
    pub address: Option<String>,
    pub target: Option<String>,
    pub proxy: Option<String>,
    #[serde(flatten)]
    pub extensions: ExtensionMap,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归测试：`lan-allowed-ips` 是 mihomo 的**入站白名单**，空数组会拒绝所有连接
    /// （包括 `127.0.0.1`），导致代理端口整体失效。因此空列表必须被省略，
    /// 绝不能序列化成 `lan-allowed-ips: []`。
    #[test]
    fn empty_lan_allowed_ips_is_omitted_when_serializing() {
        let global = GlobalConfig::default();
        assert!(global.lan_allowed_ips.is_empty());

        let value = serde_yaml::to_value(&global).expect("global config should serialize");
        let mapping = value
            .as_mapping()
            .expect("global config should be a mapping");

        assert!(
            !mapping.contains_key(Value::String("lan-allowed-ips".to_string())),
            "空白的 lan-allowed-ips 必须省略；写成 [] 会让 mihomo 拒绝所有入站连接"
        );
        // 黑名单空数组语义等同“不封禁”，同样没有写出的必要，保持对称省略。
        assert!(!mapping.contains_key(Value::String("lan-disallowed-ips".to_string())));
    }

    /// 非空白名单必须原样保留，否则会丢失用户配置的访问控制意图。
    #[test]
    fn non_empty_lan_allowed_ips_is_preserved() {
        let global = GlobalConfig {
            lan_allowed_ips: vec!["0.0.0.0/0".to_string(), "::/0".to_string()],
            ..GlobalConfig::default()
        };

        let value = serde_yaml::to_value(&global).expect("global config should serialize");
        let mapping = value
            .as_mapping()
            .expect("global config should be a mapping");
        let items = mapping
            .get(Value::String("lan-allowed-ips".to_string()))
            .and_then(Value::as_sequence)
            .expect("lan-allowed-ips should be serialized as a sequence");

        assert_eq!(items.len(), 2);
    }

    /// 回归测试：这是**真实的故障场景**。用户从其他 Clash 客户端拿来的配置只有
    /// `allow-lan: true`，**根本没有 `lan-allowed-ips` 字段**。旧版 mihomore 因为
    /// `Vec<String>` 默认空且缺少 `skip_serializing_if`，会在保存时凭空写入
    /// `lan-allowed-ips: []`，把"不限制"改成"拒绝所有"，使代理整体失效。
    #[test]
    fn config_without_lan_allowed_ips_does_not_gain_the_field() {
        // 用户原始配置的形态：有 allow-lan，但没有 lan-allowed-ips。
        let yaml = "mixed-port: 7890\nallow-lan: true\nmode: rule\nfind-process-mode: strict\n";

        let document: crate::model::MihomoConfigDocument =
            serde_yaml::from_str(yaml).expect("user-shaped config should parse");
        assert!(
            document.global.lan_allowed_ips.is_empty(),
            "缺失的字段应反序列化为空列表"
        );

        let serialized = serde_yaml::to_string(&document).expect("document should serialize");

        assert!(
            !serialized.contains("lan-allowed-ips"),
            "不能凭空添加 lan-allowed-ips：空数组会让 mihomo 拒绝所有入站连接\n{serialized}"
        );
        // 用户原有的字段必须原样保留，不能顺手改掉。
        assert!(serialized.contains("allow-lan: true"), "{serialized}");
        assert!(
            serialized.contains("find-process-mode: strict"),
            "{serialized}"
        );
    }

    /// 反向确认：用户显式写下的空白名单也应被省略，而不是写成 `[]`。
    #[test]
    fn explicit_empty_lan_allowed_ips_is_also_omitted() {
        let yaml = "mixed-port: 7890\nallow-lan: true\nlan-allowed-ips: []\n";

        let document: crate::model::MihomoConfigDocument =
            serde_yaml::from_str(yaml).expect("config should parse");
        let serialized = serde_yaml::to_string(&document).expect("document should serialize");

        assert!(!serialized.contains("lan-allowed-ips"), "{serialized}");
    }
}
