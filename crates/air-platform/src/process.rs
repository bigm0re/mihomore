// 子进程窗口策略集中放在 platform 层，避免 core/app 直接散落 Windows 专有标志。
// Windows GUI 进程启动 console 子程序时默认可能闪出控制台窗口；这里仅隐藏窗口，
// stdout/stderr 仍由调用方 pipe 到日志，不能影响诊断能力。

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn hide_tokio_subprocess_window(command: &mut tokio::process::Command) {
    apply_tokio_no_window(command);
}

pub fn hide_std_subprocess_window(command: &mut std::process::Command) {
    apply_std_no_window(command);
}

#[cfg(windows)]
fn apply_tokio_no_window(command: &mut tokio::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn apply_tokio_no_window(_command: &mut tokio::process::Command) {}

#[cfg(windows)]
fn apply_std_no_window(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn apply_std_no_window(_command: &mut std::process::Command) {}

/// 探测本机内网 IPv4 地址。
///
/// 实现方式是不实际发包的 UDP `connect`：内核会为“到公网的路由”选出本机出口地址，
/// 这比枚举网卡更可靠（能跳过虚拟网卡、回环和未连接接口）。
/// 探测失败不是错误，仪表盘会显示“未知”，因此这里返回 `Option`。
pub fn local_ipv4_address() -> Option<String> {
    // 使用 TEST-NET-1（RFC 5737）保留地址：不会真正建立连接，也不会对外发送流量。
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let address = socket.local_addr().ok()?;
    let ip = address.ip();
    // 未连接接口时 local_addr 可能是 0.0.0.0；这属于无效结果。
    if ip.is_unspecified() {
        return None;
    }
    Some(ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ipv4_address_never_returns_unspecified_address() {
        // 该测试不断言具体网段：CI 与开发机的网卡配置不同，
        // 只验证不会把 0.0.0.0 这种无效地址当成结果返回。
        if let Some(address) = local_ipv4_address() {
            assert!(!address.starts_with("0.0.0.0"));
            assert!(address.parse::<std::net::Ipv4Addr>().is_ok());
        }
    }
}
