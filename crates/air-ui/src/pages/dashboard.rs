//! 仪表盘页面渲染。
//!
//! 布局策略（对应「窗口缩放时智能调整大小和位置」的要求）：
//!
//! - 内容按可用宽度分档：宽屏 3 列、中屏 2 列、窄屏 1 列，卡片随之重排。
//! - 内容整体可纵向滚动，窗口变小时不会裁掉卡片。
//! - 右下角内核按钮用绝对定位悬浮在滚动容器之外，任何尺寸下都不会被内容遮挡。

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::Sizable;
use gpui_component::StyledExt;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;

use air_ui::components::{self, foundation};
use air_ui::icons::{self, Icon};
use air_ui::shell::{Shell, ShellPalette};

// 子模块声明：`dashboard.rs` 是页面入口，几何计算、卡片投影和状态都在独立文件里，
// 便于各自单测而不依赖 GPUI 绘制。
// `state` 对外可见：Shell 需要持有流量采样状态。
pub(crate) mod state;

mod cards;
mod charts;
mod format;

use self::cards::{CoreControlState, OutboundMode};
use self::state::DashboardTrafficState;

/// 仪表盘卡片之间的间距，沿用 4px 栅格。
const CARD_GAP: f32 = foundation::SPACE_4;

/// 网络速度曲线的最小高度；窗口变矮时不再压缩，改为整体出现滚动条。
const CHART_MIN_HEIGHT: f32 = 180.0;

/// 内核悬浮按钮距离窗口右下角的偏移。
const FLOATING_BUTTON_OFFSET: f32 = foundation::SPACE_4;

/// 卡片重排的宽度阈值。
///
/// 阈值按「一张卡片最小可读宽度 + 间距」推导：三列需要约 900px，
/// 两列需要约 620px，低于此值改为单列，避免卡片被压到无法阅读。
const THREE_COLUMN_MIN_WIDTH: f32 = 900.0;
const TWO_COLUMN_MIN_WIDTH: f32 = 620.0;

/// 宽屏下右侧窄列（系统代理 / 虚拟网卡）的宽度范围。
const SIDE_COLUMN_MIN_WIDTH: f32 = 240.0;
const SIDE_COLUMN_MAX_WIDTH: f32 = 340.0;

/// 每行显示的卡片数量。
///
/// 这是「智能调整大小和位置」的核心：只根据可用宽度决定列数，
/// 卡片自身用 `flex_1` 均分宽度，因此不会出现固定像素导致的重叠。
pub fn column_count(available_width: f32) -> usize {
    if available_width >= THREE_COLUMN_MIN_WIDTH {
        3
    } else if available_width >= TWO_COLUMN_MIN_WIDTH {
        2
    } else {
        1
    }
}

/// 渲染仪表盘页面。
///
/// `available_width` 由调用方传入窗口内容区宽度；`now_unix` 由 Shell 提供，
/// 使运行时长在每次重绘时刷新而不必在页面内维护定时器。
pub(crate) fn render_dashboard_page(
    snapshot: &air_app::AppSnapshot,
    traffic: &DashboardTrafficState,
    runtime_mode: &str,
    tun_enabled: bool,
    tun_pending: bool,
    core_pending: bool,
    available_width: f32,
    now_unix: i64,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let columns = column_count(available_width);
    // 内容宽度必须用显式像素值。
    //
    // `overflow_y_scrollbar()` 内部的 `Scrollable` 会给被包裹元素强制加
    // `.size_auto().flex_1()`，这会**覆盖** `w_full()`，导致内容宽度塌陷成
    // "最宽子项的自然宽度"（实测只有 ~190px，卡片全挤在左侧）。
    // 显式 `w(px(..))` 不会被 `size_auto()` 改写成 0，因此能稳定占满可用宽度。
    let content_width = available_width.max(1.0);
    let content = div()
        .flex()
        .flex_col()
        .gap(px(CARD_GAP))
        .p_5()
        .w(px(content_width))
        .min_w(px(0.0))
        // 内容不能被压缩：否则窗口变矮时卡片会被压扁而不溢出，滚动条就不会出现。
        .flex_shrink_0()
        .child(render_traffic_and_chart_row(
            snapshot,
            traffic,
            columns,
            content_width,
            tun_enabled,
            tun_pending,
            palette,
            cx,
        ))
        .child(render_secondary_row(
            snapshot,
            traffic,
            runtime_mode,
            columns,
            palette,
            cx,
        ));

    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .w_full()
        .min_h(px(0.0))
        .overflow_hidden()
        .bg(palette.background)
        // 滚动区域采用 gpui-component 官方示例（`story-container`）的写法：
        // 滚动容器自身用 `size_full()`，内容作为它的直接子元素。
        // 之前失败是因为用了 `flex_1` + 自定义 `ScrollHandle` 手动组装，
        // 导致高度被锁死且横向塌陷（卡片挤在左侧）。
        .child(
            div()
                .id("page-dashboard-scroll")
                .size_full()
                .overflow_y_scrollbar()
                .child(content),
        )
        .child(render_core_floating_button(
            snapshot,
            core_pending,
            now_unix,
            palette,
            cx,
        ))
}

/// 第一行：网络速度曲线（宽） + 系统代理 / 虚拟网卡（窄）。
fn render_traffic_and_chart_row(
    snapshot: &air_app::AppSnapshot,
    traffic: &DashboardTrafficState,
    columns: usize,
    available_width: f32,
    tun_enabled: bool,
    tun_pending: bool,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let chart = render_speed_chart_card(traffic, palette);
    // 两张卡片都需要 `cx` 生成点击回调；分开绑定避免同一表达式中重复可变借用。
    let system_proxy = render_system_proxy_card_with_state(&snapshot.system_proxy, palette, cx);
    let tun = render_tun_card(tun_enabled, tun_pending, palette, cx);

    if columns >= 3 {
        // 宽屏：曲线占据剩余宽度，右侧窄列固定宽度并纵向堆叠两张卡片。
        // 用固定宽度而不是 flex 比例，避免窄列被曲线挤压到文字换行。
        let side_width =
            (available_width * 0.34).clamp(SIDE_COLUMN_MIN_WIDTH, SIDE_COLUMN_MAX_WIDTH);
        div()
            .flex()
            .flex_row()
            .gap(px(CARD_GAP))
            .w_full()
            .min_w(px(0.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow()
                    .flex_basis(px(0.0))
                    .min_w(px(0.0))
                    .child(chart),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .w(px(side_width))
                    .gap(px(CARD_GAP))
                    .child(system_proxy)
                    .child(tun),
            )
    } else {
        // 中窄屏：纵向堆叠，避免卡片被压扁到无法阅读。
        div()
            .flex()
            .flex_col()
            .gap(px(CARD_GAP))
            .w_full()
            .min_w(px(0.0))
            .child(chart)
            .child(system_proxy)
            .child(tun)
    }
}

/// 第二行：流量统计、内网 IP、网络检测。
fn render_secondary_row(
    snapshot: &air_app::AppSnapshot,
    traffic: &DashboardTrafficState,
    runtime_mode: &str,
    columns: usize,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let mut row = div().flex().gap(px(CARD_GAP)).w_full().min_w(px(0.0));
    row = match columns {
        3 => row.flex_row(),
        2 => row.flex_row().flex_wrap(),
        _ => row.flex_col(),
    };

    let cards = vec![
        render_traffic_stats_card(traffic, palette).into_any_element(),
        render_outbound_mode_card(runtime_mode, palette, cx).into_any_element(),
        render_network_card(snapshot, palette, cx).into_any_element(),
    ];

    for card in cards {
        row = row.child(
            div()
                .flex()
                .flex_col()
                // 单列时必须占满宽度；多列时均分。
                .flex_1()
                .min_w(px(if columns == 1 { 0.0 } else { 240.0 }))
                .child(card),
        );
    }
    row
}

/// 卡片外框：统一圆角、边框、内边距和标题行。
fn card_shell(
    title: &'static str,
    icon: Icon,
    palette: ShellPalette,
    body: AnyElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(foundation::RADIUS_LG))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(palette.muted)
                .child(icons::icon(icon, palette.muted))
                .child(title),
        )
        .child(body)
}

/// 网络速度卡片：标题行显示实时上下行速率，下方是双色曲线。
fn render_speed_chart_card(
    traffic: &DashboardTrafficState,
    palette: ShellPalette,
) -> impl IntoElement {
    let upload = format::format_rate(traffic.current_upload());
    let download = format::format_rate(traffic.current_download());
    let samples = traffic
        .samples()
        .map(|sample| sample.upload.max(sample.download) as f32)
        .collect::<Vec<_>>();
    let peak = traffic.chart_peak() as f32;

    let chart = div().w_full().h(px(CHART_MIN_HEIGHT)).child(
        gpui::canvas(
            move |bounds, _, _| {
                let width = bounds.size.width.as_f32();
                let height = bounds.size.height.as_f32();
                charts::sample_points(&samples, width, height, peak)
            },
            move |bounds, points, window, _| {
                if points.len() < 2 {
                    return;
                }
                // 重要：`paint_path` 使用**窗口绝对坐标**，canvas 不会自动平移坐标系
                // （`Window::paint_path` 直接 insert_primitive，`Style::paint` 也不做偏移）。
                // 因此必须手动加上 `bounds.origin`，否则图形会被画到窗口左上角。
                let origin = bounds.origin;
                let to_abs =
                    |p: gpui::Point<gpui::Pixels>| gpui::point(p.x + origin.x, p.y + origin.y);
                let height = bounds.size.height.as_f32();
                let baseline = charts::baseline_y(height);

                // ① 曲线下方的渐变填充。用与描边相同的贝塞尔控制点勾勒轮廓，
                //    避免填充边缘与曲线之间出现缝隙。
                if let Some(outline) = charts::area_outline(&points, baseline) {
                    let mut area = gpui::PathBuilder::fill();
                    area.move_to(to_abs(outline.start));
                    for segment in &outline.segments {
                        area.cubic_bezier_to(
                            to_abs(segment.to),
                            to_abs(segment.ctrl_a),
                            to_abs(segment.ctrl_b),
                        );
                    }
                    area.line_to(to_abs(outline.baseline_end));
                    area.line_to(to_abs(outline.baseline_start));
                    area.close();
                    if let Ok(path) = area.build() {
                        // 自上而下淡出：曲线处最亮，靠近基线逐渐透明，形成体积感。
                        let gradient = gpui::linear_gradient(
                            180.0,
                            gpui::linear_color_stop(palette.active.opacity(0.30), 0.0),
                            gpui::linear_color_stop(palette.active.opacity(0.0), 1.0),
                        );
                        window.paint_path(path, gradient);
                    }
                }

                // ② 平滑曲线本体。用三次贝塞尔连接采样点，避免折线的锯齿拐角。
                let segments = charts::smooth_segments(&points);
                let mut builder = gpui::PathBuilder::stroke(px(2.0));
                builder.move_to(to_abs(points[0]));
                for segment in &segments {
                    builder.cubic_bezier_to(
                        to_abs(segment.to),
                        to_abs(segment.ctrl_a),
                        to_abs(segment.ctrl_b),
                    );
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, palette.active);
                }
            },
        )
        .size_full(),
    );

    card_shell(
        "网络速度",
        Icon::Activity,
        palette,
        div()
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .min_w(px(0.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_3()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(rate_chip(Icon::ArrowUp, &upload, palette))
                    .child(rate_chip(Icon::ArrowDown, &download, palette)),
            )
            .child(chart)
            .into_any_element(),
    )
}

fn rate_chip(icon: Icon, value: &str, palette: ShellPalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(icons::icon(icon, palette.muted))
        .child(value.to_string())
}

/// 流量统计卡片：环形图 + 上传/下载累计值。
fn render_traffic_stats_card(
    traffic: &DashboardTrafficState,
    palette: ShellPalette,
) -> impl IntoElement {
    let (upload_value, upload_unit) = format::split_bytes(traffic.uploaded_bytes());
    let (download_value, download_unit) = format::split_bytes(traffic.downloaded_bytes());
    let share = traffic.upload_share();

    let donut = div().w(px(110.0)).h(px(110.0)).flex_none().child(
        gpui::canvas(
            move |bounds, _, _| {
                let width = bounds.size.width.as_f32();
                let height = bounds.size.height.as_f32();
                let radius = (width.min(height) / 2.0) - 8.0;
                // 同时记录 canvas 在窗口中的绝对原点：`paint_path` 不平移坐标系，
                // 必须自己加上 origin，否则环形图会被画到窗口左上角（就是那个灰色半圆）。
                (
                    bounds.origin.x.as_f32(),
                    bounds.origin.y.as_f32(),
                    width / 2.0,
                    height / 2.0,
                    radius,
                )
            },
            move |_, (origin_x, origin_y, center_x, center_y, radius), window, _| {
                // 把 canvas 相对坐标转换为窗口绝对坐标。
                let abs_of = |p: gpui::Point<gpui::Pixels>| {
                    gpui::point(p.x + px(origin_x), p.y + px(origin_y))
                };
                // 先画完整底环，再叠加已上传占比的弧，形成环形进度。
                // `arc_to` 的半径为 Point，最后一个参数是终点坐标。
                let radii = gpui::point(px(radius), px(radius));
                let mut ring = gpui::PathBuilder::stroke(px(8.0));
                ring.move_to(abs_of(charts::point_on_circle(
                    center_x, center_y, radius, 0.0,
                )));
                ring.arc_to(
                    radii,
                    px(0.0),
                    true,
                    true,
                    abs_of(charts::point_on_circle(center_x, center_y, radius, 0.999)),
                );
                if let Ok(path) = ring.build() {
                    window.paint_path(path, palette.subtle);
                }
                if let Some(share) = share
                    && let Some((start, end)) = charts::donut_arc(center_x, center_y, radius, share)
                {
                    let mut arc = gpui::PathBuilder::stroke(px(8.0));
                    arc.move_to(abs_of(start));
                    // 超过半圈时必须置位 large_arc，否则弧会被画成短边。
                    arc.arc_to(radii, px(0.0), share > 0.5, true, abs_of(end));
                    if let Ok(path) = arc.build() {
                        window.paint_path(path, palette.active);
                    }
                }
            },
        )
        .size_full(),
    );

    card_shell(
        "流量统计",
        Icon::Gauge,
        palette,
        div()
            .flex()
            .items_center()
            .gap_4()
            .w_full()
            .min_w(px(0.0))
            .child(donut)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(traffic_total_row(
                        Icon::ArrowUp,
                        "上传",
                        &upload_value,
                        upload_unit,
                        palette,
                    ))
                    .child(traffic_total_row(
                        Icon::ArrowDown,
                        "下载",
                        &download_value,
                        download_unit,
                        palette,
                    )),
            )
            .into_any_element(),
    )
}

fn traffic_total_row(
    icon: Icon,
    label: &'static str,
    value: &str,
    unit: &'static str,
    palette: ShellPalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .min_w(px(0.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(palette.muted)
                .child(icons::icon(icon, palette.muted))
                .child(label),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .gap_1()
                .child(div().text_lg().font_bold().child(value.to_string()))
                .child(div().text_xs().text_color(palette.muted).child(unit)),
        )
}

/// 系统代理卡片：开关状态直接来自系统投影，并由 Shell 处理点击。
///
/// 返回类型显式声明 `use<>`，使元素不捕获 `cx` 的生命周期；
/// 否则同一作用域内连续构建多张带回调的卡片会触发可变借用冲突。
fn render_system_proxy_card_with_state(
    state: &air_platform::system_proxy::SystemProxyState,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let card = cards::SystemProxyCard::from_state(state);
    let detail = card.detail.clone();
    let enabled = card.enabled;
    let interactive = card.interactive;
    // 端口不在线时 detail 已带 "· 内核未响应" 后缀，用警告色提示。
    let detail_color = if card.port_alive == Some(false) {
        palette.warning
    } else {
        palette.muted
    };

    card_shell(
        "系统代理",
        Icon::Power,
        palette,
        div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .min_w(px(0.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(palette.muted)
                            .child(icons::icon(Icon::Power, palette.muted))
                            .child("系统代理"),
                    )
                    .child(
                        // 开关本体只展示状态；点击由外层容器统一派发命令，
                        // 避免 Switch 组件自身的事件与 Shell 回调重复触发。
                        div()
                            .id("dashboard-system-proxy-toggle")
                            .cursor_pointer()
                            .child(components::app_switch(
                                "dashboard-system-proxy",
                                enabled,
                                !interactive,
                                "切换系统代理",
                            ))
                            .on_click(cx.listener(move |shell, _, _window, cx| {
                                shell.toggle_system_proxy_from_dashboard(!enabled);
                                cx.notify();
                            })),
                    ),
            )
            .child(div().text_xs().text_color(detail_color).child(detail))
            .into_any_element(),
    )
}

/// 虚拟网卡（TUN）卡片。
///
/// TUN 开关直接读写已保存的核心配置（`tun.enable`），因此卡片必须展示真实配置值
/// 而不是固定占位；否则用户会看到"已关闭"但内核实际已启用 TUN。
fn render_tun_card(
    tun_enabled: bool,
    tun_pending: bool,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let next = !tun_enabled;
    card_shell(
        "虚拟网卡",
        Icon::Network,
        palette,
        div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .min_w(px(0.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(palette.muted)
                            .child(icons::icon(Icon::Network, palette.muted))
                            .child("虚拟网卡"),
                    )
                    .child(
                        div()
                            .id("dashboard-tun-toggle")
                            .cursor_pointer()
                            .child(components::app_switch(
                                "dashboard-tun",
                                tun_enabled,
                                tun_pending,
                                "切换虚拟网卡（TUN）",
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.toggle_tun_from_dashboard(next, window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(if tun_enabled {
                        "已开启"
                    } else {
                        "已关闭"
                    }),
            )
            .into_any_element(),
    )
}

/// 出站模式卡片：三选一。
fn render_outbound_mode_card(
    runtime_mode: &str,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let active = OutboundMode::from_mode_value(runtime_mode);
    let options =
        OutboundMode::ALL
            .iter()
            .fold(div().flex().flex_col().gap_1().w_full(), |column, mode| {
                let mode = *mode;
                let selected = mode == active;
                column.child(
                    div()
                        .id(format!("dashboard-mode-{}", mode.value()))
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(32.0))
                        .px_3()
                        .rounded(px(foundation::RADIUS_SM))
                        .cursor_pointer()
                        .text_sm()
                        .text_color(if selected {
                            palette.text
                        } else {
                            palette.muted
                        })
                        .bg(if selected {
                            palette.active
                        } else {
                            palette.surface
                        })
                        .hover(move |this| {
                            if selected {
                                this.bg(palette.active_hover)
                            } else {
                                this.bg(palette.hover)
                            }
                        })
                        .child(icons::icon(
                            if selected {
                                Icon::CheckCircle
                            } else {
                                Icon::Circle
                            },
                            if selected {
                                palette.active_text
                            } else {
                                palette.muted
                            },
                        ))
                        .child(mode.label())
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.set_runtime_mode_from_dashboard(mode.value(), window, cx);
                            cx.notify();
                        })),
                )
            });

    card_shell(
        "出站模式",
        Icon::ListTree,
        palette,
        options.into_any_element(),
    )
}

/// 网络检测卡片：内网 IP 与公网地址。
fn render_network_card(
    snapshot: &air_app::AppSnapshot,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let local_ip = cards::local_ip_text(snapshot);
    let ipv4 = cards::public_address_text(&snapshot.network.ipv4);
    let ipv6 = cards::public_address_text(&snapshot.network.ipv6);

    card_shell(
        "网络检测",
        Icon::Globe,
        palette,
        div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .min_w(px(0.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(palette.muted)
                            .child(icons::icon(Icon::Network, palette.muted))
                            .child("内网 IP"),
                    )
                    .child(
                        div().id("dashboard-network-refresh").child(
                            Button::new("dashboard-network-refresh-button")
                                .icon(Icon::RefreshCw)
                                .ghost()
                                .small()
                                .on_click(cx.listener(|shell, _, _window, cx| {
                                    shell.probe_dashboard_network();
                                    cx.notify();
                                })),
                        ),
                    ),
            )
            .child(div().text_lg().font_bold().child(local_ip))
            .child(
                div()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(format!("IPv4 {ipv4}")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(format!("IPv6 {ipv6}")),
            )
            .into_any_element(),
    )
}

/// 右下角悬浮的内核启动按钮。
///
/// 绝对定位 + 高 z 序，保证无论内容如何滚动、窗口如何缩放都不会被遮挡。
fn render_core_floating_button(
    snapshot: &air_app::AppSnapshot,
    core_pending: bool,
    now_unix: i64,
    palette: ShellPalette,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let state = if core_pending {
        CoreControlState::Pending
    } else {
        CoreControlState::from_runtime(&snapshot.runtime)
    };
    let elapsed = cards::core_elapsed_seconds(snapshot, now_unix);
    let label = cards::core_control_label(state, elapsed);
    let running = state == CoreControlState::Running;
    let clickable = state.is_clickable();

    div()
        .absolute()
        .bottom(px(FLOATING_BUTTON_OFFSET))
        .right(px(FLOATING_BUTTON_OFFSET))
        .child(
            div()
                .id("dashboard-core-floating-button")
                .flex()
                .items_center()
                .gap_2()
                .h(px(40.0))
                .px_4()
                .rounded(px(foundation::RADIUS_LG))
                .border_1()
                .border_color(palette.border)
                .bg(palette.surface)
                .text_sm()
                .font_bold()
                .text_color(if clickable {
                    palette.text
                } else {
                    palette.muted
                })
                .shadow_lg()
                .cursor_pointer()
                .hover(move |this| this.bg(palette.hover))
                .child(icons::icon(
                    if running {
                        Icon::CircleStop
                    } else {
                        Icon::Power
                    },
                    if running {
                        palette.active
                    } else {
                        palette.muted
                    },
                ))
                .child(label)
                .on_click(cx.listener(move |shell, _, _window, cx| {
                    if clickable {
                        shell.toggle_core_from_dashboard();
                        cx.notify();
                    }
                })),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_windows_use_three_columns() {
        assert_eq!(column_count(1080.0), 3);
        assert_eq!(column_count(THREE_COLUMN_MIN_WIDTH), 3);
    }

    #[test]
    fn medium_windows_use_two_columns() {
        assert_eq!(column_count(THREE_COLUMN_MIN_WIDTH - 1.0), 2);
        assert_eq!(column_count(TWO_COLUMN_MIN_WIDTH), 2);
    }

    #[test]
    fn narrow_windows_fall_back_to_single_column() {
        assert_eq!(column_count(TWO_COLUMN_MIN_WIDTH - 1.0), 1);
        assert_eq!(column_count(320.0), 1);
        assert_eq!(column_count(0.0), 1);
    }

    #[test]
    fn column_thresholds_never_shrink_below_readable_width() {
        // 三列时每列至少约 300px，两列时至少约 310px，保证卡片文字不被压扁。
        assert!(THREE_COLUMN_MIN_WIDTH / 3.0 >= 240.0);
        assert!(TWO_COLUMN_MIN_WIDTH / 2.0 >= 240.0);
    }
}
