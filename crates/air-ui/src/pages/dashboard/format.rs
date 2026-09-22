//! 仪表盘的数值格式化。
//!
//! 与日志页的 `format_bytes` 不同，仪表盘需要把「数值」和「单位」分开渲染
//! （设计稿中「788 KB」是不同字号），因此这里返回结构化结果而不是拼接字符串。

/// 把字节数格式化为「数值 + 单位」两段。
pub fn split_bytes(bytes: u64) -> (String, &'static str) {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    // 字节保留整数；带单位时保留一位小数，但三位数以上去掉小数避免卡片排版溢出。
    let text = if unit == 0 {
        format!("{bytes}")
    } else if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    (text, UNITS[unit])
}

/// 把速率（字节/秒）格式化为 `数值 单位/s`。
pub fn format_rate(bytes_per_second: u64) -> String {
    let (value, unit) = split_bytes(bytes_per_second);
    format!("{value} {unit}/s")
}

/// 把运行时长（秒）格式化为 `HH:MM:SS`。
///
/// 小时不截断到两位：内核可能连续运行数天，`100:00:00` 比 `04:00:00` 更真实。
pub fn format_elapsed(seconds: i64) -> String {
    let total = seconds.max(0);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}

/// 计算内核已运行秒数。
///
/// 时间戳来自系统时钟，可能因为校时而出现在未来；这种情况钳制到 0，
/// 避免界面显示负时长。
pub fn elapsed_since(start_unix: i64, now_unix: i64) -> i64 {
    (now_unix - start_unix).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_keep_integer_form_without_unit_scaling() {
        assert_eq!(split_bytes(0), ("0".to_string(), "B"));
        assert_eq!(split_bytes(999), ("999".to_string(), "B"));
    }

    #[test]
    fn scales_to_kilobytes_and_megabytes_with_one_decimal() {
        assert_eq!(split_bytes(1024), ("1.0".to_string(), "KB"));
        assert_eq!(split_bytes(1536), ("1.5".to_string(), "KB"));
        assert_eq!(split_bytes(1024 * 1024), ("1.0".to_string(), "MB"));
    }

    #[test]
    fn drops_decimals_for_three_digit_values() {
        // 三位数以上再带小数会让卡片排版溢出，因此直接取整。
        assert_eq!(split_bytes(100 * 1024), ("100".to_string(), "KB"));
        assert_eq!(split_bytes(999 * 1024), ("999".to_string(), "KB"));
    }

    #[test]
    fn rate_appends_per_second_suffix() {
        assert_eq!(format_rate(0), "0 B/s");
        assert_eq!(format_rate(2048), "2.0 KB/s");
    }

    #[test]
    fn elapsed_is_zero_padded() {
        assert_eq!(format_elapsed(0), "00:00:00");
        assert_eq!(format_elapsed(59), "00:00:59");
        assert_eq!(format_elapsed(60), "00:01:00");
        assert_eq!(format_elapsed(3661), "01:01:01");
    }

    #[test]
    fn elapsed_hours_are_not_wrapped_at_two_digits() {
        // 连续运行数天时小时数会超过两位，不能截断成看起来像“重启过”的值。
        assert_eq!(format_elapsed(100 * 3600), "100:00:00");
    }

    #[test]
    fn elapsed_clamps_negative_input_to_zero() {
        assert_eq!(format_elapsed(-5), "00:00:00");
    }

    #[test]
    fn elapsed_since_clamps_future_timestamps_to_zero() {
        // 系统校时可能让 now 早于启动时间；不能显示负时长。
        assert_eq!(elapsed_since(1000, 900), 0);
        assert_eq!(elapsed_since(1000, 1000), 0);
        assert_eq!(elapsed_since(1000, 1060), 60);
    }
}
