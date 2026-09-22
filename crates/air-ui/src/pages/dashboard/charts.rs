//! 仪表盘图表几何计算。
//!
//! 这里只做坐标换算，不依赖 GPUI 的绘制 API，因此可以脱离窗口单测。
//! 页面负责把算好的点交给 `canvas` 画线。

use gpui::{Point, px};

/// 曲线相对画布的内边距。
///
/// 上下留白让峰值不会贴边被裁切，底部留白保证 0 速率时曲线仍可见。
pub const CHART_PADDING_Y: f32 = 8.0;
/// 曲线左右两端的留白。
///
/// 折线从 `x=0` 开始画、到 `x=width` 结束，会紧贴卡片内边，stroke 的一半会溢出到
/// padding 区域甚至圆角之外。这里把曲线和基线都内缩 4 像素，避免被裁切。
pub const CHART_PADDING_X: f32 = 4.0;

/// 折线的采样点坐标。
///
/// 约定：
///
/// - 只有一个采样点时，横向无法按索引铺开，直接放在中间，避免除以零。
/// - 采样值超过 `peak` 时钳制到顶部，保证突发流量不会画出画布之外。
pub fn sample_points(
    samples: &[f32],
    width: f32,
    height: f32,
    peak: f32,
) -> Vec<Point<gpui::Pixels>> {
    let baseline = baseline_y(height);
    // 峰值必须为正，否则所有点都会落到基线；调用方已通过 floor 保证，这里再兜一层。
    let safe_peak = if peak > 0.0 { peak } else { 1.0 };
    let usable_height = (baseline - CHART_PADDING_Y).max(0.0);

    // 横向留白让曲线不贴边；width 极窄时退化为单点居中。
    let inner_width = (width - 2.0 * CHART_PADDING_X).max(0.0);

    match samples.len() {
        0 => Vec::new(),
        1 => vec![Point {
            x: px(CHART_PADDING_X + inner_width / 2.0),
            y: px(y_for_value(samples[0], usable_height, safe_peak, baseline)),
        }],
        count => {
            let step = if count > 1 {
                inner_width / (count - 1) as f32
            } else {
                0.0
            };
            samples
                .iter()
                .enumerate()
                .map(|(index, value)| Point {
                    x: px(CHART_PADDING_X + index as f32 * step),
                    y: px(y_for_value(*value, usable_height, safe_peak, baseline)),
                })
                .collect()
        }
    }
}

/// 平滑曲线使用的单个三次贝塞尔控制点对。
///
/// `charts` 层只做几何计算，返回控制点而不是直接绘制，这样平滑算法可以脱离 GPUI 单测。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveSegment {
    /// 本段的终点（下一个采样点）。
    pub to: Point<gpui::Pixels>,
    /// 第一控制点（靠近当前点）。
    pub ctrl_a: Point<gpui::Pixels>,
    /// 第二控制点（靠近终点）。
    pub ctrl_b: Point<gpui::Pixels>,
}

/// 平滑强度系数（Catmull-Rom 转三次贝塞尔的经典取值 `1/6`）。
///
/// 该值让曲线在采样点处恰好穿过数据点，同时切线连续，视觉上接近平滑流量图的
/// 手绘曲线；取更大会在数据剧烈跳动时产生过冲，取更小则接近折线。
const CURVE_TENSION: f32 = 1.0 / 6.0;

/// 把采样点转换为平滑的三次贝塞尔线段（Catmull-Rom 样条）。
///
/// 关键性质：
///
/// - 曲线**穿过**每一个采样点，不会改变数据语义。
/// - 每个采样点的切线由前后相邻点决定，因此转折处连续，不会出现折线拐角。
/// - 端点使用单侧差分（把首尾点各自外推一份），避免首尾出现异常切线。
///
/// 返回的线段数等于 `points.len() - 1`；点数不足 2 时返回空。
pub fn smooth_segments(points: &[Point<gpui::Pixels>]) -> Vec<CurveSegment> {
    if points.len() < 2 {
        return Vec::new();
    }

    let mut segments = Vec::with_capacity(points.len() - 1);
    for index in 0..points.len() - 1 {
        let previous = points[index.saturating_sub(1)];
        let current = points[index];
        let next = points[index + 1];
        // 最后一段的"再下一个点"用终点自身外推，保持与起点相同的单侧差分策略。
        let after_next = points.get(index + 2).copied().unwrap_or(next);

        // Catmull-Rom 切线：相邻两点差的一半。首尾点靠上面的外推得到合理切线。
        let tangent_out = gpui::point(
            (next.x - previous.x) * CURVE_TENSION,
            (next.y - previous.y) * CURVE_TENSION,
        );
        let tangent_in = gpui::point(
            (after_next.x - current.x) * CURVE_TENSION,
            (after_next.y - current.y) * CURVE_TENSION,
        );

        segments.push(CurveSegment {
            to: next,
            ctrl_a: gpui::point(current.x + tangent_out.x, current.y + tangent_out.y),
            ctrl_b: gpui::point(next.x - tangent_in.x, next.y - tangent_in.y),
        });
    }

    segments
}

/// 曲线下方的填充多边形：沿曲线走一遍，再沿基线折回闭合。
///
/// 为了让填充与曲线完全重合，这里使用与描边相同的贝塞尔控制点，而不是用直线连接采样点；
/// 否则曲线与填充边缘之间会出现可见的缝隙。
///
/// 返回的点序列可直接交给 `PathBuilder` 的 `move_to` / `cubic_bezier_to` / `close`。
pub fn area_outline(points: &[Point<gpui::Pixels>], baseline: f32) -> Option<AreaOutline> {
    if points.len() < 2 {
        return None;
    }
    let segments = smooth_segments(points);
    if segments.is_empty() {
        return None;
    }
    Some(AreaOutline {
        start: points[0],
        segments,
        // 收尾：先垂直落到基线，再水平回到起点下方，最后闭合。
        baseline_end: gpui::point(points[points.len() - 1].x, px(baseline)),
        baseline_start: gpui::point(points[0].x, px(baseline)),
    })
}

/// 曲线下方填充区域的几何描述。
#[derive(Clone, Debug, PartialEq)]
pub struct AreaOutline {
    /// 曲线起点。
    pub start: Point<gpui::Pixels>,
    /// 平滑曲线各段。
    pub segments: Vec<CurveSegment>,
    /// 曲线终点垂直落到的基线位置。
    pub baseline_end: Point<gpui::Pixels>,
    /// 基线回到起点下方的位置。
    pub baseline_start: Point<gpui::Pixels>,
}

/// 曲线基线（0 速率）的纵坐标。
///
/// 画布高度过小时可能小于顶部内边距，这里钳制到至少等于顶部留白，
/// 保证 `baseline >= top`，避免出现负的可用高度。
pub fn baseline_y(height: f32) -> f32 {
    (height - CHART_PADDING_Y).max(CHART_PADDING_Y)
}

fn y_for_value(value: f32, usable_height: f32, peak: f32, baseline: f32) -> f32 {
    // 负速率没有物理意义；钳制到 0 防止曲线画到基线下方。
    let ratio = (value.max(0.0) / peak).min(1.0);
    baseline - ratio * usable_height
}

/// 环形图（donut）上某个比例对应的坐标。
///
/// 从 12 点方向开始顺时针绘制，与常见流量统计图一致。
pub fn point_on_circle(
    center_x: f32,
    center_y: f32,
    radius: f32,
    fraction: f32,
) -> Point<gpui::Pixels> {
    // 12 点方向对应 -90°；顺时针意味着角度随比例增加。
    let angle = fraction.clamp(0.0, 1.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    Point {
        x: px(center_x + radius * angle.cos()),
        y: px(center_y + radius * angle.sin()),
    }
}

/// 环形图的弧线端点对。
///
/// 返回 `(起点, 终点)`；无流量时返回 `None`，由 UI 决定显示空环还是提示文字。
pub fn donut_arc(
    center_x: f32,
    center_y: f32,
    radius: f32,
    fraction: f32,
) -> Option<(Point<gpui::Pixels>, Point<gpui::Pixels>)> {
    if fraction <= 0.0 {
        return None;
    }
    Some((
        point_on_circle(center_x, center_y, radius, 0.0),
        point_on_circle(center_x, center_y, radius, fraction),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_segments_pass_through_every_sample_point() {
        // 关键语义：平滑不能改变数据，曲线必须穿过每一个采样点。
        let points = sample_points(&[100.0, 900.0, 200.0, 700.0], 300.0, 100.0, 1000.0);
        let segments = smooth_segments(&points);

        assert_eq!(segments.len(), points.len() - 1);
        for (index, segment) in segments.iter().enumerate() {
            assert_eq!(segment.to, points[index + 1]);
        }
    }

    #[test]
    fn smooth_segments_are_absent_for_degenerate_input() {
        assert!(smooth_segments(&[]).is_empty());
        assert!(smooth_segments(&[gpui::point(px(1.0), px(2.0))]).is_empty());
    }

    #[test]
    fn smooth_segments_keep_flat_series_flat() {
        // 全 0（空闲）时切线必须为 0，不能把直线拉出波纹。
        let points = sample_points(&[0.0, 0.0, 0.0, 0.0], 300.0, 100.0, 1000.0);
        let segments = smooth_segments(&points);
        let baseline = points[0].y;

        for segment in &segments {
            assert_eq!(segment.ctrl_a.y, baseline);
            assert_eq!(segment.ctrl_b.y, baseline);
            assert_eq!(segment.to.y, baseline);
        }
    }

    #[test]
    fn smooth_segments_control_points_stay_within_neighbor_bounds() {
        // Catmull-Rom 在 1/6 系数下不会超出相邻数据点的凸包，避免曲线冲出画布。
        let points = sample_points(&[0.0, 1000.0, 0.0], 300.0, 100.0, 1000.0);
        let segments = smooth_segments(&points);

        let min_y = points
            .iter()
            .map(|p| p.y.as_f32())
            .fold(f32::INFINITY, f32::min);
        let max_y = points
            .iter()
            .map(|p| p.y.as_f32())
            .fold(f32::NEG_INFINITY, f32::max);
        for segment in &segments {
            for control in [segment.ctrl_a, segment.ctrl_b] {
                assert!(
                    control.y.as_f32() >= min_y - 0.01 && control.y.as_f32() <= max_y + 0.01,
                    "控制点 {control:?} 超出数据范围 [{min_y}, {max_y}]"
                );
            }
        }
    }

    #[test]
    fn area_outline_closes_along_the_baseline() {
        let points = sample_points(&[0.0, 800.0, 0.0], 300.0, 100.0, 1000.0);
        let baseline = baseline_y(100.0);
        let outline = area_outline(&points, baseline).expect("outline should exist");

        // 起点是曲线起点，两条基线点位于同一水平线上。
        assert_eq!(outline.start, points[0]);
        assert_eq!(outline.baseline_start.y, px(baseline));
        assert_eq!(outline.baseline_end.y, px(baseline));
        assert_eq!(outline.baseline_start.x, points[0].x);
        assert_eq!(outline.baseline_end.x, points[points.len() - 1].x);
        // 填充轮廓与描边共用同一组贝塞尔控制点，否则会出现缝隙。
        assert_eq!(outline.segments, smooth_segments(&points));
    }

    #[test]
    fn area_outline_is_absent_for_degenerate_input() {
        assert!(area_outline(&[], 50.0).is_none());
        assert!(area_outline(&[gpui::point(px(1.0), px(2.0))], 50.0).is_none());
    }

    #[test]
    fn area_outline_stays_inside_the_canvas() {
        let points = sample_points(&[0.0, 1000.0, 0.0, 500.0], 300.0, 100.0, 1000.0);
        let outline = area_outline(&points, baseline_y(100.0)).expect("outline should exist");

        for point in std::iter::once(outline.start)
            .chain(std::iter::once(outline.baseline_start))
            .chain(std::iter::once(outline.baseline_end))
            .chain(outline.segments.iter().map(|s| s.to))
        {
            assert!(
                point.y.as_f32() >= 0.0 && point.y.as_f32() <= 100.0,
                "{point:?}"
            );
        }
    }

    #[test]
    fn baseline_is_clamped_for_degenerate_canvas_heights() {
        // 高度为 0 时不能返回负值，否则可用高度变成负数，曲线会画到画布外。
        assert_eq!(baseline_y(0.0), CHART_PADDING_Y);
        assert_eq!(baseline_y(4.0), CHART_PADDING_Y);
        assert_eq!(baseline_y(100.0), 100.0 - CHART_PADDING_Y);
    }

    #[test]
    fn zero_rate_baseline_stays_inside_the_canvas() {
        let points = sample_points(&[0.0, 0.0, 0.0], 300.0, 100.0, 1000.0);

        for point in points {
            assert!(point.y >= px(0.0));
            assert!(point.y <= px(100.0));
        }
    }

    #[test]
    fn values_above_peak_are_clamped_into_canvas() {
        // 采样值超过峰值时必须贴顶，而不是画出画布。
        let points = sample_points(&[5000.0], 300.0, 100.0, 1000.0);

        assert_eq!(points[0].y, px(CHART_PADDING_Y));
    }

    #[test]
    fn sample_points_handle_single_sample_without_dividing_by_zero() {
        // 只有一个点时索引差为 0；必须放在中间而不是产生 NaN。
        let points = sample_points(&[500.0], 300.0, 100.0, 1000.0);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].x, px(150.0));
        assert!(points[0].y.as_f32().is_finite());
    }

    #[test]
    fn sample_points_are_inset_by_horizontal_padding() {
        // 曲线必须左右各留 CHART_PADDING_X，否则 stroke 的一半会溢出到卡片 padding 区域。
        let points = sample_points(&[0.0, 1.0, 2.0], 300.0, 100.0, 2.0);

        assert_eq!(points.len(), 3);
        assert_eq!(points[0].x, px(CHART_PADDING_X));
        assert_eq!(points[2].x, px(300.0 - CHART_PADDING_X));
        // 中间点应落在两端之间。
        let mid = points[1].x.as_f32();
        assert!(mid > points[0].x.as_f32() && mid < points[2].x.as_f32());
    }

    #[test]
    fn sample_points_stay_inside_a_narrow_canvas() {
        // 画布比两侧留白总和还窄时不能出现负宽度或越界坐标。
        let points = sample_points(&[1.0, 2.0], 6.0, 40.0, 2.0);

        assert_eq!(points.len(), 2);
        for point in &points {
            assert!(point.x.as_f32() >= 0.0);
            assert!(point.x.as_f32() <= 6.0);
        }
    }

    #[test]
    fn sample_points_are_empty_for_no_samples() {
        assert!(sample_points(&[], 300.0, 100.0, 1000.0).is_empty());
    }

    #[test]
    fn chart_peak_floor_keeps_low_traffic_readable() {
        // 峰值极小（或为 0）时不能把噪声放大成满屏尖峰；调用方传入 floor 后
        // 低流量应当贴近基线而不是贴顶。
        let floor = 16.0 * 1024.0;
        let points = sample_points(&[64.0], 300.0, 100.0, floor);

        assert!(points[0].y > px(90.0));
    }

    #[test]
    fn negative_values_do_not_draw_below_baseline() {
        let points = sample_points(&[-100.0], 300.0, 100.0, 1000.0);

        assert_eq!(points[0].y, px(baseline_y(100.0)));
    }

    #[test]
    fn point_on_circle_starts_at_twelve_oclock() {
        let start = point_on_circle(50.0, 50.0, 10.0, 0.0);

        // 12 点方向：x 与圆心一致，y 在圆心上方。
        assert!((start.x.as_f32() - 50.0).abs() < 0.01);
        assert!((start.y.as_f32() - 40.0).abs() < 0.01);
    }

    #[test]
    fn point_on_circle_reaches_three_oclock_at_quarter_turn() {
        let quarter = point_on_circle(50.0, 50.0, 10.0, 0.25);

        // 顺时针四分之一圈到 3 点方向。
        assert!((quarter.x.as_f32() - 60.0).abs() < 0.01);
        assert!((quarter.y.as_f32() - 50.0).abs() < 0.01);
    }

    #[test]
    fn donut_arc_is_absent_without_traffic() {
        assert!(donut_arc(50.0, 50.0, 10.0, 0.0).is_none());
        assert!(donut_arc(50.0, 50.0, 10.0, -1.0).is_none());
    }

    #[test]
    fn donut_arc_spans_from_top_clockwise() {
        let (start, end) = donut_arc(50.0, 50.0, 10.0, 0.5).expect("arc should exist");

        assert!((start.y.as_f32() - 40.0).abs() < 0.01);
        // 半圈后到 6 点方向。
        assert!((end.y.as_f32() - 60.0).abs() < 0.01);
    }
}
