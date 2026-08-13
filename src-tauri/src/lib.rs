//! FingerTip 应用入口（Tauri 2.x）
//!
//! Phase 0：脚手架占位，仅配置窗口 + Naive UI 兼容权限。
//! Phase 1：暴露 hook 模块（rdev_listener / writer）。
//! Phase 5：注册系统托盘。
//! Phase 7：DB 初始化 + Tauri Command 注册 + 调度器启动。

pub mod commands;
pub mod db;
pub mod generate;
pub mod hook;
pub mod keymap;
pub mod model;
pub mod privacy;
pub mod scheduler;
pub mod summary;
pub mod tray;

// v0.5.0: 键盘 Hook 是否已成功启动（全局标志，供前端读取状态条渲染）。
// 启动失败（如权限被拒）保持 false，状态条灰点；成功 store(true)，绿点。
pub static HOOK_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

use std::sync::{Arc, Mutex};
use tauri::Manager;

use commands::AppState;

/// Tauri 应用主入口
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // v0.4.1: 初始化日志后端 —— 此前 `log::*` 宏无 logger 全为静默空操作，
    // 排查生成链路问题时一条后端日志都看不到。env_logger 输出到 stdout（tauri dev 终端可见），
    // 默认 info 级；可用 RUST_LOG 环境变量调级。
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // v0.3: AI 抽象层改为按 command 调用时构造（factory 在 crate::generate），不再在 run() 顶层常驻。
    // —— 1. 启动 Tauri ——
    //
    // v0.2.5 关键修复：DB 路径必须在 setup() 闭包内通过 app.path().app_data_dir()
    //   解析，绝对不能在 run() 顶层用当前工作目录 (".") 回退。原因：
    //   - HKCU Run 键启动的进程，其 cwd 是 C:\Windows\System32\（普通用户无写权限）
    //   - 旧逻辑会写 C:\Windows\System32\fingertip.db → init_at 返回 Err → expect("init DB") panic
    //   - 新逻辑用 bundle identifier (`com.fingertip.app`) 解析到用户写权限内的目录：
    //       Windows: %APPDATA%\com.fingertip.app\fingertip.db
    //       macOS:   ~/Library/Application Support/com.fingertip.app/fingertip.db
    //       Linux:   ~/.local/share/com.fingertip.app/fingertip.db
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--silent"])))
        .invoke_handler(tauri::generate_handler![
            commands::get_today_summary,
            commands::trigger_run_summary_now,
            commands::get_today_key_count,
            commands::get_hook_status, // v0.5.0: 前端状态条依赖此命令
            commands::get_today_hourly,
            commands::generate_now,
            commands::get_autostart,
            commands::set_autostart,
            commands::list_summaries,
            commands::set_mood,
            commands::get_artifact,
            commands::generate_sentence,
            commands::upload_and_generate_qr,
            commands::get_model_config,
            commands::set_model_config,
            // v0.6.0: 重新生成系列命令 —— 复用已有 description/fields，仅重跑模型
            commands::regenerate_sentence,
            commands::regenerate_music,
            commands::regenerate_art,
        ])
        .setup(move |app| {
                // —— 2a. 解析应用数据目录 + 初始化 SQLite（持久化，重启不丢） ——
                let app_data_dir = app
                    .path()
                    .app_data_dir()
                    .expect("failed to resolve app_data_dir");
                if let Err(e) = std::fs::create_dir_all(&app_data_dir) {
                    // create_dir_all 偶发失败时（例如权限边界）继续尝试 init_at，
                    // 让它内部逻辑统一报错位置，便于排查。
                    log::warn!(
                        "create_dir_all({}) failed (db init may also fail): {}",
                        app_data_dir.display(),
                        e
                    );
                }
                let db_path = app_data_dir.join("fingertip.db");
                log::info!("FingerTip DB: {}", db_path.display());
                let conn = db::init_at(&db_path).expect("init DB");
                let conn = Arc::new(Mutex::new(conn));

                // v0.4 T11: 初始化 FingertipConfig（首次启动写默认到 fingertip-config.json）
                let config_path = app_data_dir.join("fingertip-config.json");
                if !config_path.exists() {
                    // 写默认配置（任何解析失败由 load_config 自身回退默认）
                    if let Err(e) = crate::model::config::save_config(
                        &config_path,
                        &crate::model::config::FingertipConfig::default(),
                    ) {
                        log::warn!("save_config(default) 失败（不影响启动）: {}", e);
                    }
                }

                // —— 2b. 注册 AppState 给 Tauri command（注入 tauri::State<AppState>） ——
                let state = AppState {
                    conn: conn.clone(),
                    config_path,
                };
                app.manage(state);

                // 窗口标题
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_title("FingerTip");
                }
                // 托盘
                if let Err(e) = tray::build(&app.handle()) {
                    log::error!("Failed to build tray: {:?}", e);
                }
                // 启动 Scheduler 后台循环（Logic-5）
                // 注意：用 tauri::async_runtime::spawn，不用裸 tokio::spawn
                //   —— Tauri 2.x 的 setup 闭包运行在 main thread 的同步上下文，
                //   裸 tokio::spawn 找不到 runtime 会 panic。
                let conn_for_sched = conn.clone();
                let mood_source: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
                let _ = tauri::async_runtime::spawn(async move {
                    // spawn_loop(...).await 返回 Result<(), JoinError> — 显式丢弃
                    let _ = scheduler::driver::spawn_loop(conn_for_sched, mood_source).await;
                });

                // 启动 HookListener 完整链路（Logic-3）：rdev → HookWriter → SQLite
                //
                // 行为（v0.2.1 关键修复）：
                //   1. rdev 闭包只用 try_send 推入 channel（**绝不阻塞 OS 钩子线程**）
                //   2. 独立 writer 后台线程 50 条 OR 100ms flush 一次
                //   3. PRAGMA WAL + synchronous=NORMAL 已让单次 INSERT 极快
                //   4. 用 prepare_cached 复用 prepared statement
                //   5. 单事务批量提交
                //
                // 这是「每键真写入 + 不卡 OS」的最优解，代价：
                //   - 崩溃丢 ≤ 1s 数据（用户已接受「近实时」取舍）
                //   - 极端突发（> 4096/100ms）极少数丢弃（dropped 计数监控）
                //
                // 历史背景：
                //   v0.1 batch flush → v0.2 每键直接 INSERT（必卡）
                //   v0.2.1 加 channel + writer 后台 = 解 OS 钩子线程阻塞
                let conn_for_hook = conn.clone();

                // 启动 HookWriter 后台 writer 线程
                let writer = hook::writer::HookWriter::spawn(conn_for_hook.clone());

                let session_id = format!(
                    "session-{}",
                    chrono::Local::now().timestamp_millis()
                );
                let mut listener = hook::rdev_listener::RdevListener::new(session_id);
                let writer_for_sink = writer.clone();
                if let Err(e) = listener.start(Box::new(move |e| {
                    // 永不阻塞 —— 把 1 个 KeyEvent 推到 channel
                    writer_for_sink.send(e);
                })) {
                    log::error!("Failed to start RdevListener: {:?}", e);
                } else {
                    // v0.5.0: Hook 启动成功 → 标记状态条绿点
                    HOOK_RUNNING.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                log::info!("HookWriter 启动：channel={} / batch={} / flush_interval={}ms",
                    hook::writer::CHANNEL_CAPACITY,
                    hook::writer::BATCH_SIZE,
                    hook::writer::FLUSH_INTERVAL_MS,
                );

                // 让 writer 句柄常驻整个 app 生命周期（leak 防止 writer 被误释放）
                std::mem::forget(writer);
                Ok(())
            })
        // v0.8.3: 点 × 最小化到托盘而不是退出进程 —— 后台键盘记录不中断。
        // 注意：托盘「退出」走 app.exit(0)（RunEvent::ExitRequested），
        // 不经过 CloseRequested，不会被这里的 prevent_close 拦截。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                // 悬停托盘图标时提示仍在后台，避免用户以为程序已退出
                if let Some(tray) = window.app_handle().tray_by_id("main-tray") {
                    let _ = tray.set_tooltip(Some("FingerTip 仍在后台运行 · 单击托盘图标恢复窗口"));
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
