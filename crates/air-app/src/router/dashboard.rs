use air_app::events::AppNotificationLevel;

use super::context::CommandExecutionContext;
use super::shared::ensure_not_canceled;

/// 处理仪表盘「系统代理」开关。
///
/// UI 只表达用户意图；真实读写系统注册表、回收策略和持久化都在 app/service 层完成。
/// 关闭时若发现代理不是本程序写入的，`apply_system_proxy` 会返回错误，这里转成
/// 用户可见提示，避免开关静默失败。
pub(super) async fn handle_set_system_proxy_enabled(
    context: &CommandExecutionContext,
    enabled: bool,
) -> air_error::AppResult<()> {
    ensure_not_canceled(&context.token)?;
    tracing::info!(enabled, "handling dashboard system proxy toggle");
    match context.services.apply_system_proxy(enabled) {
        Ok(state) => {
            context.services.emit_notification(
                AppNotificationLevel::Success,
                if state.enabled {
                    format!("系统代理已开启（{}）", state.server)
                } else {
                    "系统代理已关闭".to_string()
                },
            );
            Ok(())
        }
        Err(error) => {
            // 写盘失败后必须把权威状态重新投影回快照，否则 UI 开关会停留在用户点击后的位置。
            if let Err(refresh_error) = context.services.refresh_system_proxy_projection() {
                tracing::warn!(
                    error = %refresh_error,
                    "failed to refresh system proxy projection after toggle failure"
                );
            }
            Err(error)
        }
    }
}

/// 从注册表重新读取系统代理状态。
///
/// 用户在系统设置里手动改动代理后，仪表盘需要能跟随显示真实状态，因此启动、
/// 核心启停和窗口重新聚焦都会派发本命令。
pub(super) async fn handle_refresh_system_proxy(
    context: &CommandExecutionContext,
) -> air_error::AppResult<()> {
    ensure_not_canceled(&context.token)?;
    context.services.refresh_system_proxy_projection()?;
    Ok(())
}

/// 处理仪表盘「网络检测」：读取内网 IP 并探测公网地址。
pub(super) async fn handle_probe_dashboard_network(
    context: &CommandExecutionContext,
) -> air_error::AppResult<()> {
    ensure_not_canceled(&context.token)?;
    tracing::info!("handling dashboard network probe command");
    context.services.probe_dashboard_network().await;
    Ok(())
}
