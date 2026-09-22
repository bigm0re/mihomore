use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use air_mihomo::MihomoRuntimeInfo;
use air_mihomo::groups::ProxyGroupRuntimeProjection;
use air_mihomo::streams::StreamEvent;
use air_mihomo::{ConnectionsResponse, RulesResponse};
use air_platform::core_service::CoreServiceSnapshot;
use air_platform::system_proxy::SystemProxyState;

use super::command::CommandId;
use super::subscription_controller::SubscriptionStateProjection;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub runtime: RuntimeStatus,
    pub active_profile: Option<String>,
    // 运行时快照来自 MihomoService 和命令状态；UI 只读展示，不在回调里直接修改。
    #[serde(default)]
    pub runtime_info: Option<MihomoRuntimeInfo>,
    // controller 地址由当前 profile 或运行时装配注入，避免 UI 硬编码 mihomo API 入口。
    #[serde(default)]
    pub controller_addr: Option<String>,
    // 当前快照不再缓存旧的本地配置校验摘要，只保留核心服务等顶层运行态。
    #[serde(default)]
    pub core_service: CoreServiceSnapshot,
    // 系统代理是操作系统级状态，必须每次从注册表投影刷新，不能只存 GUI 开关意图。
    #[serde(default)]
    pub system_proxy: SystemProxyState,
    // 仪表盘网络检测结果；仅在用户或启动时主动探测后更新。
    #[serde(default)]
    pub network: NetworkProbeSnapshot,
    // 内核本次启动时间（Unix 秒）；仪表盘用它计算并展示运行时长。
    #[serde(default)]
    pub core_running_since_unix: Option<i64>,
    // 最近错误用于首页和状态栏展示；写入前必须完成敏感信息脱敏。
    #[serde(default)]
    pub last_error: Option<String>,
}

/// 仪表盘「网络检测」卡片的数据投影。
///
/// 内网 IP 与公网 IP 分开存储：内网 IP 读取失败不应影响公网展示，
/// 反之亦然。探测失败时保留 `error` 文本供 UI 提示，而不是丢掉整条结果。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NetworkProbeSnapshot {
    /// 本机内网 IPv4 地址。
    #[serde(default)]
    pub local_ip: Option<String>,
    #[serde(default)]
    pub ipv4: PublicAddressProbe,
    #[serde(default)]
    pub ipv6: PublicAddressProbe,
    /// 最近一次探测完成时间（Unix 秒）。
    #[serde(default)]
    pub checked_at_unix: Option<i64>,
}

/// 单个公网地址探测结果。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PublicAddressProbe {
    #[serde(default)]
    pub address: Option<String>,
    /// 探测失败原因；写入快照前必须脱敏。
    #[serde(default)]
    pub error: Option<String>,
}

impl PublicAddressProbe {
    pub fn resolved(address: impl Into<String>) -> Self {
        Self {
            address: Some(address.into()),
            error: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            address: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum RuntimeStatus {
    #[default]
    Idle,
    Starting,
    Running,
    Stopping,
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AppEvent {
    SnapshotChanged(AppSnapshot),
    // mihomo 日志、流量、内存和连接流统一由 app 层转发给 UI；真实订阅和重连策略仍由后台服务负责。
    MihomoStreamEvent(StreamEvent),
    OverridePreviewGenerated {
        contents: String,
    },
    // /connections 的一次性 HTTP 刷新结果独立成事件，避免命令路由拿到响应后被丢弃。
    ConnectionsStateChanged(ConnectionsResponse),
    // `/rules` 返回的是 mihomo 当前运行态规则链；禁用状态同样只属于内核运行期，
    // UI 接到事件后刷新列表，不把该状态写回用户 YAML。
    RulesStateChanged(RulesResponse),
    // 代理组配置来自本地 YAML；运行态选择和 API 展开的成员由命令路由刷新后回填。
    ProxyGroupStateChanged(ProxyGroupRuntimeProjection),
    // 测速结果只携带被测目标和延迟值，页面按自己的成员索引映射到可见卡片。
    ProxyDelayMeasured {
        name: String,
        delay_ms: u64,
    },
    ProxyGroupDelayMeasured {
        name: String,
        member_delays: BTreeMap<String, u64>,
    },
    SubscriptionStateChanged(SubscriptionStateProjection),
    // 仪表盘系统代理开关的权威状态来自注册表；每次读写后回填此事件。
    SystemProxyStateChanged(SystemProxyState),
    // 仪表盘网络检测（内网/公网 IP）结果。
    NetworkProbeCompleted(NetworkProbeSnapshot),
    SubscriptionYamlLoaded {
        subscription_id: String,
        contents: String,
    },
    SubscriptionUpdateCanceled {
        subscription_id: String,
    },
    CommandStarted {
        id: CommandId,
    },
    CommandFinished {
        id: CommandId,
    },
    RuntimeStatusChanged(RuntimeStatus),
    UserVisibleError {
        message: String,
    },
    UserNotification {
        level: AppNotificationLevel,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AppNotificationLevel {
    Info,
    Success,
    Warning,
}

impl AppEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SnapshotChanged(_) => "SnapshotChanged",
            Self::MihomoStreamEvent(_) => "MihomoStreamEvent",
            Self::OverridePreviewGenerated { .. } => "OverridePreviewGenerated",
            Self::ConnectionsStateChanged(_) => "ConnectionsStateChanged",
            Self::RulesStateChanged(_) => "RulesStateChanged",
            Self::ProxyGroupStateChanged(_) => "ProxyGroupStateChanged",
            Self::ProxyDelayMeasured { .. } => "ProxyDelayMeasured",
            Self::ProxyGroupDelayMeasured { .. } => "ProxyGroupDelayMeasured",
            Self::SubscriptionStateChanged(_) => "SubscriptionStateChanged",
            Self::SystemProxyStateChanged(_) => "SystemProxyStateChanged",
            Self::NetworkProbeCompleted(_) => "NetworkProbeCompleted",
            Self::SubscriptionYamlLoaded { .. } => "SubscriptionYamlLoaded",
            Self::SubscriptionUpdateCanceled { .. } => "SubscriptionUpdateCanceled",
            Self::CommandStarted { .. } => "CommandStarted",
            Self::CommandFinished { .. } => "CommandFinished",
            Self::RuntimeStatusChanged(_) => "RuntimeStatusChanged",
            Self::UserVisibleError { .. } => "UserVisibleError",
            Self::UserNotification { .. } => "UserNotification",
        }
    }

    pub fn log_payload(&self) -> String {
        // 事件日志记录后台推送给 UI 的完整事件体，便于对齐 UI reducer 的输入。
        // 如果后续新增不可序列化字段，降级 Debug 文本也不能阻断事件广播。
        serde_json::to_string(self).unwrap_or_else(|_| format!("{self:?}"))
    }
}
