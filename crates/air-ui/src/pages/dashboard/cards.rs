//! 仪表盘卡片的展示投影与标签计算。
//!
//! 这里把 `AppSnapshot` 中的原始数据整理成「卡片要显示的字符串」，不涉及绘制，
//! 因此可以单测标签、状态归一化和运行时长等容易出错的分支。

use air_app::{AppSnapshot, RuntimeStatus};
use air_platform::system_proxy::SystemProxyState;

/// 内核启动按钮的状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreControlState {
    /// 未运行，按钮提示启动。
    Idle,
    /// 正在启动或停止，按钮禁用。
    Pending,
    /// 正在运行，按钮显示已运行时长。
    Running,
}

impl CoreControlState {
    pub fn from_runtime(status: &RuntimeStatus) -> Self {
        match status {
            RuntimeStatus::Running => Self::Running,
            RuntimeStatus::Starting | RuntimeStatus::Stopping => Self::Pending,
            RuntimeStatus::Idle | RuntimeStatus::Failed { .. } => Self::Idle,
        }
    }

    /// 按钮是否可点击。
    pub fn is_clickable(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

/// 内核按钮上的文字。
///
/// - 未运行时提示「启动内核」。
/// - 运行中显示已运行时长（`00:07:59`）。
/// - 过渡态显示进行中的动作，避免用户重复点击。
///
/// 运行中但缺少启动时间时（例如外部启动的核心）回退到「运行中」，
/// 而不是显示 `00:00:00` 造成"刚启动"的误解。
pub fn core_control_label(state: CoreControlState, elapsed_seconds: Option<i64>) -> String {
    match state {
        CoreControlState::Idle => "启动内核".to_string(),
        CoreControlState::Pending => "处理中".to_string(),
        CoreControlState::Running => match elapsed_seconds {
            Some(seconds) => super::format::format_elapsed(seconds),
            None => "运行中".to_string(),
        },
    }
}

/// 从快照计算内核已运行秒数。
///
/// 仅在运行中才返回时长；未运行时返回 `None`，避免停止后仍显示残留时间。
pub fn core_elapsed_seconds(snapshot: &AppSnapshot, now_unix: i64) -> Option<i64> {
    if !matches!(snapshot.runtime, RuntimeStatus::Running) {
        return None;
    }
    snapshot
        .core_running_since_unix
        .map(|start| super::format::elapsed_since(start, now_unix))
}

/// 出站模式（rule / global / direct）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundMode {
    Rule,
    Global,
    Direct,
}

impl OutboundMode {
    /// 全部模式，用于渲染三选一列表。
    pub const ALL: [Self; 3] = [Self::Rule, Self::Global, Self::Direct];

    /// 归一化 mihomo 的 mode 字符串。
    ///
    /// 与状态栏保持一致：未知值一律按「规则」处理，避免配置被写坏时界面出现空选项。
    pub fn from_mode_value(mode: &str) -> Self {
        match mode.trim().to_ascii_lowercase().as_str() {
            "global" => Self::Global,
            "direct" => Self::Direct,
            _ => Self::Rule,
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Global => "global",
            Self::Direct => "direct",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rule => "规则",
            Self::Global => "全局",
            Self::Direct => "直连",
        }
    }
}

/// 系统代理卡片的展示数据。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemProxyCard {
    /// 开关的选中状态：直接取系统真实状态，而不是 GUI 意图值。
    pub enabled: bool,
    /// 卡片副标题。
    pub detail: String,
    /// 是否允许交互（平台不支持时禁用）。
    pub interactive: bool,
    /// 写入或读取时探测代理端口是否可连接：
    /// - `Some(true)` = 端口在线
    /// - `Some(false)` = 端口无响应（需提示用户检查 mihomo）
    /// - `None` = 未探测或未启用
    pub port_alive: Option<bool>,
}

impl SystemProxyCard {
    /// 从系统代理投影构造卡片。
    ///
    /// 关闭时展示「已关闭」；开启时展示实际代理地址，让用户能核对是否指向本机内核。
    /// 端口不在线时，detail 改为警告文案，让「代理不能生效」这类问题立刻可见。
    pub fn from_state(state: &SystemProxyState) -> Self {
        if !state.supported {
            return Self {
                enabled: false,
                detail: "当前平台不支持".to_string(),
                interactive: false,
                port_alive: None,
            };
        }
        let (detail, port_alive) = if state.enabled {
            match state.port_alive {
                Some(true) => {
                    if state.server.is_empty() {
                        ("已开启".to_string(), Some(true))
                    } else {
                        (state.server.clone(), Some(true))
                    }
                }
                Some(false) => {
                    // 写入了注册表但端口无响应：通常是 mihomo 没跑或 mixed-port 未启用。
                    let base = if state.server.is_empty() {
                        "已开启".to_string()
                    } else {
                        state.server.clone()
                    };
                    (format!("{base} · 内核未响应"), Some(false))
                }
                None => {
                    if state.server.is_empty() {
                        ("已开启".to_string(), None)
                    } else {
                        (state.server.clone(), None)
                    }
                }
            }
        } else {
            ("已关闭".to_string(), state.port_alive)
        };
        Self {
            enabled: state.enabled,
            detail,
            interactive: true,
            port_alive,
        }
    }
}

/// 内网 IP 卡片的展示文本。
pub fn local_ip_text(snapshot: &AppSnapshot) -> String {
    snapshot
        .network
        .local_ip
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "未知".to_string())
}

/// 公网地址卡片的展示文本。
///
/// 优先显示地址；探测失败时显示错误摘要，两者都没有时提示尚未检测。
pub fn public_address_text(probe: &air_app::events::PublicAddressProbe) -> String {
    if let Some(address) = probe
        .address
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        return address.clone();
    }
    if let Some(error) = probe
        .error
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        return format!("检测失败：{error}");
    }
    "尚未检测".to_string()
}

/// 内核状态卡片的展示文本（供状态栏和仪表盘共用）。
///
/// 状态栏已有同名逻辑；这里只在测试中验证两处文案一致，避免重复维护两份文案。
#[cfg(test)]
pub fn core_status_text(status: &RuntimeStatus) -> &'static str {
    match status {
        RuntimeStatus::Idle => "未启动",
        RuntimeStatus::Starting => "启动中",
        RuntimeStatus::Running => "运行中",
        RuntimeStatus::Stopping => "停止中",
        RuntimeStatus::Failed { .. } => "异常",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use air_app::events::PublicAddressProbe;
    #[test]
    fn core_control_label_prompts_start_when_idle() {
        assert_eq!(core_control_label(CoreControlState::Idle, None), "启动内核");
    }

    #[test]
    fn core_control_label_shows_transition_state_while_pending() {
        assert_eq!(
            core_control_label(CoreControlState::Pending, None),
            "处理中"
        );
    }

    #[test]
    fn core_control_label_prefers_elapsed_time_while_running() {
        assert_eq!(
            core_control_label(CoreControlState::Running, Some(479)),
            "00:07:59"
        );
    }

    #[test]
    fn core_control_label_falls_back_when_running_without_start_time() {
        // 外部启动的核心没有启动时间戳；不能显示 00:00:00 让人误以为刚启动。
        assert_eq!(
            core_control_label(CoreControlState::Running, None),
            "运行中"
        );
    }

    #[test]
    fn pending_state_is_not_clickable() {
        assert!(!CoreControlState::Pending.is_clickable());
        assert!(CoreControlState::Idle.is_clickable());
        assert!(CoreControlState::Running.is_clickable());
    }

    #[test]
    fn core_elapsed_is_none_when_start_time_missing() {
        let snapshot = AppSnapshot {
            runtime: RuntimeStatus::Running,
            core_running_since_unix: None,
            ..AppSnapshot::default()
        };

        assert_eq!(core_elapsed_seconds(&snapshot, 1000), None);
    }

    #[test]
    fn core_elapsed_is_none_when_not_running() {
        // 停止后必须清空时长，否则按钮会继续显示上一次运行的累计时间。
        let snapshot = AppSnapshot {
            runtime: RuntimeStatus::Idle,
            core_running_since_unix: Some(500),
            ..AppSnapshot::default()
        };

        assert_eq!(core_elapsed_seconds(&snapshot, 1000), None);
    }

    #[test]
    fn core_elapsed_computes_seconds_while_running() {
        let snapshot = AppSnapshot {
            runtime: RuntimeStatus::Running,
            core_running_since_unix: Some(1000),
            ..AppSnapshot::default()
        };

        assert_eq!(core_elapsed_seconds(&snapshot, 1060), Some(60));
    }

    #[test]
    fn core_elapsed_clamps_future_timestamps_to_zero() {
        let snapshot = AppSnapshot {
            runtime: RuntimeStatus::Running,
            core_running_since_unix: Some(2000),
            ..AppSnapshot::default()
        };

        assert_eq!(core_elapsed_seconds(&snapshot, 1000), Some(0));
    }

    #[test]
    fn outbound_mode_normalization_matches_status_bar() {
        // 与状态栏 `normalized_mode` 保持一致，避免同一配置在两处显示不同模式。
        assert_eq!(OutboundMode::from_mode_value("rule"), OutboundMode::Rule);
        assert_eq!(
            OutboundMode::from_mode_value("GLOBAL"),
            OutboundMode::Global
        );
        assert_eq!(
            OutboundMode::from_mode_value(" direct "),
            OutboundMode::Direct
        );
        assert_eq!(OutboundMode::from_mode_value("unknown"), OutboundMode::Rule);
    }

    #[test]
    fn outbound_mode_values_round_trip() {
        for mode in OutboundMode::ALL {
            assert_eq!(OutboundMode::from_mode_value(mode.value()), mode);
            assert!(!mode.label().is_empty());
        }
    }

    #[test]
    fn system_proxy_card_body_shows_applied_state_first() {
        // 卡片必须展示系统真实状态，而不是用户上次的意图，否则会出现"开关是开的但代理没生效"。
        let state = SystemProxyState {
            supported: true,
            enabled: true,
            server: "127.0.0.1:7890".to_string(),
            port_alive: None,
        };

        let card = SystemProxyCard::from_state(&state);

        assert!(card.enabled);
        assert!(card.interactive);
        assert_eq!(card.detail, "127.0.0.1:7890");
        assert_eq!(card.port_alive, None);
    }

    #[test]
    fn system_proxy_card_reports_disabled_state() {
        let state = SystemProxyState {
            supported: true,
            enabled: false,
            server: "127.0.0.1:7890".to_string(),
            port_alive: None,
        };

        let card = SystemProxyCard::from_state(&state);

        assert!(!card.enabled);
        assert_eq!(card.detail, "已关闭");
    }

    #[test]
    fn system_proxy_card_is_disabled_on_unsupported_platforms() {
        let card = SystemProxyCard::from_state(&SystemProxyState::unsupported());

        assert!(!card.enabled);
        assert!(!card.interactive);
        assert_eq!(card.detail, "当前平台不支持");
        assert_eq!(card.port_alive, None);
    }

    #[test]
    fn system_proxy_card_handles_enabled_without_server() {
        let state = SystemProxyState {
            supported: true,
            enabled: true,
            server: String::new(),
            port_alive: None,
        };

        assert_eq!(SystemProxyCard::from_state(&state).detail, "已开启");
    }

    #[test]
    fn system_proxy_card_warns_when_port_unreachable() {
        // 这是“代理不能生效”主诉的可视化：注册表已开启、写了地址，但端口无响应。
        let state = SystemProxyState {
            supported: true,
            enabled: true,
            server: "127.0.0.1:7890".to_string(),
            port_alive: Some(false),
        };

        let card = SystemProxyCard::from_state(&state);

        assert!(card.enabled);
        assert!(card.port_alive == Some(false));
        assert!(
            card.detail.contains("内核未响应"),
            "detail 应明确提示“内核未响应”，实际为 {}",
            card.detail
        );
    }

    #[test]
    fn local_ip_falls_back_to_placeholder() {
        let snapshot = AppSnapshot::default();
        assert_eq!(local_ip_text(&snapshot), "未知");

        let snapshot = AppSnapshot {
            network: air_app::events::NetworkProbeSnapshot {
                local_ip: Some("   ".to_string()),
                ..Default::default()
            },
            ..AppSnapshot::default()
        };
        assert_eq!(local_ip_text(&snapshot), "未知");
    }

    #[test]
    fn public_address_prefers_address_over_error() {
        assert_eq!(
            public_address_text(&PublicAddressProbe::resolved("1.2.3.4")),
            "1.2.3.4"
        );
    }

    #[test]
    fn public_address_surfaces_probe_error() {
        assert_eq!(
            public_address_text(&PublicAddressProbe::failed("超时")),
            "检测失败：超时"
        );
    }

    #[test]
    fn public_address_prompts_when_never_probed() {
        assert_eq!(
            public_address_text(&PublicAddressProbe::default()),
            "尚未检测"
        );
    }

    #[test]
    fn core_status_text_covers_every_runtime_state() {
        assert_eq!(core_status_text(&RuntimeStatus::Idle), "未启动");
        assert_eq!(core_status_text(&RuntimeStatus::Starting), "启动中");
        assert_eq!(core_status_text(&RuntimeStatus::Running), "运行中");
        assert_eq!(core_status_text(&RuntimeStatus::Stopping), "停止中");
        assert_eq!(
            core_status_text(&RuntimeStatus::Failed {
                message: "x".into()
            }),
            "异常"
        );
    }
}
