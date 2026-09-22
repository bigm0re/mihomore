use super::*;

pub fn launch(force_start_core: bool, single_instance_events: Receiver<SingleInstanceEvent>) {
    gpui_platform::application()
        .with_assets(icons::AppAssets::new())
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            super::components::enforce_visible_scrollbars(cx);
            super::components::configure_global_notifications(cx);

            let window_options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
                    cx,
                )),
                titlebar: Some(main_window_titlebar_options()),
                window_decorations: Some(gpui::WindowDecorations::Client),
                window_min_size: Some(size(px(720.0), px(460.0))),
                app_id: Some("mihomore".to_string()),
                ..Default::default()
            };

            cx.spawn(async move |cx| {
                cx.open_window(window_options, |window, cx| {
                    let shell = cx
                        .new(|cx| Shell::new(window, cx, force_start_core, single_instance_events));
                    let shutdown_subscription = shell.update(cx, |_, cx| {
                        cx.on_app_quit(|shell, _| {
                            shell.stop_core_before_app_exit();
                            async {}
                        })
                    });
                    shell.update(cx, |shell, _| {
                        shell._shutdown_subscription = Some(shutdown_subscription);
                    });
                    let close_shell = shell.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        let behavior = close_shell
                            .read(cx)
                            .settings
                            .settings()
                            .close_window_behavior;
                        if behavior == CloseWindowBehavior::Tray {
                            let _ = close_shell.update(cx, |shell, cx| {
                                shell.hide_window_from_tray(window, cx);
                            });
                            return false;
                        }
                        close_shell.read(cx).stop_core_before_app_exit();
                        true
                    });
                    // gpui-component 的 Root 必须作为窗口第一层，用来承载通知、弹窗和焦点管理。
                    cx.new(|cx| Root::new(shell, window, cx))
                })
                .expect("failed to open main window");
            })
            .detach();

            cx.activate(true);
        });
}

pub(super) fn main_window_titlebar_options() -> gpui::TitlebarOptions {
    let mut options = TitleBar::title_bar_options();
    options.title = Some("mihomore".into());
    options
}

pub(super) fn create_tray() -> (TrayHandle, Receiver<TrayEvent>) {
    let options = TrayOptions {
        tooltip: "mihomore".to_string(),
        icon_png: Some(icons::brand_icon_png_bytes()),
    };
    match air_platform::tray::start_tray(options) {
        Ok((handle, events)) => {
            if handle.is_supported() {
                tracing::info!("system tray initialized");
            } else {
                tracing::info!(
                    "system tray unsupported on current platform; continuing without tray"
                );
            }
            (handle, events)
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to initialize system tray");
            let (_sender, receiver) = mpsc::channel();
            (TrayHandle::disabled(), receiver)
        }
    }
}

pub(super) fn spawn_tray_event_loop(
    receiver: Receiver<TrayEvent>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    let shell = cx.entity().clone();
    cx.spawn_in(window, async move |_, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            loop {
                match receiver.try_recv() {
                    Ok(event) => {
                        let updated = cx.update({
                            let shell = shell.clone();
                            move |window, cx| {
                                let _ = shell.update(cx, |shell, cx| {
                                    shell.handle_tray_event(event, window, cx);
                                });
                            }
                        });
                        if updated.is_err() {
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
        }
    })
    .detach();
}

pub(super) fn spawn_single_instance_event_loop(
    receiver: Receiver<SingleInstanceEvent>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    let shell = cx.entity().clone();
    cx.spawn_in(window, async move |_, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            loop {
                match receiver.try_recv() {
                    Ok(SingleInstanceEvent::ShowWindow) => {
                        let updated = cx.update({
                            let shell = shell.clone();
                            move |window, cx| {
                                let _ = shell.update(cx, |shell, cx| {
                                    tracing::info!(
                                        "single instance requested existing window restore"
                                    );
                                    shell.show_window_from_single_instance(window, cx);
                                });
                            }
                        });
                        if updated.is_err() {
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
        }
    })
    .detach();
}

pub(super) fn spawn_initial_window_hide(window: &mut Window, cx: &mut Context<Shell>) {
    // Windows 窗口需要等 GPUI 完成首次显示后再隐藏，否则 HWND 可能尚未进入可恢复状态。
    cx.spawn_in(window, async move |_, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
        let _ = cx.update(|window, _| {
            if let Err(error) = air_platform::window::hide_window(window) {
                tracing::warn!(%error, "failed to hide main window on silent startup");
            }
        });
    })
    .detach();
}

pub(super) fn spawn_tray_resource_cleanup(
    window: &mut Window,
    cx: &mut Context<Shell>,
    generation: u64,
) {
    let shell = cx.entity().clone();
    cx.spawn_in(window, async move |_, cx| {
        cx.background_executor()
            .timer(TRAY_RESOURCE_RELEASE_DELAY)
            .await;
        let _ = shell.update(cx, |shell, cx| {
            if shell.page_states_suspended_for_tray && shell.tray_cleanup_generation == generation {
                shell.destroy_all_page_states_for_tray();
                air_telemetry::memory::shrink_process_memory("tray-hide-delayed");
                cx.notify();
            }
        });
    })
    .detach();
}

pub(super) fn spawn_subscription_refresh_loop(cx: &mut Context<Shell>) {
    cx.spawn(async move |shell, cx| {
        loop {
            cx.background_executor()
                .timer(SUBSCRIPTION_REFRESH_INTERVAL)
                .await;
            let should_continue = shell
                .update(cx, |shell, cx| {
                    if shell.page_states_suspended_for_tray {
                        return true;
                    }
                    // 定时器只派发检查命令；到期判断和远程更新在 app/controller 层完成。
                    if should_dispatch_subscription_refresh(&shell.pending_commands)
                        && shell.command_router.is_some()
                    {
                        shell.dispatch_command(AppCommand::RefreshDueSubscriptions);
                        cx.notify();
                    }
                    true
                })
                .unwrap_or(false);
            if !should_continue {
                break;
            }
        }
    })
    .detach();
}

pub(super) fn should_dispatch_subscription_refresh(
    pending_commands: &BTreeMap<CommandId, AppCommand>,
) -> bool {
    !pending_commands.values().any(|command| {
        matches!(
            command,
            AppCommand::RefreshDueSubscriptions | AppCommand::UpdateSubscription { .. }
        )
    })
}

pub(super) fn should_run_connections_monitoring(
    active_route: AppRoute,
    page_states_suspended_for_tray: bool,
    runtime: &RuntimeStatus,
) -> bool {
    !page_states_suspended_for_tray
        && active_route == AppRoute::Connections
        && matches!(runtime, RuntimeStatus::Running)
}

pub(super) fn should_run_traffic_monitoring(runtime: &RuntimeStatus) -> bool {
    // 流量采样是“内核运行期统计”的数据源：仪表盘的曲线与累计流量必须覆盖整个内核运行期，
    // 因此只跟随内核是否运行，**不受当前路由和托盘隐藏影响**。
    // 若在托盘隐藏时停掉，重新打开窗口后这段流量就永久丢失，无法反映内核的真实累计用量。
    matches!(runtime, RuntimeStatus::Running)
}

/// 流式监控的期望状态迁移。
///
/// `reconcile_*_monitoring` 会在**每个 AppEvent** 上运行，因此“是否重复派发命令”是关键性质：
/// 若迁移不收敛，每个事件都会重发一次命令，形成派发风暴（历史上系统代理刷新曾因此
/// 产生过 120 万次派发）。把迁移单独建模，就能把“幂等收敛”写成可单测的不变式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MonitoringTransition {
    /// 需要启动流。
    Start,
    /// 需要停止流。
    Stop,
    /// 已经是期望状态，无需动作。
    Hold,
}

/// 根据“是否应当运行”和“当前是否在运行”推导出需要的迁移。
///
/// 收敛性：把返回的迁移应用回 `active` 后再次调用必定得到 `Hold`，
/// 因此同一状态下重复 reconcile 不会重复派发命令。
pub(super) fn monitoring_transition(should_run: bool, active: bool) -> MonitoringTransition {
    match (should_run, active) {
        (true, false) => MonitoringTransition::Start,
        (false, true) => MonitoringTransition::Stop,
        _ => MonitoringTransition::Hold,
    }
}

pub(super) fn should_run_log_monitoring(
    active_route: AppRoute,
    page_states_suspended_for_tray: bool,
) -> bool {
    !page_states_suspended_for_tray && active_route == AppRoute::Logs
}

pub(super) fn load_app_backing() -> (
    AppSettings,
    Option<AppCommandRouter>,
    AppSnapshot,
    Option<AppStateStore>,
) {
    match AppServices::new() {
        Ok(services) => {
            let settings = services.load_settings().unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to load gui settings, using defaults");
                AppSettings::default()
            });
            let snapshot = services.snapshots.snapshot();
            let snapshot_store = services.snapshots.clone();
            (
                settings,
                Some(AppCommandRouter::new(services)),
                snapshot,
                Some(snapshot_store),
            )
        }
        Err(error) => {
            tracing::warn!(%error, "failed to initialize app services, using default gui settings");
            (AppSettings::default(), None, AppSnapshot::default(), None)
        }
    }
}

pub(super) fn dispatch_startup_prepare(router: Option<&AppCommandRouter>, start_core: bool) {
    if let Some(router) = router {
        // 启动时自动完成核心检测和准备；失败通过 app event 写入快照和通知，不阻塞窗口创建。
        router.dispatch(if start_core {
            AppCommand::StartCore
        } else {
            AppCommand::PrepareCore
        });
        // 订阅定时更新在启动后立即检查一次，之后由 GPUI 定时器周期性派发同一命令。
        router.dispatch(AppCommand::RefreshDueSubscriptions);
    }
}

pub(super) fn should_start_core_on_startup(settings: &AppSettings, force_start_core: bool) -> bool {
    // UAC 提权后的新实例必须继续完成用户刚触发的启动动作。
    force_start_core || settings.start_core_after_launch
}

pub(super) fn should_hide_window_on_startup(settings: &AppSettings, tray_supported: bool) -> bool {
    settings.silent_start && tray_supported
}
