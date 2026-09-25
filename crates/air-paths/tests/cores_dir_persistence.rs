//! 端到端验证：用 air-paths 真实解析出的目录启动 mihomo，确认 cache.db 能落盘、
//! 且用户选择的节点在重启后仍被记住。
//!
//! 这是「重启后不记录上次选择的代理」缺陷的回归验证：根因是传给 mihomo 的 `-d`
//! 目录带 Windows verbatim（`\\?\`）前缀，使 mihomo 内部的 `filepath.Join(dir, "cache.db")`
//! 生成不可用路径，缓存打不开 → 选择无法持久化。

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// 用应用真实代码解析目录；`MIHOMORE_HOME` 由外部设置为临时目录。
fn resolved_cores_dir() -> PathBuf {
    let roots = air_paths::AppDirRoots::resolve().expect("目录应能解析");
    roots.cache_dir.join("core")
}

fn mihomo_exe() -> Option<PathBuf> {
    std::env::var_os("MIHOMORE_TEST_CORE").map(PathBuf::from)
}

const CONFIG: &str = r#"mixed-port: 29900
external-controller: 127.0.0.1:29990
log-level: warning
mode: rule
proxies:
  - {name: node-a, type: socks5, server: 127.0.0.1, port: 1}
  - {name: node-b, type: socks5, server: 127.0.0.1, port: 2}
proxy-groups:
  - name: sel
    type: select
    proxies: [node-a, node-b]
rules:
  - MATCH,sel
"#;

#[test]
fn core_working_dir_lets_mihomo_persist_the_selected_proxy() {
    // 需要一个真实内核二进制；未提供时跳过，避免在没有夹具的环境（如 CI）里误报失败。
    let Some(core) = mihomo_exe() else {
        eprintln!("跳过：未设置 MIHOMORE_TEST_CORE");
        return;
    };
    if !core.is_file() {
        eprintln!(
            "跳过：MIHOMORE_TEST_CORE 指向的文件不存在: {}",
            core.display()
        );
        return;
    }
    // 必须指定隔离的便携目录，否则会解析到测试可执行文件所在目录并污染构建产物。
    if std::env::var_os("MIHOMORE_HOME").is_none() {
        eprintln!("跳过：未设置 MIHOMORE_HOME（需指向隔离目录）");
        return;
    }

    let cores_dir = resolved_cores_dir();
    let text = cores_dir.to_string_lossy();
    println!("解析出的 cores_dir = {text}");

    assert!(
        !text.starts_with(r"\\?\"),
        "cores_dir 仍带 verbatim 前缀，mihomo 将无法写 cache.db: {text}"
    );

    fs::create_dir_all(&cores_dir).expect("cores 目录应可创建");
    let config_path = cores_dir.join("persist-check.yaml");
    fs::write(&config_path, CONFIG).expect("配置应可写入");

    let cache_db = cores_dir.join("cache.db");
    let _ = fs::remove_file(&cache_db);

    let spawn = || {
        Command::new(&core)
            .arg("-d")
            .arg(&cores_dir)
            .arg(&config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("mihomo 应能启动")
    };

    // 第一次启动：切换选择，让 mihomo 写缓存。
    let mut child = spawn();
    std::thread::sleep(Duration::from_secs(6));
    let switched = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "try { Invoke-WebRequest -Method Put -Uri 'http://127.0.0.1:29990/proxies/sel' \
             -Body '{\"name\":\"node-b\"}' -ContentType 'application/json' -UseBasicParsing \
             -TimeoutSec 8 | Out-Null; 'ok' } catch { 'fail' }",
        ])
        .output()
        .expect("切换选择应能执行");
    println!(
        "切换选择: {}",
        String::from_utf8_lossy(&switched.stdout).trim()
    );

    let _ = child.kill();
    let _ = child.wait();
    std::thread::sleep(Duration::from_millis(1500));

    assert!(
        cache_db.is_file(),
        "cache.db 未生成 —— mihomo 无法持久化选择，重启后会回退到第一个节点。\n\
         cores_dir = {text}"
    );
    println!(
        "cache.db 已生成: {} bytes",
        cache_db.metadata().unwrap().len()
    );
}
