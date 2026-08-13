//! 性能基线测试（设计文档第六节验收门槛）。
//!
//! 验证意图：核心聚合路径必须满足性能指标，
//! 防止后续重构无意中引入性能 regression。

use fingertip_lib::hook::event::KeyEvent;
use fingertip_lib::summary::aggregator::Aggregator;
use std::time::Instant;

#[test]
fn aggregate_100k_events_under_5s() {
    // 验证意图：10 万事件聚合 < 5 秒（设计文档性能基线）
    let events: Vec<KeyEvent> = (0..100_000)
        .map(|i| KeyEvent::now(i as u32, "s".into(), 0))
        .collect();

    let start = Instant::now();
    let _counts = Aggregator::count_by_key(&events);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "10 万事件聚合耗时 {} 秒，超出 5 秒基线",
        elapsed.as_secs_f64()
    );

    // 顺便记录实际耗时，便于趋势观察
    eprintln!("[perf] 100k events aggregated in {:?}", elapsed);
}

#[test]
fn hourly_buckets_100k_events_under_3s() {
    // 验证意图：时段分桶（chrono 调用较贵）< 3 秒
    let events: Vec<KeyEvent> = (0..100_000)
        .map(|i| {
            let mut e = KeyEvent::now(0, "s".into(), 0);
            // 分布在不同小时（用 i % 24）
            e.timestamp_ms = 1_700_000_000_000 + (i as i64 % 24) * 3_600_000;
            e
        })
        .collect();

    let start = Instant::now();
    let _buckets = Aggregator::hourly_buckets(&events);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 3,
        "10 万事件时段分桶耗时 {:?}，超出 3 秒基线",
        elapsed
    );
}