//! Task 0.2: 窗口策略配置验证
//!
//! 验证意图：保证主窗口有合理最小尺寸（防止被缩到看不见），
//! 并且默认隐藏（后台托盘常驻应用不应启动即弹窗打扰用户）。

#[test]
fn window_min_size_is_set() {
    let cfg = include_str!("../tauri.conf.json");
    assert!(
        cfg.contains("\"minWidth\": 1024"),
        "窗口最小宽度应 ≥ 1024（用户反馈：避免窄到需要滚动）"
    );
    assert!(
        cfg.contains("\"minHeight\": 680"),
        "窗口最小高度应 ≥ 680，确保 hero + 4 stats 单屏可见"
    );
}

#[test]
fn window_starts_hidden_for_tray_residency() {
    let cfg = include_str!("../tauri.conf.json");
    assert!(
        cfg.contains("\"visible\": false"),
        "后台常驻应用默认应隐藏窗口，避免启动时打扰用户"
    );
}