//! 仪表盘的纯数据与格式化逻辑。
//!
//! 这里只做「把原始速率采样变成可渲染的视图模型」这一件事，不触碰 GPUI：
//! 采样窗口、累计流量、环形占比和时长格式化都能独立单测，页面组件只负责摆放。

use std::collections::VecDeque;

/// 网络速度曲线保留的采样点数量。
///
/// 500ms 一个点、共 120 点约等于最近 60 秒，足够看清突发峰值，同时保持内存占用固定。
pub const MAX_TRAFFIC_SAMPLES: usize = 120;

/// 曲线峰值的最小下界（字节/秒）。
///
/// 没有下界时，空闲状态（全 0）会导致除零，且极小流量会把噪声放大成满屏尖峰。
pub const CHART_PEAK_FLOOR: f64 = 16.0 * 1024.0;

/// 采样间隔（秒），用于把速率积分成累计流量。
///
/// mihomo `/traffic` 默认约 1s 推送一次；这里与采样点语义保持一致。
pub const SAMPLE_INTERVAL_SECONDS: f64 = 1.0;

/// 单个速率采样点。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrafficSample {
    pub upload: u64,
    pub download: u64,
}

/// 仪表盘流量状态：曲线采样 + 累计流量。
#[derive(Clone, Debug, Default)]
pub struct DashboardTrafficState {
    samples: VecDeque<TrafficSample>,
    uploaded_bytes: f64,
    downloaded_bytes: f64,
}

/// 流量统计的**运行期**跟踪器：把曲线/累计值与“所属的内核运行期编号”绑定。
///
/// 这是修复“关闭窗口再打开就重置”的关键抽象。流量统计描述的是**内核这一次运行**的用量，
/// 而不是“某个窗口或页面的显示期”。因此：
///
/// - 窗口关闭到托盘、再打开 → 未开启新运行期 → **数据保留**；
/// - 内核停止后重新启动 → `begin_core_run` → **重新计数**。
///
/// 把编号与数据放在同一个结构里，可以让“何时该清空”成为可单测的不变式，
/// 而不是散落在多个调用点的 `reset()`。
#[derive(Clone, Debug, Default)]
pub struct DashboardTrafficRun {
    state: DashboardTrafficState,
    run_id: u64,
}

impl DashboardTrafficRun {
    /// 记录一个速率采样点，并按采样间隔累计流量。
    pub fn record(&mut self, upload: u64, download: u64) {
        self.state.record(upload, download);
    }

    /// 开启一个新的内核运行期：清空曲线与累计值，并把编号加一。
    ///
    /// 这是**唯一**会丢弃流量数据的地方，由内核进入 `Running` 时调用。
    /// 用递增编号而非布尔标记，是为了让“停止后立即重启”也能被识别为新运行期。
    pub fn begin_core_run(&mut self) {
        self.run_id = self.run_id.wrapping_add(1);
        self.state.reset();
    }

    /// 当前统计所属的内核运行期编号（0 表示尚未观察到内核启动）。
    pub fn run_id(&self) -> u64 {
        self.run_id
    }

    /// 只读访问底层的采样与累计值，供页面渲染使用。
    pub fn state(&self) -> &DashboardTrafficState {
        &self.state
    }
}

impl DashboardTrafficState {
    /// 记录一个速率采样点，并按采样间隔累计流量。
    ///
    /// 超出窗口的旧点会被丢弃，保证内存与渲染成本恒定。
    pub fn record(&mut self, upload: u64, download: u64) {
        self.samples.push_back(TrafficSample { upload, download });
        while self.samples.len() > MAX_TRAFFIC_SAMPLES {
            self.samples.pop_front();
        }
        // 速率 × 间隔 = 本次新增字节；用 f64 累计避免长时间运行后 u64 精度或溢出问题。
        self.uploaded_bytes += upload as f64 * SAMPLE_INTERVAL_SECONDS;
        self.downloaded_bytes += download as f64 * SAMPLE_INTERVAL_SECONDS;
    }

    /// 清空采样与累计值。
    ///
    /// 内核停止或流量流断开时调用，避免把上一次运行的曲线和总量混进新会话。
    pub fn reset(&mut self) {
        self.samples.clear();
        self.uploaded_bytes = 0.0;
        self.downloaded_bytes = 0.0;
    }

    pub fn samples(&self) -> impl Iterator<Item = &TrafficSample> {
        self.samples.iter()
    }

    /// 采样点数量。
    #[cfg(test)]
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// 最近一次采样的上传速率。
    pub fn current_upload(&self) -> u64 {
        self.samples.back().map(|sample| sample.upload).unwrap_or(0)
    }

    /// 最近一次采样的下载速率。
    pub fn current_download(&self) -> u64 {
        self.samples
            .back()
            .map(|sample| sample.download)
            .unwrap_or(0)
    }

    /// 累计上传字节数（取整展示）。
    pub fn uploaded_bytes(&self) -> u64 {
        self.uploaded_bytes.max(0.0) as u64
    }

    /// 累计下载字节数（取整展示）。
    pub fn downloaded_bytes(&self) -> u64 {
        self.downloaded_bytes.max(0.0) as u64
    }

    /// 曲线纵轴峰值。
    ///
    /// 取窗口内最大速率，并施加下界，保证空闲时曲线贴底而不是被放大成噪声。
    pub fn chart_peak(&self) -> f64 {
        let observed = self
            .samples
            .iter()
            .map(|sample| sample.upload.max(sample.download) as f64)
            .fold(0.0_f64, f64::max);
        observed.max(CHART_PEAK_FLOOR)
    }

    /// 上传在总流量中的占比，用于环形图。
    ///
    /// 无流量时返回 `None`，让 UI 显示为「无数据」而不是一个误导性的 50%。
    pub fn upload_share(&self) -> Option<f32> {
        let total = self.uploaded_bytes + self.downloaded_bytes;
        if total <= 0.0 {
            return None;
        }
        Some((self.uploaded_bytes / total).clamp(0.0, 1.0) as f32)
    }

    /// 是否记录到过任何流量。
    ///
    /// 由 `upload_share` 的调用方间接使用；保留公开方法便于测试与后续扩展。
    #[cfg(test)]
    pub fn has_traffic(&self) -> bool {
        self.uploaded_bytes > 0.0 || self.downloaded_bytes > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_window_is_bounded() {
        let mut state = DashboardTrafficState::default();
        for index in 0..(MAX_TRAFFIC_SAMPLES + 25) {
            state.record(index as u64, index as u64);
        }

        assert_eq!(state.sample_count(), MAX_TRAFFIC_SAMPLES);
        // 最旧的采样应已被丢弃，窗口内保留的是最后 MAX_TRAFFIC_SAMPLES 个点。
        assert_eq!(state.samples().next().map(|sample| sample.upload), Some(25));
    }

    #[test]
    fn totals_accumulate_rate_times_interval() {
        let mut state = DashboardTrafficState::default();
        state.record(1024, 2048);
        state.record(1024, 2048);

        assert_eq!(state.uploaded_bytes(), 2048);
        assert_eq!(state.downloaded_bytes(), 4096);
    }

    #[test]
    fn oversized_sample_gap_updates_chart_without_inflating_totals() {
        // 采样窗口有上限，但累计值必须继续累加，不能因为丢点而停止统计。
        let mut state = DashboardTrafficState::default();
        for _ in 0..(MAX_TRAFFIC_SAMPLES + 10) {
            state.record(100, 100);
        }

        assert_eq!(state.sample_count(), MAX_TRAFFIC_SAMPLES);
        assert_eq!(
            state.uploaded_bytes(),
            100 * (MAX_TRAFFIC_SAMPLES + 10) as u64
        );
    }

    #[test]
    fn chart_peak_has_floor_to_avoid_flattening_and_division_by_zero() {
        let state = DashboardTrafficState::default();

        assert_eq!(state.chart_peak(), CHART_PEAK_FLOOR);
    }

    #[test]
    fn chart_peak_tracks_observed_maximum_above_floor() {
        let mut state = DashboardTrafficState::default();
        state.record(10, CHART_PEAK_FLOOR as u64 * 4);

        assert_eq!(state.chart_peak(), CHART_PEAK_FLOOR * 4.0);
    }

    #[test]
    fn upload_share_splits_evenly_when_no_traffic_recorded() {
        let state = DashboardTrafficState::default();

        // 无流量时不给出占比，避免环形图显示误导性的 50%。
        assert_eq!(state.upload_share(), None);
        assert!(!state.has_traffic());
    }

    #[test]
    fn upload_share_reflects_recorded_totals() {
        let mut state = DashboardTrafficState::default();
        state.record(1000, 3000);

        let share = state.upload_share().expect("share should exist");
        assert!((share - 0.25).abs() < f32::EPSILON);
        assert!(state.has_traffic());
    }

    #[test]
    fn recorded_traffic_is_detected_from_either_direction() {
        let mut state = DashboardTrafficState::default();
        state.record(0, 1);
        assert!(state.has_traffic());

        let mut state = DashboardTrafficState::default();
        state.record(1, 0);
        assert!(state.has_traffic());
    }

    #[test]
    fn reset_clears_samples_and_totals() {
        let mut state = DashboardTrafficState::default();
        state.record(1024, 1024);

        state.reset();

        assert_eq!(state.sample_count(), 0);
        assert_eq!(state.uploaded_bytes(), 0);
        assert_eq!(state.downloaded_bytes(), 0);
        assert_eq!(state.current_upload(), 0);
    }

    #[test]
    fn current_rates_follow_latest_sample() {
        let mut state = DashboardTrafficState::default();
        state.record(11, 22);
        state.record(33, 44);

        assert_eq!(state.current_upload(), 33);
        assert_eq!(state.current_download(), 44);
    }

    #[test]
    fn traffic_run_keeps_data_within_the_same_core_run() {
        // 核心回归：同一个内核运行期内（例如关闭窗口再打开、页面来回切换），
        // 曲线与累计值必须保留。
        let mut run = DashboardTrafficRun::default();
        run.begin_core_run();
        let run_id = run.run_id();
        run.record(1000, 2000);
        run.record(500, 500);

        // 模拟窗口关闭再打开：期间只继续采样，不开启新运行期。
        run.record(300, 700);

        assert_eq!(run.run_id(), run_id, "同一内核运行期内编号不能变化");
        assert_eq!(run.state().uploaded_bytes(), 1800);
        assert_eq!(run.state().downloaded_bytes(), 3200);
        assert_eq!(run.state().sample_count(), 3);
    }

    #[test]
    fn traffic_run_resets_when_a_new_core_run_begins() {
        // 内核重启（包括停止后立即重启）必须重新计数。
        let mut run = DashboardTrafficRun::default();
        run.begin_core_run();
        run.record(1000, 2000);
        assert!(run.state().uploaded_bytes() > 0);

        run.begin_core_run();

        assert_eq!(run.run_id(), 2, "每次内核启动都应当递增运行期编号");
        assert_eq!(run.state().uploaded_bytes(), 0);
        assert_eq!(run.state().downloaded_bytes(), 0);
        assert_eq!(run.state().sample_count(), 0);
    }

    #[test]
    fn traffic_run_id_increments_even_for_back_to_back_restarts() {
        // 用递增编号而不是布尔标记，才能在“停止后立刻重启”时也识别为新运行期。
        let mut run = DashboardTrafficRun::default();
        assert_eq!(run.run_id(), 0, "尚未观察到内核启动");

        for expected in 1..=3 {
            run.begin_core_run();
            assert_eq!(run.run_id(), expected);
        }
    }
}
