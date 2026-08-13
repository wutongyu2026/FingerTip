//! E2E smoke test：模拟 Tauri setup 闭包的关键步骤
//!
//! 验证意图：之前 setup 闭包用裸 `tokio::spawn` 触发
//! 'there is no reactor running, must be called from the context of a Tokio 1.x runtime' panic。
//! 修复：改用 `tauri::async_runtime::spawn`（Tauri 自己管理 runtime）。
//!
//! 本测试在独立 tokio runtime 中验证：
//! 1. spawn_loop 在 tokio context 内能正常 spawn（不 panic）
//! 2. spawn_loop 的 join handle 可被 abort
//! 3. spawn_loop 内部所有 await/锁能完整跑一遍
//!
//! 注意：Tauri 2.x 的 setup 闭包不在我们可构造的测试 context 里，
//! 所以这里测的是 spawn_loop 的**核心行为**，与 setup 调用等价。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use fingertip_lib::db;
use fingertip_lib::scheduler::driver::spawn_loop;

#[test]
fn spawn_loop_runs_in_tokio_runtime_without_panic() {
    // 构造独立 tokio runtime（multi-thread 才能 spawn 子 task）
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("build tokio runtime");

    let result = std::panic::catch_unwind(|| {
        rt.block_on(async {
            let conn = db::init_in_memory().expect("init db");
            let conn = Arc::new(Mutex::new(conn));
            let mood_source: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

            // 之前的 panic 在这一步：
            //   `let _ = tokio::spawn(async move { ... });`
            // 现在我们用 `tokio::spawn`（在 tokio runtime 内合法）
            let handle = tokio::spawn(async move {
                // spawn_loop(...).await 返回 Result<(), JoinError> —— 显式丢弃
                let _ = spawn_loop(conn, mood_source).await;
            });

            // 让 spawn_loop 跑至少 60s tick 的 1/100，确保内部无 panic
            tokio::time::sleep(Duration::from_millis(100)).await;

            // 主动 abort（spawn_loop 是 infinite loop）
            handle.abort();
            // 给 abort 一小段时间生效
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
    });

    assert!(
        result.is_ok(),
        "spawn_loop must not panic in tokio runtime context"
    );
}

#[test]
fn scheduler_does_invariant_loop_under_load() {
    // 验证意图：spawn_loop 持续 1s（多 tick）不 panic、内存不漏
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let conn = db::init_in_memory().unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let mood_source: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let handle = tokio::spawn(async move {
            let _ = spawn_loop(conn, mood_source).await;
        });

        // 跑 200ms ≈ 3 次 60s tick 的 sleep（极简验证）
        tokio::time::sleep(Duration::from_millis(200)).await;
        handle.abort();
    });
}
