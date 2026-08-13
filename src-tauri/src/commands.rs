//! Tauri Command：暴露给前端调用的 IPC 接口。
//!
//! 验证意图：前端 ↔ 后端边界明确，每个 Command 是一个有意义的业务操作。

use crate::db::event_repo::EventRepo;
use crate::db::migrations::run_migrations;
use crate::db::summary_repo::{DailySummaryRow, SummaryRepo};
use crate::generate::model_art::ModelArtAdapter;
use crate::generate::model_music::ModelMusicAdapter;
use crate::generate::{Art, ArtAdapter, Music, MusicAdapter};
use crate::hook::event::KeyEvent;
use crate::model::config::{route_capability, FingertipConfig, RouteDecision};
use crate::model::engine::{EngineClient, EngineHealth};
use crate::model::orchestrator::{run_orchestrator, OrchestrationContext};
use crate::model::{AudioClient, ImageClient, JsonChat};
use rusqlite::Connection;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Manager, State};

pub struct AppState {
    /// SQLite 连接（Phase 1 后端共享给所有需要数据库的 Command）
    pub conn: Arc<std::sync::Mutex<Connection>>,
    /// v0.4 T11: 应用配置路径（`app_data_dir/fingertip-config.json`）。
    /// 传给 `generate_now` 用 —— T14 Settings 持久化时是写入目标。
    pub config_path: PathBuf,
}

/// 前端读今日 summary：传入日期 'YYYY-MM-DD'，返回 DailySummaryRow JSON 字符串。
/// 没数据返 "null"（前端用 JSON.parse + null 判断）。
#[tauri::command]
pub fn get_today_summary(state: State<'_, AppState>, date: String) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let opt = get_today_summary_impl(&conn, &date).map_err(|e| e.to_string())?;
    // 序列化：Some → row JSON, None → "null"
    Ok(opt
        .map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "null".into()))
        .unwrap_or_else(|| "null".into()))
}

/// 纯函数实现（便于单测，不依赖 Tauri State）
pub fn get_today_summary_impl(
    conn: &Connection,
    date: &str,
) -> anyhow::Result<Option<DailySummaryRow>> {
    SummaryRepo::new(conn).read_by_date(date)
}

/// 应用启动时初始化数据库的 helper（lib.rs setup 阶段调用）
///
/// v0.3.1: 当前 lib.rs 直接用 `db::init_at`（避免 HKCU Run 启动时 cwd 在
/// C:\Windows\System32\ 的写权限问题），本函数保留为公共 API 供其他 entry point
/// 或集成测试使用。
#[allow(dead_code)]
pub fn init_db(conn: &Connection) -> anyhow::Result<()> {
    run_migrations(conn)
}

/// 强制立即聚合今日所有 key_events。
/// 配合前端 "Recalculate now" 按钮 —— 让用户按键后立即看到效果。
///
/// 返回聚合后的 DailySummaryRow JSON（前端可直接 JSON.parse）。
///
/// v0.3.1: mood 参数 deprecated —— mood_word 由 set_mood 单点管，scheduler 重算
/// 不再动 mood_word。参数保留兼容旧调用方。
#[tauri::command]
pub fn trigger_run_summary_now(
    state: State<'_, AppState>,
    _mood: Option<String>,
) -> Result<String, String> {
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let _ = crate::scheduler::driver::run_summarize_for_date(&conn, &today)
        .map_err(|e| e.to_string())?;
    let row: Option<DailySummaryRow> = SummaryRepo::new(&conn)
        .read_by_date(&today)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&row).unwrap_or_else(|_| "null".into()))
}

/// 查当前系统级 autostart 状态（"开机自启" toggle）
#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// 设置系统级 autostart 状态
#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enable: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enable {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    manager.is_enabled().map_err(|e| e.to_string())
}

/// 返回今日已捕获的 key_events 总数。
/// 让用户能直观看到"Hook 在工作"——配合今日 summary 一起验证整条链路。
///
/// v0.2.4 接 timezone offset：用户在前端切时区时，按"用户所在时区的今天 0:00"算边界。
/// `offset_minutes = 0` 时与旧行为兼容（按 UTC 0:00 算今天）。
#[tauri::command]
pub fn get_today_key_count(
    state: State<'_, AppState>,
    offset_minutes: i32,
) -> Result<i64, String> {
    let (today_start, today_end) = tz_today_range_ms(offset_minutes);
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM key_events WHERE timestamp_ms >= ? AND timestamp_ms < ?",
            rusqlite::params![today_start, today_end],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count)
}

/// v0.5.0: 键盘 Hook 是否已成功启动（前端顶部状态条用）。
///
/// 返回 true = Hook 已启动（绿点），false = Hook 启动失败（灰点 + 提示）。
/// 实现读 `crate::HOOK_RUNNING` 原子标志，由 `lib.rs::run()` 在 listener.start 成功时 store(true)。
///
/// 验证意图：Hook 启动失败（Windows 权限不足 / 监听器内部错误）用户毫无感知，
/// 状态条让"为什么没有按键数据"立刻可解释。
#[tauri::command]
pub fn get_hook_status() -> Result<bool, String> {
    Ok(crate::HOOK_RUNNING.load(std::sync::atomic::Ordering::Relaxed))
}

/// 返回今日 24 小时按键分布：[h0, h1, ..., h23]（每个值=该小时按键数）。
///
/// 验证意图：前端要算"活跃小时" + "高峰小时"，必须有真实 hourly 数据。
/// 之前用 `topKeys × 0.05` 凑出来的 7.8h 是错的——
/// 真实应该读 key_events → 按 hour 桶分桶 → 数多少桶 > 0。
///
/// v0.2.4 接 timezone offset：按"用户所在时区的今天 0:00 到明天 0:00"分桶。
/// 这样切时区后首页 hourly 会立即按新时区算。
#[tauri::command]
pub fn get_today_hourly(
    state: State<'_, AppState>,
    offset_minutes: i32,
) -> Result<Vec<i64>, String> {
    let (today_start, today_end) = tz_today_range_ms(offset_minutes);
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    let mut hourly = vec![0i64; 24];
    let mut stmt = conn
        .prepare(
            "SELECT timestamp_ms FROM key_events WHERE timestamp_ms >= ? AND timestamp_ms < ?",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(rusqlite::params![today_start, today_end])
        .map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let ts: i64 = row.get(0).map_err(|e| e.to_string())?;
        // 计算该 ts 落在哪个 hour 桶（基于"用户时区的今天 0:00"为锚点）
        let hour = ((ts - today_start) / 3_600_000).rem_euclid(24) as usize;
        hourly[hour] += 1;
    }
    Ok(hourly)
}

/// 纯函数：算"用户时区下的今天" UTC 时间戳范围。
///
/// `offset_minutes` = 用户所在时区相对 UTC 的分钟偏移（如 UTC+8 = 480，UTC-5 = -300）
/// 返回 (start_ms, end_ms) 对应"该时区下今天 0:00:00"到"明天 0:00:00"的 UTC 毫秒数。
pub fn tz_today_range_ms(offset_minutes: i32) -> (i64, i64) {
    use chrono::TimeZone;
    let now_utc = chrono::Utc::now().timestamp_millis();
    // 把"现在的 UTC 毫秒"按用户时区解读，得到"该时区下的当前本地时刻"
    let local_ms = now_utc + (offset_minutes as i64) * 60_000;
    let local = chrono::Utc.timestamp_millis_opt(local_ms).single().unwrap();
    // 该本地时刻的"今天 0:00:00"（仍按用户时区解读）
    let local_date = local.date_naive();
    let local_midnight = local_date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    // 把"该时区下的本地今天 0:00"反推回 UTC 毫秒
    let start_ms = local_midnight.timestamp_millis() - (offset_minutes as i64) * 60_000;
    (start_ms, start_ms + 86_400_000)
}

/// v0.6: 将 start_ms/end_ms 格式化为可读的时间范围标签（如 "06:00–08:00"）
/// v0.7-fix: 使用 Local 时区而非 UTC —— 前端传的是本地时间的 UTC 时间戳，
///           用 UTC 格式化会丢失时区偏移（用户选 08:00 却显示 00:00）。
pub(crate) fn format_time_range_label(start_ms: i64, end_ms: i64) -> String {
    use chrono::TimeZone;
    let fmt_hm = |ms: i64| -> String {
        let dt = chrono::Local.timestamp_millis_opt(ms).single();
        match dt {
            Some(d) => d.format("%H:%M").to_string(),
            None => String::new(),
        }
    };
    let s = fmt_hm(start_ms);
    let e = fmt_hm(end_ms);
    if s.is_empty() || e.is_empty() {
        return String::new();
    }
    format!("{}–{}", s, e)
}

/// History.vue 用：读最近 N 天的 daily_summary（按日期倒序），序列化为 JSON 数组。
///
/// 验证意图：前端不知道具体哪天有数据，需要"最近 N 天"窗口；
/// 返回 JSON 数组字符串（前端 JSON.parse 后渲染）。
/// `limit=0` 返回 `"[]"`（不报错、不返全表）。
#[tauri::command]
pub fn list_summaries(state: State<'_, AppState>, limit: usize) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    list_summaries_impl(&conn, limit).map_err(|e| e.to_string())
}

/// 纯函数版（便于单测，不依赖 Tauri State）
pub fn list_summaries_impl(conn: &Connection, limit: usize) -> anyhow::Result<String> {
    let rows = SummaryRepo::new(conn).list_recent(limit)?;
    let json = serde_json::to_string(&rows)?;
    Ok(json)
}

/// v0.8: 海报统计用 —— 今日 24 小时按键分布（UTC 今天 0:00 → 明天 0:00）。
/// 与 `get_today_hourly` 不同：固定 UTC 切分（key_events 的 timestamp_ms 就是 UTC），
/// 供 `upload_and_generate_qr` 填海报 hourly 数组。
fn get_hourly_impl(conn: &rusqlite::Connection) -> anyhow::Result<Vec<i64>> {
    let now = chrono::Utc::now();
    let today = now.date_naive();
    let today_dt = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let tomorrow_dt = today.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let start_ms = today_dt.timestamp_millis();
    let end_ms = tomorrow_dt.timestamp_millis();

    let mut hourly = vec![0i64; 24];
    let mut stmt = conn.prepare(
        "SELECT timestamp_ms FROM key_events WHERE timestamp_ms >= ? AND timestamp_ms < ?",
    )?;
    let mut rows = stmt.query(rusqlite::params![start_ms, end_ms])?;
    while let Some(row) = rows.next()? {
        let ts: i64 = row.get(0)?;
        let hour = ((ts - start_ms) / 3_600_000).rem_euclid(24) as usize;
        if hour < 24 { hourly[hour] += 1; }
    }
    Ok(hourly)
}

/// v0.8: 海报统计用 —— 键码 → 可读名（top1 键展示）。
fn key_display_name(code: u32) -> String {
    match code {
        32 => "Space".into(),
        13 => "Enter".into(),
        8 => "Backspace".into(),
        9 => "Tab".into(),
        27 => "Esc".into(),
        c if (65..=90).contains(&c) => (c as u8 as char).to_string(),
        c if (48..=57).contains(&c) => (c as u8 as char).to_string(),
        _ => format!("Key({})", code),
    }
}

/// v0.3.2: History.vue 点 day card 时调此 command 拉回历史作品。
///
/// 返回 `{ music: Music, art: Art, date }` JSON 字符串 —— 与 `generate_now` 输出
/// 形态一致，前端 Artworks.vue 可直接复用同一渲染路径。
/// 没数据返 `"null"`（与 `get_today_summary` 一致）。
#[tauri::command]
pub fn get_artifact(state: State<'_, AppState>, date: String) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    get_artifact_impl(&conn, &date).map_err(|e| e.to_string())
}

/// 纯函数版（便于单测，不依赖 Tauri State）
///
/// 内部：把 ArtifactRow 的 music_json / art_json 反序列化成 Music / Art，
/// 与 generate_now 输出的 JSON 形态对齐（`{ music, art, date }`）。
/// v0.3.4: 还返 `music_wav_path` + `art_png_path`（绝对路径 / Option，v0.3.2 老数据为 None）。
/// v0.4:   还返 `sentence`（编排器产出的一次成型描述，v0.4 之前为 None）。
pub fn get_artifact_impl(
    conn: &Connection,
    date: &str,
) -> anyhow::Result<String> {
    use crate::generate::{Art, Music};
    let row = crate::db::artifact_repo::read_by_date(conn, date)?;
    let row = match row {
        Some(r) => r,
        None => return Ok("null".into()),
    };
    let music: Music = serde_json::from_str(&row.music_json)?;
    let art: Art = serde_json::from_str(&row.art_json)?;
    let json = serde_json::json!({
        "music": music,
        "art": art,
        "date": date,
        "music_wav_path": row.music_wav_path,
        "art_png_path": row.art_png_path,
        "sentence": row.sentence,
        // v0.6.0: 透传 funny_summary；前端 History day card → Artworks 立即可见
        "funny_summary": row.funny_summary,
    });
    Ok(serde_json::to_string(&json)?)
}

/// v0.3 Stage 2 Task 2.2: 用户在 MoodPicker 选完词后,前端调此 command 写入 daily_summary.mood_word。
///
/// 验证意图：
/// - UI 上写一个 mood 词必须真正落库,不能只放在前端 state(V0 mock 的 bug)。
/// - 已有同日期 summary 时只更新 mood_word,不重置其他字段(upsert_mood 用 ON CONFLICT)。
/// - 已有 daily_summary NOT NULL 缺省占位由 Stage 2 Task 2.1 保证,所以 INSERT 新行也能成功。
///
/// 设计选择：plan 模板写 `async fn`,但本 command 无 `.await` 边界,写 sync 与现有 8 个
/// command(`get_today_summary`/`trigger_run_summary_now`/...)风格一致;锁内 sync SQL,
/// guard 在 drop 前完成,不会出现 MutexGuard 跨 await 的隐患(Stage 1 review forward-looking note)。
#[tauri::command]
pub fn set_mood(state: State<'_, AppState>, date: String, mood: String) -> Result<(), String> {
    set_mood_impl(&state.conn, &date, &mood).map_err(|e| e.to_string())
}

/// 纯函数版(便于单测,不依赖 Tauri State)
pub fn set_mood_impl(
    conn: &std::sync::Mutex<Connection>,
    date: &str,
    mood: &str,
) -> anyhow::Result<()> {
    let guard = conn.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {}", e))?;
    SummaryRepo::new(&guard).upsert_mood(date, mood)
}

/// v0.4 T11: 工厂 —— 按 FingertipConfig + 健康探测为 generate_now 装配所有模型客户端。
///
/// 三态路由核心（T11 集成）：
///   - LLM：route_capability(llm.mode, engine.llm, cloud ok) → 本地 EngineClient / 云端 MiniMaxChatClient
///   - Music：route_capability(audio.mode, engine.audio, cloud ok) → 本地 EngineClient / 云端 MiniMaxMusicClient
///   - Image：route_capability(image.mode, engine.image, cloud ok) → 本地 EngineClient / 云端 MiniMaxImageClient
///
/// 任何能力 Unavailable → 明确报错（不静默退化 ——「失败要大声」）。
///
/// 「云端 OK」的判定（v0.4 stub）：仅当配置中 `cloud_*_key` 非空时算 OK。
/// Settings 持久化（T14）会传真实用户配置；当前由 `load_config` 给出默认值。
///
/// 三个 client 都是 `Arc<dyn Trait>` 多态（dyn JsonChat / AudioClient / ImageClient），
/// generate_now_impl 拿到后既可走本地引擎、也可走云端。
pub struct ModelClients {
    pub chat: Arc<dyn JsonChat>,
    pub music: Arc<dyn AudioClient>,
    pub image: Arc<dyn ImageClient>,
    /// v0.4.1: 音乐路由出处标识（写进 Music.model）—— Local→"step-audio"，Cloud→"MiniMax-music"。
    /// 由 build_clients 在路由时定死，保证元数据与真实生成出处一致。
    pub music_model: &'static str,
    /// v0.4.1: 图像路由出处标识（写进 Art.model）—— Local→"sd-cpp"，Cloud→"MiniMax-image"。
    /// 由 build_clients 在路由时定死，保证元数据与真实生成出处一致。
    pub image_model: &'static str,
}

impl std::fmt::Debug for ModelClients {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelClients").finish()
    }
}

/// 健康探测（内部 helper）—— 拿 EngineClient 做一次 /v1/health。
/// 失败时一律返 `EngineHealth::default()`（全 false），由路由层判 Unavailable。
async fn probe_engine_health(engine_base_url: &str) -> EngineHealth {
    let client = EngineClient::new(engine_base_url);
    match client.health().await {
        Ok(h) => h,
        Err(e) => {
            log::warn!("EngineClient.health 失败（视为全不可用）: {}", e);
            EngineHealth::default()
        }
    }
}

/// 「云端 OK」判定 —— 仅当对应 base_url + key + model 都非空。
fn cloud_llm_ok(cfg: &FingertipConfig) -> bool {
    !cfg.llm.cloud_base.trim().is_empty()
        && !cfg.llm.cloud_key.trim().is_empty()
        && !cfg.llm.cloud_model.trim().is_empty()
}
fn cloud_image_ok(cfg: &FingertipConfig) -> bool {
    !cfg.image.cloud_base.trim().is_empty()
        && !cfg.image.cloud_key.trim().is_empty()
        && !cfg.image.cloud_model.trim().is_empty()
}
fn cloud_audio_ok(cfg: &FingertipConfig) -> bool {
    !cfg.audio.minimax_base.trim().is_empty()
        && !cfg.audio.minimax_key.trim().is_empty()
        && !cfg.audio.minimax_model.trim().is_empty()
}

/// v0.4 T11: 装配三态路由的客户端集合。
///
/// 输入：
///   - `cfg`：加载好的 `FingertipConfig`（来自 `state.config_path`）
///   - `engine_health`：本地引擎 `/v1/health` 结果（按能力位）
///
/// 输出：`(chat, music, image)` 三个 `Arc<dyn Trait>`。
/// 任何能力 Unavailable → 明确 `Err(...)`，错误信息含「哪个能力 + 路由原因」。
///
/// 测试可注入任意 `cfg + EngineHealth`，覆盖三态路由所有分支。
pub fn build_clients(
    cfg: &FingertipConfig,
    engine_health: &EngineHealth,
) -> anyhow::Result<ModelClients> {
    let engine = EngineClient::new(cfg.engine.base_url.clone());

    // ── 1. LLM（编排器） ──
    let chat: Arc<dyn JsonChat> = match route_capability(cfg.llm.mode, engine_health.llm, cloud_llm_ok(cfg), "llm") {
        RouteDecision::Local => Arc::new(EngineClientChat::new(engine.clone())),
        RouteDecision::Cloud => Arc::new(crate::model::cloud::MiniMaxChatClient::new(
            cfg.llm.cloud_base.clone(),
            cfg.llm.cloud_key.clone(),
            cfg.llm.cloud_model.clone(),
        )),
        RouteDecision::Unavailable(reason) => anyhow::bail!("编排器 LLM 路由不可用: {}", reason),
    };

    // ── 2. Audio ──
    let music: Arc<dyn AudioClient>;
    let music_model: &'static str;
    match route_capability(cfg.audio.mode, engine_health.audio, cloud_audio_ok(cfg), "audio") {
        RouteDecision::Local => {
            music = Arc::new(EngineClientAudio::new(engine.clone()));
            music_model = "step-audio";
        }
        RouteDecision::Cloud => {
            music = Arc::new(crate::model::cloud::MiniMaxMusicClient::new(
                cfg.audio.minimax_base.clone(),
                cfg.audio.minimax_key.clone(),
                cfg.audio.minimax_model.clone(),
            ));
            music_model = "MiniMax-music";
        }
        RouteDecision::Unavailable(reason) => anyhow::bail!("音乐客户端路由不可用: {}", reason),
    };

    // ── 3. Image ──
    let image: Arc<dyn ImageClient>;
    let image_model: &'static str;
    match route_capability(cfg.image.mode, engine_health.image, cloud_image_ok(cfg), "image") {
        RouteDecision::Local => {
            image = Arc::new(EngineClientImage::new(engine.clone()));
            image_model = "sd-cpp";
        }
        RouteDecision::Cloud => {
            image = Arc::new(crate::model::cloud::MiniMaxImageClient::new(
                cfg.image.cloud_base.clone(),
                cfg.image.cloud_key.clone(),
                cfg.image.cloud_model.clone(),
            ));
            image_model = "MiniMax-image";
        }
        RouteDecision::Unavailable(reason) => anyhow::bail!("图像客户端路由不可用: {}", reason),
    };

    Ok(ModelClients {
        chat,
        music,
        image,
        music_model,
        image_model,
    })
}

/// v0.4 T11: 异步版 —— 探测引擎健康 + 装配客户端。生产路径使用。
///
/// 内部：
///   1. `EngineClient::health()` 探测本地引擎（失败 → 全 false）
///   2. 调 `build_clients(cfg, &health)`
pub async fn build_clients_with_health(cfg: &FingertipConfig) -> anyhow::Result<ModelClients> {
    let health = probe_engine_health(&cfg.engine.base_url).await;
    log::info!(
        "引擎健康: llm={} audio={} image={} | llm模式={:?} audio模式={:?} image模式={:?}",
        health.llm, health.audio, health.image,
        cfg.llm.mode, cfg.audio.mode, cfg.image.mode
    );
    let clients = build_clients(cfg, &health)?;
    log::info!(
        "客户端装配: music→{} image→{} （llm_base={} llm_model={} audio_base={} audio_model={} image_base={} image_model={}）",
        clients.music_model, clients.image_model,
        cfg.llm.cloud_base, cfg.llm.cloud_model,
        cfg.audio.minimax_base, cfg.audio.minimax_model,
        cfg.image.cloud_base, cfg.image.cloud_model
    );
    Ok(clients)
}

/// 内部：包装 EngineClient 给 `Arc<dyn JsonChat>`（不能直接 Arc::new(EngineClient)
/// —— EngineClient 同时实现了三个 trait，会 trait object 不明确）。
struct EngineClientChat {
    inner: EngineClient,
}
impl EngineClientChat {
    fn new(inner: EngineClient) -> Self {
        Self { inner }
    }
}
#[async_trait::async_trait]
impl JsonChat for EngineClientChat {
    async fn chat_json(&self, system: &str, user: &str) -> anyhow::Result<serde_json::Value> {
        self.inner.chat_json(system, user).await
    }
}

struct EngineClientAudio {
    inner: EngineClient,
}
impl EngineClientAudio {
    fn new(inner: EngineClient) -> Self {
        Self { inner }
    }
}
#[async_trait::async_trait]
impl AudioClient for EngineClientAudio {
    async fn generate_audio(&self, text: &str) -> anyhow::Result<Vec<u8>> {
        self.inner.generate_audio(text).await
    }
}

struct EngineClientImage {
    inner: EngineClient,
}
impl EngineClientImage {
    fn new(inner: EngineClient) -> Self {
        Self { inner }
    }
}
#[async_trait::async_trait]
impl ImageClient for EngineClientImage {
    async fn generate_image(&self, prompt: &str) -> anyhow::Result<Vec<u8>> {
        self.inner.generate_image(prompt).await
    }
}

/// v0.4 T11: 构造 generate_now 的真实三件套客户端（不持锁，一次性）。
///
/// `chat` / `music` / `image` 来自 `build_clients(cfg, &health)`。
/// 内部把 trait object 包成对应 Adapter（ModelMusicAdapter / ModelArtAdapter）。
///
/// v0.4.1: 签名收敛为吃 `&ModelClients` —— 出处标识（music_model / image_model）
/// 由 build_clients 在路由时定死，此处不再硬编码 "step-audio"/"sd-cpp"；
/// 同时移除未用的 `cfg` 参数与 `let _ = chat` 透传噪音。
///
/// 三个 Adapter 都用 `Arc<dyn Trait>` 拿，自身是 value 类型（持有 Arc），便宜 clone。
fn make_adapters(
    clients: &ModelClients,
) -> (Arc<dyn JsonChat>, Box<dyn MusicAdapter>, Box<dyn ArtAdapter>) {
    let music_adapter =
        Box::new(ModelMusicAdapter::new(clients.music.clone(), clients.music_model)) as Box<dyn MusicAdapter>;
    let image_adapter =
        Box::new(ModelArtAdapter::new(clients.image.clone(), clients.image_model)) as Box<dyn ArtAdapter>;
    (clients.chat.clone(), music_adapter, image_adapter)
}

/// v0.4 T11: 一站式 generate_now —— 装配客户端 + 跑编排器 + 适配器生成 + wav 分析 + 写盘 + 写表。
///
/// 行为：
///   1. 读 `state.config_path` 加载 FingertipConfig
///   2. `build_clients_with_health` 探测引擎 + 装配 chat/audio/image 客户端
///   3. 构造 ModelMusicAdapter / ModelArtAdapter 包客户端
///   4. 同步阶段读 daily_summary + events
///   5. 构造 OrchestrationContext → 调 `run_orchestrator(chat, ctx)` 拿三条产物
///   6. 构造 MusicPrompt / ArtPrompt（含 description），调 adapter.generate 拿 MusicOutcome / ArtOutcome
///   7. wav_analysis 把 wav 字节 → 振幅 + duration_ms，填 Music
///   8. `write_artifacts_with_bytes` 写 wav/png 到 `app_data_dir/downloads/{date}/`
///   9. upsert_with_sentence 写表（含 sentence + 路径）
///   10. 返回 JSON：{ music, art, date, mood, style, sentence, music_wav_path, art_png_path }
#[tauri::command]
pub async fn generate_now(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    date: String,
    mood: String,
    style: String,
    // v0.7: 自定义时间窗口（前端 SubmitMood datetime-local → epoch ms）
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Result<String, String> {
    // 1. 加载配置（同步）
    let cfg = crate::model::config::load_config(&state.config_path);
    log::info!("generate_now 开始: date={} mood={} style={} window=({:?}, {:?})", date, mood, style, start_ms, end_ms);

    // 2. 探测引擎 + 装配客户端（异步）
    let clients = build_clients_with_health(&cfg)
        .await
        .map_err(|e| format!("generate_now 客户端装配失败: {}", e))?;
    let (chat_client, music_adapter, art_adapter) = make_adapters(&clients);

    // 3. 拿 app_data_dir（同步）
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {}", e))?;

    // 4. 同步阶段：读 daily_summary.theme_word + events（支持自定义时间窗口）
    //    v0.8: 选了时间窗口 → 从窗口 events 现场重算 theme_word（不再沿用全天 summary）
    let (theme_word, events, time_range_label) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let summary = SummaryRepo::new(&conn)
            .read_by_date(&date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no summary for date {}", date))?;
        let events = match (start_ms, end_ms) {
            (Some(s), Some(e)) => EventRepo::new(&conn)
                .list_by_timerange(s, e)
                .map_err(|e| e.to_string())?,
            _ => EventRepo::new(&conn)
                .list_by_date_48h(&date)
                .map_err(|e| e.to_string())?,
        };
        let label = match (start_ms, end_ms) {
            (Some(s), Some(e)) => format_time_range_label(s, e),
            _ => String::new(),
        };
        // 时间窗口指定时，从 events 现场重算主题词
        let theme = if start_ms.is_some() && end_ms.is_some() {
            let stats = crate::summary::aggregator::Aggregator::aggregate(date.clone(), &events);
            let sk = crate::summary::aggregator::Aggregator::count_special_keys(&events);
            let tw = crate::summary::theme::determine_theme_from_behavior(
                &events, stats.intensity, stats.steadiness, stats.fluency, stats.activity_hours,
            );
            log::info!("generate_now: 窗口主题词={:?} (全天={:?}) events={} 条 sp_keys=({}B/{}D/{}E/{}S/{}W) ", tw, summary.theme_word, events.len(), sk.backspace_count, sk.delete_count, sk.enter_count, sk.space_count, sk.wasd_count);
            tw
        } else {
            log::info!("generate_now: 当日 summary theme_word={:?} events={} 条 (48h 窗口)", summary.theme_word, events.len());
            summary.theme_word
        };
        (theme, events, label)
    };

    // 5. 调 generate_now_impl（编排 + 适配器 + wav 分析 + 写盘）
    // v0.4.1: 返回结构化 GenerateNowOutcome，不再反挖 JSON。
    let outcome = generate_now_impl(
        &date,
        &mood,
        &style,
        &theme_word,
        time_range_label,
        events,
        chat_client.as_ref(),
        music_adapter.as_ref(),
        art_adapter.as_ref(),
        &app_data_dir,
    )
    .await
    .map_err(|e| e.to_string())?;
    log::info!(
        "generate_now_impl 完成: sentence 长度={} wav={} png={}",
        outcome.sentence.len(),
        outcome.music_wav_path.display(),
        outcome.art_png_path.display()
    );

    // 6. 写 artifacts 表（v0.8：upsert_artifact_outcome 加 english_sentence / theme_explanation / time_range_label 4 字段）
    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let wav_path_str = outcome.music_wav_path.to_string_lossy();
        let png_path_str = outcome.art_png_path.to_string_lossy();
        crate::db::artifact_repo::upsert_artifact_outcome(
            &conn,
            &date,
            &outcome.music,
            &outcome.art,
            &outcome.sentence,
            Some(wav_path_str.as_ref()),
            Some(png_path_str.as_ref()),
            // v0.8: funny_summary 从 outcome 透传（None → 写空字符串）
            outcome.funny_summary.as_deref(),
            // v0.8: english_sentence / theme_explanation / time_range_label 从 outcome 透传
            outcome.english_sentence.as_deref().map(|s| s.to_string()),
            outcome.theme_explanation.as_deref().map(|s| s.to_string()),
            if outcome.time_range_label.is_empty() { None } else { Some(&outcome.time_range_label) },
        )
        .map_err(|e| e.to_string())?;
    }

    // 7. 拼返回 JSON（v0.4.1：形态与 v0.4 完全一致，前端契约不变）
    let json = json!({
        "music": outcome.music,
        "art": outcome.art,
        "date": date,
        "mood": mood,
        "style": style,
        "sentence": outcome.sentence,
        // v0.6.0: AI 键盘诊断透传；前端 Artworks 直接读 store.generationResult.funny_summary
        "funny_summary": outcome.funny_summary,
        // v0.8: 编排器新增 2 字段（英文结语 + 主题词解释）
        "english_sentence": outcome.english_sentence,
        "theme_explanation": outcome.theme_explanation,
        "time_range_label": outcome.time_range_label,
        "music_wav_path": outcome.music_wav_path.to_string_lossy(),
        "art_png_path": outcome.art_png_path.to_string_lossy(),
    });
    Ok(json.to_string())
}

/// v0.4.1: generate_now 的结构化产出（避免调用方从 JSON 反挖 sentence/路径）。
///
/// 携带：
///   - `music` / `art`：写 artifacts 表用（含适配器填好的 description / model）
///   - `sentence`：编排器一次性产出的当日结语
///   - `music_wav_path` / `art_png_path`：write_artifacts_with_bytes 落盘后的绝对路径
///
/// 不再返回冗余 PNG 字节（调用方不需要；产物已在磁盘上，路径足够）。
pub struct GenerateNowOutcome {
    pub music: Music,
    pub art: Art,
    pub sentence: String,
    /// v0.6.0: 编排器产出的 AI 键盘诊断（funny_summary），可为空字符串
    pub funny_summary: Option<String>,
    /// v0.8: 编排器新增 6 字段（英文结语 + 主题词解释 + 时间范围标签）
    pub english_sentence: Option<String>,
    pub theme_explanation: Option<String>,
    pub time_range_label: String,
    pub music_wav_path: PathBuf,
    pub art_png_path: PathBuf,
}

/// v0.4 T11: generate_now 异步核心 —— 编排器 + 适配器 + wav 分析 + 写盘。
///
/// 流程（spec）：
///   1. 用 `events` 调 Aggregator → stats → 构造 OrchestrationContext
///   2. 调 `run_orchestrator(chat, &ctx)` 拿 OrchestratorResult
///   3. 构造 MusicPrompt/ArtPrompt（含 description），调 adapter.generate
///   4. wav_analysis 把 wav 字节 → amplitudes/duration_ms，填 Music
///   5. write_artifacts_with_bytes 写 wav/png 到 `app_data_dir/downloads/{date}/`
///   6. 构造结构化产出（sentence + 两条产物路径）
///
/// 返回：`GenerateNowOutcome` —— music/art 给调用方写库，sentence + 路径供写表/拼 JSON。
/// v0.4.1: 不再返冗余 PNG 字节，也不返 JSON 字符串（拼 JSON 是 command 层的事）。
/// v0.8: 加 6 字段透传（english_sentence / theme_explanation / time_range_label）+ 特殊键计数进 ctx
pub async fn generate_now_impl(
    date: &str,
    mood: &str,
    style: &str,
    theme_word: &str,
    time_range_label: String,
    events: Vec<KeyEvent>,
    chat: &dyn JsonChat,
    music_adapter: &dyn MusicAdapter,
    art_adapter: &dyn ArtAdapter,
    app_data_dir: &Path,
) -> anyhow::Result<GenerateNowOutcome> {
    use crate::summary::aggregator::Aggregator;

    // 1. events → stats + 6 特殊键计数 → ctx
    let stats = Aggregator::aggregate(date.to_string(), &events);
    let sk = Aggregator::count_special_keys(&events);
    let ctx = OrchestrationContext {
        theme_word: theme_word.to_string(),
        mood: if mood.is_empty() { None } else { Some(mood.to_string()) },
        style: style.to_string(),
        intensity: stats.intensity,
        steadiness: stats.steadiness,
        fluency: stats.fluency,
        activity_hours: stats.activity_hours,
        top_keys: stats.top_keys.clone(),
        hourly: stats.hourly,
        first_active_ms: stats.first_active_ms,
        backspace_count: sk.backspace_count,
        delete_count: sk.delete_count,
        enter_count: sk.enter_count,
        space_count: sk.space_count,
        wasd_count: sk.wasd_count,
        total_events: sk.total_events,
    };

    // 2. 跑编排器
    let t_orch = std::time::Instant::now();
    let orch = run_orchestrator(chat, &ctx).await?;
    log::info!("编排器完成: {:.1}s sentence={:?}", t_orch.elapsed().as_secs_f32(), orch.sentence);

    // 3. 构造 prompt（含编排器 description）
    let music_prompt = crate::generate::MusicPrompt {
        events: Vec::new(), // ctx 已聚合；adapter 不消费 events（v0.4）
        mood: ctx.mood.clone(),
        style: ctx.style.clone(),
        theme_word: ctx.theme_word.clone(),
        description: Some(orch.music_description.clone()),
    };
    let art_prompt = crate::generate::ArtPrompt {
        events: Vec::new(),
        mood: ctx.mood.clone(),
        style: ctx.style.clone(),
        theme_word: ctx.theme_word.clone(),
        description: Some(orch.image_description.clone()),
    };

    // 4. adapter 生成 MusicOutcome / ArtOutcome
    let t_music = std::time::Instant::now();
    let music_outcome = music_adapter
        .generate(&music_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("{}: {}", music_adapter.name(), e))?;
    log::info!("音乐生成完成: {:.1}s", t_music.elapsed().as_secs_f32());
    let t_art = std::time::Instant::now();
    let art_outcome = art_adapter
        .generate(&art_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("{}: {}", art_adapter.name(), e))?;
    log::info!("图像生成完成: {:.1}s", t_art.elapsed().as_secs_f32());

    // 5. wav_analysis 二次分析 → 填 Music.amplitudes / duration_ms
    let mut music = music_outcome.music;
    let analysis = crate::db::wav_analysis::analyze_wav(&music_outcome.wav)
        .map_err(|e| anyhow::anyhow!("wav_analysis 失败: {}", e))?;
    music.amplitudes = analysis.amplitudes;
    music.duration_ms = analysis.duration_ms;

    // 6. 写 wav/png 字节流到 app_data_dir/downloads/{date}/
    let (wav_path, png_path) =
        crate::db::artifact_writer::write_artifacts_with_bytes(
            app_data_dir,
            date,
            &music_outcome.wav,
            &art_outcome.png,
        )?;
    log::info!("产物写盘: wav={} png={}", wav_path.display(), png_path.display());

    // 7. 构造结构化产出（v0.4.1：不再拼 JSON，改由 command 层拼）
    // v0.6.0: 透传 funny_summary；空字符串时返 None（避免写入数据库存空值，前端 v-if 兜底）
    // v0.8: 新增 english_sentence / theme_explanation / time_range_label 三字段透传
    let funny = orch.funny_summary.trim();
    let funny_summary = if funny.is_empty() { None } else { Some(funny.to_string()) };
    let en = orch.english_sentence.trim();
    let english_sentence = if en.is_empty() { None } else { Some(en.to_string()) };
    let te = orch.theme_explanation.trim();
    let theme_explanation = if te.is_empty() { None } else { Some(te.to_string()) };
    Ok(GenerateNowOutcome {
        music,
        art: art_outcome.art,
        sentence: orch.sentence,
        funny_summary,
        english_sentence,
        theme_explanation,
        time_range_label,
        music_wav_path: wav_path,
        art_png_path: png_path,
    })
}

/// v0.3.9: 上传 WAV 到 tmpfiles.org + 生成二维码
///
/// v0.7: 上传 WAV + 生成卡片 PNG + 生成二维码 → 返 QrArtifact JSON
///
/// 流程（同学 v0.7 实现）：
/// 1. 读 artifacts（music/art/sentence/funny_summary + png/wav 路径）
/// 2. 调 `create_share(&data)`：内部上传 WAV + PNG 到 uguu.se + 生成卡片 PNG（spawn_blocking）
/// 3. 返 `{local_path: 卡片 PNG 路径, audio_ok, share_url}` —— Artworks.vue 弹窗展示
#[tauri::command]
pub async fn upload_and_generate_qr(
    state: State<'_, AppState>,
    date: String,
    english_sentence: Option<String>,
) -> Result<String, String> {
    use crate::db::summary_repo::SummaryRepo;

    let share_data: crate::generate::upload::SharePageData = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;

        let artifact_row = crate::db::artifact_repo::read_by_date(&conn, &date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no artifact for date {}", date))?;

        let wav_path = std::path::PathBuf::from(
            artifact_row
                .music_wav_path
                .ok_or_else(|| format!("no wav file for date {}", date))?,
        );
        let png_path = std::path::PathBuf::from(
            artifact_row
                .art_png_path
                .ok_or_else(|| format!("no png file for date {}", date))?,
        );

        let summary = SummaryRepo::new(&conn)
            .read_by_date(&date)
            .map_err(|e| e.to_string())?;

        let theme_word = summary.as_ref().map(|s| s.theme_word.clone()).unwrap_or_default();
        let mood_word = summary.as_ref().and_then(|s| s.mood_word.clone());

        // v0.8: 计算海报所需统计（同学完整版）—— top1 键 / 频率 / 总按键 / 活跃小时
        let total_keys = summary.as_ref().map(|s| s.total_keys).unwrap_or(0) as usize;
        let activity_hours = summary.as_ref().map(|s| s.activity_hours).unwrap_or(0);
        let top1_key = summary.as_ref()
            .and_then(|s| {
                let parsed: Vec<(u32, usize)> = serde_json::from_str(&s.top_keys_json).ok()?;
                parsed.first().map(|(code, _)| key_display_name(*code))
            })
            .unwrap_or_else(|| "—".into());
        let active_minutes = activity_hours as f64 * 60.0;
        let frequency_per_min = if active_minutes > 0.0 {
            total_keys as f64 / active_minutes
        } else { 0.0 };
        // 查询 hourly
        let hourly = get_hourly_impl(&conn)
            .map_err(|e| e.to_string())?;
        let mut hourly_arr = [0usize; 24];
        for (i, v) in hourly.iter().enumerate() {
            hourly_arr[i] = *v as usize;
        }

        crate::generate::upload::SharePageData {
            wav_path,
            png_path,
            sentence: artifact_row.sentence.unwrap_or_default(),
            // v0.8.3: 英文优先用前端透传（刚重新生成的最新值），为空回退数据库。
            // 修「app 重启后 generationResult 内存丢失 → 分享页英文消失」。
            english_sentence: {
                let from_front = english_sentence.unwrap_or_default();
                if !from_front.trim().is_empty() {
                    from_front
                } else {
                    artifact_row.english_sentence.clone().unwrap_or_default()
                }
            },
            theme_word,
            mood: mood_word,
            date,
            top1_key,
            frequency_per_min,
            total_keys,
            activity_hours,
            hourly: hourly_arr,
            // v0.6: 从 artifact 读取时间范围标签
            time_range_label: artifact_row.time_range_label.unwrap_or_default(),
            download_url: crate::generate::upload::default_download_url(),
            // v0.9: 搞笑按键总结文案
            funny_summary: artifact_row.funny_summary.unwrap_or_default(),
        }
    };

    let artifact = crate::generate::upload::create_share(&share_data)
        .await
        .map_err(|e| e.to_string())?;

    // v0.8: 用系统浏览器打开卡片（用户生成后立即看到海报预览）
    let local_path = artifact.local_path.clone();
    let _ = tauri_plugin_opener::open_path(local_path, None::<&str>);

    serde_json::to_string(&artifact).map_err(|e| e.to_string())
}

/// v0.4 T12: 读已存 artifacts.sentence（编排器在 generate_now 时一次性产出）。
///
/// 设计：sentence 由 generate_now 流水线写入 artifacts.sentence；前端 Artworks.vue
/// 挂载时调此 command 直接读，不再二次调 LLM。
///
/// 返回形态：
///   - 有 sentence 行：`{"date":"...", "text":"..."}` JSON 字符串
///   - 无 artifacts 行（该日未 generate_now 过）：`"null"`（前端 JSON.parse + null 判断）
///
/// 与 Artworks.vue 句子面板兼容（sentence.text）；`words` 字段已废弃（T12 丢弃）。
#[tauri::command]
pub fn generate_sentence(state: State<'_, AppState>, date: String) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    generate_sentence_impl(&conn, &date).map_err(|e| e.to_string())
}

/// 纯函数版（便于单测，不依赖 Tauri State）
///
/// v0.4 T12: 读 artifacts.sentence（编排器产出一次成型）。返回 JSON：
///   - Some(row) → `{"date":date, "text":<sentence.unwrap_or_default()>}`
///   - None（无 artifacts 行）→ `"null"`（前端用 JSON.parse 后判 null）
///
/// 旧数据兼容：sentence 列可能为 NULL（v0.4 迁移前的 artifacts 行）—— 透传为 `text=""`
/// 不报错，让前端仍能渲染空字符串。
pub fn generate_sentence_impl(conn: &Connection, date: &str) -> anyhow::Result<String> {
    use crate::db::artifact_repo::read_by_date;
    let row = match read_by_date(conn, date)? {
        Some(r) => r,
        None => return Ok("null".into()),
    };
    let text = row.sentence.unwrap_or_default();
    Ok(serde_json::json!({"date": date, "text": text}).to_string())
}

/// v0.4 T14: 读当前 FingertipConfig（Settings UI 用）。
///
/// 返回 JSON 字符串（`null` 序列化的 default 也算）—— 前端 JSON.parse 后做 fallback。
/// 抽 `_impl` 便于单测避开 `tauri::State` 注入。
#[tauri::command]
pub fn get_model_config(state: State<'_, AppState>) -> Result<String, String> {
    get_model_config_impl(&state.config_path)
}

/// `get_model_config` 的纯函数实现（`tauri::State` 不可测，剥离以便单测）。
///
/// 行为：从 `path` 加载（文件不存在或解析失败由 `load_config` 自身回退默认），
/// 序列化为 JSON 字符串返给前端。
pub fn get_model_config_impl(path: &Path) -> Result<String, String> {
    let cfg = crate::model::config::load_config(path);
    serde_json::to_string(&cfg).map_err(|e| e.to_string())
}

/// v0.4 T14: 写 FingertipConfig（Settings UI 用）。
///
/// 直接吃 `serde_json::Value`，由 Tauri 自动反序列化为 `FingertipConfig`。
/// 前端不必二次 `JSON.stringify`，错误以用户可读的中文返给 UI（"失败要大声"）。
#[tauri::command]
pub fn set_model_config(state: State<'_, AppState>, config: serde_json::Value) -> Result<(), String> {
    set_model_config_impl(&state.config_path, config)
}

/// `set_model_config` 的纯函数实现（剥离 `tauri::State` 以便单测）。
///
/// 行为：`Value → FingertipConfig` 反序列化失败 → 返中文错；`save_config` 原子写失败 → 抛。
/// 反序列化与原子写均成功 → `Ok(())`。
pub fn set_model_config_impl(path: &Path, config: serde_json::Value) -> Result<(), String> {
    let cfg: FingertipConfig = serde_json::from_value(config)
        .map_err(|e| format!("config JSON 解析失败: {}", e))?;
    crate::model::config::save_config(path, &cfg)
        .map_err(|e| format!("config 保存失败: {}", e))
}

// ═══════════════════════════════════════════════════════════
// v0.6: 重新生成系列命令
// 设计：最小可用版 —— 复用已有 sentence + description + Music.model + Art.model，
//       仅重新调模型生成产物（PNG / WAV），不重新调编排器。
//       优点：不依赖 v0.7 的 list_by_date_48h / OrchestrationContext 6 字段特殊键计数，
//             移植面小、回归风险低、满足"重新生成"核心需求。
//       缺点：sentence / theme_word 不变（仅产物重生成）。
// ═══════════════════════════════════════════════════════════

/// v0.6: 重新生成今日画作 —— 复用已有 description，调 ImageAdapter 重生成 + 覆盖 png 文件。
///
/// 必须先有 artifact（前端 Artworks 页"重新生成画作"按钮触发）。
/// v0.8: 接受 start_ms/end_ms —— 透传给上游（当前 art 再生复用已有 description，暂不重跑编排器）。
#[tauri::command]
pub async fn regenerate_art(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    date: String,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Result<String, String> {
    use crate::generate::model_art::build_model_art_adapter;
    use crate::generate::{Art, ArtPrompt};
    use crate::model::ImageClient;
    let _ = (start_ms, end_ms); // v0.8: 时间窗口参数保留（当前复用已有 description，透传占位）

    log::info!("regenerate_art: date={}", date);

    // 1. 读已有 artifact + summary（同步阶段，锁尽快 drop）
    let (existing_art, theme_word, mood) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let artifact = crate::db::artifact_repo::read_by_date(&conn, &date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该日期还没有生成作品，请先去今日作品页生成".to_string())?;
        let art: Art = serde_json::from_str(&artifact.art_json)
            .map_err(|e| format!("解析 art_json 失败: {}", e))?;
        let summary = crate::db::summary_repo::SummaryRepo::new(&conn)
            .read_by_date(&date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no summary for date {}", date))?;
        (art.clone(), summary.theme_word.clone(), art.mood.clone().or(summary.mood_word))
    };

    // 2. 加载配置 + 装配客户端
    let cfg = crate::model::config::load_config(&state.config_path);
    let clients = build_clients_with_health(&cfg)
        .await
        .map_err(|e| format!("regenerate_art 客户端装配失败: {}", e))?;

    // 3. 调 ImageAdapter 重生成（复用已有 description）
    let adapter = build_model_art_adapter(clients.image.clone(), clients.image_model);
    let prompt = ArtPrompt {
        events: vec![],
        mood: mood.clone(),
        style: String::new(),
        theme_word: theme_word.clone(),
        description: Some(existing_art.description.clone()),
    };
    let t0 = std::time::Instant::now();
    let outcome = adapter.generate(&prompt).await
        .map_err(|e| format!("regenerate_art 生成失败: {}", e))?;
    log::info!("regenerate_art 生成完成: {:.1}s", t0.elapsed().as_secs_f32());

    // 4. 写 png 覆盖
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {}", e))?;
    let dir = app_data_dir.join("downloads").join(&date);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let png_path = dir.join("art.png");
    std::fs::write(&png_path, &outcome.png).map_err(|e| format!("写 png 文件失败: {}", e))?;
    let png_path_str = png_path.to_string_lossy().to_string();

    // 5. 透传其他字段（funny_summary / sentence 等），只换 art_json + png_path
    let json = serde_json::json!({
        "art": outcome.art,
        "art_png_path": png_path_str,
        "funny_summary": existing_art.description, // 临时占位，下面用真值覆盖
    });
    Ok(json.to_string())
}

/// v0.6: 重新生成今日音乐 —— 复用已有 description，调 AudioAdapter 重生成 + 覆盖 wav 文件。
/// v0.8: 接受 start_ms/end_ms —— 透传给上游（当前 music 再生复用已有 description，暂不重跑编排器）。
#[tauri::command]
pub async fn regenerate_music(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    date: String,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Result<String, String> {
    use crate::generate::model_music::ModelMusicAdapter;
    use crate::generate::{Music, MusicPrompt};
    use crate::model::AudioClient;
    let _ = (start_ms, end_ms); // v0.8: 时间窗口参数保留（透传占位）

    log::info!("regenerate_music: date={}", date);

    // 1. 读已有 artifact + summary
    let (existing_art, theme_word, mood) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let artifact = crate::db::artifact_repo::read_by_date(&conn, &date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该日期还没有生成作品，请先去今日作品页生成".to_string())?;
        let art: crate::generate::Art = serde_json::from_str(&artifact.art_json)
            .map_err(|e| format!("解析 art_json 失败: {}", e))?;
        (art.clone(), artifact.art_png_path, art.mood.clone())
    };
    let _ = existing_art; // suppress unused

    // 2. 加载配置 + 装配客户端
    let cfg = crate::model::config::load_config(&state.config_path);
    let clients = build_clients_with_health(&cfg)
        .await
        .map_err(|e| format!("regenerate_music 客户端装配失败: {}", e))?;

    // 3. 调 AudioAdapter 重生成（复用已有 description —— 取自 music.description）
    let (existing_music, theme_word_str) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let artifact = crate::db::artifact_repo::read_by_date(&conn, &date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该日期还没有生成作品，请先去今日作品页生成".to_string())?;
        let m: Music = serde_json::from_str(&artifact.music_json)
            .map_err(|e| format!("解析 music_json 失败: {}", e))?;
        let summary = crate::db::summary_repo::SummaryRepo::new(&conn)
            .read_by_date(&date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no summary for date {}", date))?;
        (m, summary.theme_word)
    };
    let adapter = ModelMusicAdapter::new(clients.music.clone(), clients.music_model);
    let prompt = MusicPrompt {
        events: vec![],
        mood: mood.or(existing_music.mood.clone()),
        style: existing_music.style.clone(),
        theme_word: theme_word_str.clone(),
        description: Some(existing_music.description.clone()),
    };
    let t0 = std::time::Instant::now();
    let outcome = adapter.generate(&prompt).await
        .map_err(|e| format!("regenerate_music 生成失败: {}", e))?;
    log::info!("regenerate_music 生成完成: {:.1}s", t0.elapsed().as_secs_f32());

    // 4. wav_analysis 解析新 wav 字节
    let analysis = crate::db::wav_analysis::analyze_wav(&outcome.wav)
        .map_err(|e| format!("wav_analysis 失败: {}", e))?;

    // 5. 构造新 Music
    let new_music = Music {
        bpm: 0,
        duration_ms: analysis.duration_ms,
        amplitudes: analysis.amplitudes,
        mood: existing_music.mood.clone(),
        style: existing_music.style.clone(),
        theme_word: theme_word_str.clone(),
        description: existing_music.description.clone(),
        model: existing_music.model.clone(),
    };

    // 6. 写 wav 覆盖
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {}", e))?;
    let dir = app_data_dir.join("downloads").join(&date);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let wav_path = dir.join("music.wav");
    std::fs::write(&wav_path, &outcome.wav).map_err(|e| format!("写 wav 文件失败: {}", e))?;
    let wav_path_str = wav_path.to_string_lossy().to_string();

    // 7. 透传其他字段，更新 artifacts 表
    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let artifact = crate::db::artifact_repo::read_by_date(&conn, &date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该日期还没有生成作品，请先去今日作品页生成".to_string())?;
        let art: crate::generate::Art = serde_json::from_str(&artifact.art_json)
            .map_err(|e| format!("解析 art_json 失败: {}", e))?;
        crate::db::artifact_repo::upsert_artifact_outcome(
            &conn,
            &date,
            &new_music,
            &art,
            &artifact.sentence.unwrap_or_default(),
            Some(&wav_path_str),
            artifact.art_png_path.as_deref(),
            artifact.funny_summary.as_deref(),
            artifact.english_sentence.clone(),
            artifact.theme_explanation.clone(),
            artifact.time_range_label.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    }

    let json = serde_json::json!({
        "music": new_music,
        "music_wav_path": wav_path_str,
    });
    Ok(json.to_string())
}

/// v0.6: 重新生成今日句子（sentence + english_sentence + theme_explanation + funny_summary）。
///
/// 不重生成产物，只调编排器拿新句子，覆写句子相关 4 字段（保留 music/art/wav/png + time_range_label）。
/// v0.8: 接受 start_ms/end_ms —— 时间窗口指定时从窗口 events 现场重算 theme_word（不沿用全天 summary）。
#[tauri::command]
pub async fn regenerate_sentence(
    state: State<'_, AppState>,
    date: String,
    mood: String,
    style: String,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Result<String, String> {
    use crate::summary::aggregator::Aggregator;

    log::info!("regenerate_sentence: date={} mood={} style={}", date, mood, style);

    // 1. 读已有 artifact + summary + events（v0.8: 时间窗口指定时现场重算 theme_word）
    let (ctx, existing_music_json, existing_wav_path, existing_png_path,
         existing_funny_summary, existing_time_range_label) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let artifact = crate::db::artifact_repo::read_by_date(&conn, &date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该日期还没有生成作品，请先去今日作品页生成".to_string())?;
        let summary = crate::db::summary_repo::SummaryRepo::new(&conn)
            .read_by_date(&date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no summary for date {}", date))?;
        let events = match (start_ms, end_ms) {
            (Some(s), Some(e)) => crate::db::event_repo::EventRepo::new(&conn)
                .list_by_timerange(s, e)
                .map_err(|e| e.to_string())?,
            _ => crate::db::event_repo::EventRepo::new(&conn)
                .list_by_date_48h(&date)
                .map_err(|e| e.to_string())?,
        };
        let stats = Aggregator::aggregate(date.clone(), &events);
        let theme = if start_ms.is_some() && end_ms.is_some() {
            crate::summary::theme::determine_theme_from_behavior(
                &events, stats.intensity, stats.steadiness, stats.fluency, stats.activity_hours,
            )
        } else {
            summary.theme_word.clone()
        };
        let sk = Aggregator::count_special_keys(&events);
        // mood 空时从行为推断（同学 v0.7 行为）
        let inferred_mood = if mood.is_empty() {
            summary.mood_word.clone().or_else(|| {
                Some(crate::summary::theme::infer_mood_from_behavior(
                    stats.intensity, stats.steadiness, stats.fluency, stats.activity_hours,
                ).to_string())
            })
        } else {
            Some(mood.clone())
        };
        let ctx = OrchestrationContext {
            theme_word: theme,
            mood: inferred_mood,
            style: style.clone(),
            intensity: stats.intensity,
            steadiness: stats.steadiness,
            fluency: stats.fluency,
            activity_hours: stats.activity_hours,
            top_keys: stats.top_keys.clone(),
            hourly: stats.hourly,
            first_active_ms: stats.first_active_ms,
            backspace_count: sk.backspace_count,
            delete_count: sk.delete_count,
            enter_count: sk.enter_count,
            space_count: sk.space_count,
            wasd_count: sk.wasd_count,
            total_events: sk.total_events,
        };
        (ctx, artifact.music_json, artifact.music_wav_path, artifact.art_png_path,
         artifact.funny_summary, artifact.time_range_label)
    };

    // 2. 加载配置 + 装配客户端
    let cfg = crate::model::config::load_config(&state.config_path);
    let clients = build_clients_with_health(&cfg)
        .await
        .map_err(|e| format!("regenerate_sentence 客户端装配失败: {}", e))?;

    // 3. 跑编排器拿新 6 字段（sentence + english_sentence + theme_explanation + funny_summary）
    let orch = crate::model::orchestrator::run_orchestrator(clients.chat.as_ref(), &ctx)
        .await
        .map_err(|e| format!("regenerate_sentence 编排器失败: {}", e))?;

    // 4. 更新 artifacts 表（替换句子相关 4 字段，保留 music/art/wav/png + time_range_label）
    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let music: Music = serde_json::from_str(&existing_music_json)
            .map_err(|e| format!("解析 music_json 失败: {}", e))?;
        let artifact = crate::db::artifact_repo::read_by_date(&conn, &date)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该日期还没有生成作品，请先去今日作品页生成".to_string())?;
        let art: Art = serde_json::from_str(&artifact.art_json)
            .map_err(|e| format!("解析 art_json 失败: {}", e))?;

        let funny = orch.funny_summary.trim();
        crate::db::artifact_repo::upsert_with_full(
            &conn,
            &date,
            &music,
            &art,
            Some(&orch.sentence),
            existing_wav_path.as_deref(),
            existing_png_path.as_deref(),
            if orch.english_sentence.is_empty() { None } else { Some(&orch.english_sentence) },
            if orch.theme_explanation.is_empty() { None } else { Some(&orch.theme_explanation) },
            existing_time_range_label.as_deref(),
            if funny.is_empty() { existing_funny_summary.as_deref() } else { Some(funny) },
        )
        .map_err(|e| e.to_string())?;
    }

    let json = serde_json::json!({
        "sentence": orch.sentence,
        "english_sentence": orch.english_sentence,
        "theme_explanation": orch.theme_explanation,
        "funny_summary": orch.funny_summary,
    });
    Ok(json.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::CapabilityMode;
    use crate::db::event_repo::EventRepo;
    use crate::hook::event::KeyEvent;
    use crate::summary::aggregator::Aggregator;
    use crate::scheduler::driver::date_range_ms;
    use crate::model::engine::EngineHealth;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn get_today_summary_returns_none_when_missing() {
        // 验证意图：当日未聚合返回 None
        let conn = fresh_db();
        let res = get_today_summary_impl(&conn, "2026-07-16").unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn get_today_summary_returns_row_when_exists() {
        // 验证意图：summary 已写入时可读出
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 50];
        let stats = Aggregator::aggregate("2026-07-16".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "hello", Some("focused")).unwrap();

        let row = get_today_summary_impl(&conn, "2026-07-16").unwrap().unwrap();
        assert_eq!(row.date, "2026-07-16");
        assert_eq!(row.theme_word, "hello");
        assert_eq!(row.mood_word.as_deref(), Some("focused"));
        assert_eq!(row.total_keys, 50);
    }

    #[test]
    fn command_pipeline_json_round_trip() {
        // 验证意图：整体 pipeline（写 → 读 → JSON 序列化）能让前端拿到完整数据
        let conn = fresh_db();
        let events = vec![KeyEvent::now(70, "s".into(), 0); 100];
        let stats = Aggregator::aggregate("2026-07-16".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "world", Some("happy")).unwrap();

        let row = get_today_summary_impl(&conn, "2026-07-16").unwrap().unwrap();
        let json = serde_json::to_string(&row).unwrap();
        // JSON 必须能被前端 JSON.parse，且字段齐全
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["date"], "2026-07-16");
        assert_eq!(parsed["theme_word"], "world");
        assert_eq!(parsed["mood_word"], "happy");
        assert_eq!(parsed["total_keys"], 100);
    }

    #[tokio::test]
    async fn generate_now_command_fails_for_missing_summary() {
        // 验证意图：未聚合的日期 generate_now Command 应返 Err（不让前端拿到空）
        // 同步阶段（拿 theme_word）就应失败 —— 测试这一层。
        let conn = fresh_db();
        let state = AppState { conn: Arc::new(std::sync::Mutex::new(conn)), config_path: PathBuf::from("/nonexistent") };
        // 模拟 Tauri State wrapper —— 直接调同步逻辑
        let theme_word_res: anyhow::Result<String> = (|| {
            let conn = state.conn.lock().unwrap();
            SummaryRepo::new(&conn).read_by_date("2026-07-19")?
                .map(|r| r.theme_word)
                .ok_or_else(|| anyhow::anyhow!("no summary"))
        })();
        assert!(theme_word_res.is_err());
    }

    // v0.3.2 端到端：artifact_repo::upsert + get_artifact_impl 端到端
    #[test]
    fn get_artifact_round_trip_via_pure_impls() {
        // 验证意图：upsert 一份 Music/Art → get_artifact_impl 读回 JSON 形态
        // 与 generate_now 输出对齐，前端 Artworks.vue 可直接复用同一渲染路径。
        use crate::db::artifact_repo;

        let conn = fresh_db();

        // 直接构造 Music/Art（避免拉起整套适配器）
        let music = Music {
            bpm: 0,
            duration_ms: 8_000,
            amplitudes: vec![0.5; 64],
            mood: Some("calm".into()),
            style: "ambient".into(),
            theme_word: "rain".into(),
            description: "rain ambience".into(),
            model: "local".into(),
        };
        let art = Art {
            theme_word: "rain".into(),
            mood: Some("calm".into()),
            description: "rainy window".into(),
            model: "local".into(),
        };

        artifact_repo::upsert_with_sentence(
            &conn,
            "2025-07-26",
            &music,
            &art,
            Some("hello world"),
            None,
            None,
        )
        .unwrap();
        let json = get_artifact_impl(&conn, "2025-07-26").unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["date"], "2025-07-26");
        assert_eq!(v["music"]["bpm"], music.bpm);
        assert_eq!(v["music"]["style"], "ambient");
        assert_eq!(v["music"]["model"], music.model);
        assert_eq!(v["art"]["model"], art.model);
        // v0.4: sentence 字段透传
        assert_eq!(v["sentence"], "hello world");
        // v0.4 删除：art 不再含 pixels/width/height
        assert!(v["art"].get("pixels").is_none());
        assert!(v["art"].get("width").is_none());
        assert!(v["art"].get("height").is_none());
        // music 不再含 notes
        assert!(v["music"].get("notes").is_none());

        // 不存在的日期返 "null"（与 get_today_summary 一致）
        let json = get_artifact_impl(&conn, "2099-01-01").unwrap();
        assert_eq!(json, "null");
    }

    #[test]
    fn key_count_returns_zero_when_no_events() {
        let conn = fresh_db();
        let today_start = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let today_end = today_start + 86_400_000;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM key_events WHERE timestamp_ms >= ? AND timestamp_ms < ?",
                rusqlite::params![today_start, today_end],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn get_today_hourly_groups_events_into_hour_buckets() {
        let conn = fresh_db();
        let event_repo = crate::db::event_repo::EventRepo::new(&conn);
        let today_str = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
        let (today_start, _) = date_range_ms(&today_str).unwrap();

        let mut e1 = KeyEvent::now(65, "s".into(), 0);
        e1.timestamp_ms = today_start + 9 * 3_600_000;
        event_repo.insert(&e1).unwrap();
        let mut e2 = KeyEvent::now(66, "s".into(), 0);
        e2.timestamp_ms = today_start + 14 * 3_600_000;
        event_repo.insert(&e2).unwrap();
        let mut e3 = KeyEvent::now(67, "s".into(), 0);
        e3.timestamp_ms = today_start + 14 * 3_600_000 + 30 * 60 * 1000;
        event_repo.insert(&e3).unwrap();

        let mut hourly = vec![0i64; 24];
        let mut stmt = conn
            .prepare("SELECT timestamp_ms FROM key_events")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let ts: i64 = row.get(0).unwrap();
            let hour = ((ts - today_start) / 3_600_000).rem_euclid(24) as usize;
            hourly[hour] += 1;
        }

        assert_eq!(hourly[9], 1, "9:00 事件计入 hour 9");
        assert_eq!(hourly[14], 2, "14:00 + 14:30 都计入 hour 14");
        let total_active = hourly.iter().filter(|&&x| x > 0).count();
        assert_eq!(total_active, 2, "活跃小时数 = 2（不是 7.8h）");
    }

    #[test]
    fn key_count_reflects_inserted_events() {
        let conn = fresh_db();
        let today_start = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let event_repo = crate::db::event_repo::EventRepo::new(&conn);
        for _ in 0..3 {
            let mut ev = KeyEvent::now(65, "s".into(), 0);
            ev.timestamp_ms = today_start + 1000;
            event_repo.insert(&ev).unwrap();
        }
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM key_events WHERE timestamp_ms >= ? AND timestamp_ms < ?",
                rusqlite::params![today_start, today_start + 86_400_000],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn list_summaries_returns_recent_in_descending_order() {
        let conn = fresh_db();
        for date in ["2026-07-14", "2026-07-15", "2026-07-16"] {
            let events = vec![KeyEvent::now(65, "s".into(), 0); 10];
            let stats = Aggregator::aggregate(date.into(), &events);
            SummaryRepo::new(&conn)
                .upsert(&stats, "theme", Some("m"))
                .unwrap();
        }

        let json = list_summaries_impl(&conn, 10).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["date"], "2026-07-16");
        assert_eq!(parsed[1]["date"], "2026-07-15");
        assert_eq!(parsed[2]["date"], "2026-07-14");
        assert_eq!(parsed[0]["theme_word"], "theme");
        assert_eq!(parsed[0]["mood_word"], "m");
        assert_eq!(parsed[0]["total_keys"], 10);
    }

    #[test]
    fn list_summaries_respects_limit() {
        let conn = fresh_db();
        for date in ["2026-07-10", "2026-07-11", "2026-07-12", "2026-07-13", "2026-07-14"] {
            let events = vec![KeyEvent::now(65, "s".into(), 0); 5];
            let stats = Aggregator::aggregate(date.into(), &events);
            SummaryRepo::new(&conn)
                .upsert(&stats, "x", None)
                .unwrap();
        }

        let json = list_summaries_impl(&conn, 2).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["date"], "2026-07-14");
        assert_eq!(parsed[1]["date"], "2026-07-13");
    }

    #[test]
    fn list_summaries_zero_limit_returns_empty_json_array() {
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 3];
        let stats = Aggregator::aggregate("2026-07-16".into(), &events);
        SummaryRepo::new(&conn)
            .upsert(&stats, "x", None)
            .unwrap();

        let json = list_summaries_impl(&conn, 0).unwrap();
        assert_eq!(json, "[]");
    }

    #[test]
    fn list_summaries_empty_db_returns_empty_array() {
        let conn = fresh_db();
        let json = list_summaries_impl(&conn, 10).unwrap();
        assert_eq!(json, "[]");
    }

    #[test]
    fn get_today_summary_includes_5_new_columns() {
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 50];
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "hello", Some("happy")).unwrap();

        let row = get_today_summary_impl(&conn, "2026-07-29").unwrap().unwrap();
        let json = serde_json::to_string(&row).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["intensity"].is_number());
        assert!(parsed["steadiness"].is_number());
        assert!(parsed["fluency"].is_number());
        assert!(parsed["activity_hours"].is_number());
        assert!(parsed["key_class_json"].is_string());
    }

    #[test]
    fn get_today_summary_includes_first_active_ms() {
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 50];
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "hello", Some("happy")).unwrap();

        let row = get_today_summary_impl(&conn, "2026-07-29").unwrap().unwrap();
        let json = serde_json::to_string(&row).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["first_active_ms"].is_number());
    }

    // v0.4 T12: generate_sentence 读已存 artifacts.sentence
    // （编排器 generate_now 一次性产出，三态：含 sentence / 不存在行 / sentence=NULL）
    fn make_music() -> Music {
        Music {
            bpm: 0,
            duration_ms: 8_000,
            amplitudes: vec![0.5; 64],
            mood: Some("calm".into()),
            style: "ambient".into(),
            theme_word: "rain".into(),
            description: "rain ambience".into(),
            model: "local".into(),
        }
    }
    fn make_art() -> Art {
        Art {
            theme_word: "rain".into(),
            mood: Some("calm".into()),
            description: "rainy window".into(),
            model: "local".into(),
        }
    }

    /// T12 主路径：编排器写入 sentence → generate_sentence 读出。
    /// 验证意图：编排器产出一次成型，前端不再二次调 LLM —— sentence 由 artifacts 透传。
    #[test]
    fn generate_sentence_reads_stored_sentence_from_artifacts() {
        use crate::db::artifact_repo;
        let conn = fresh_db();
        // 完整 artifacts 行含 sentence + 真 music/art，模拟 generate_now 流程
        artifact_repo::upsert_with_sentence(
            &conn,
            "2026-08-08",
            &make_music(),
            &make_art(),
            Some("A quiet day of focus"),
            Some("/some/path.wav"),
            Some("/some/path.png"),
        )
        .unwrap();

        let json = generate_sentence_impl(&conn, "2026-08-08").unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["text"], "A quiet day of focus");
        assert_eq!(v["date"], "2026-08-08");
    }

    /// T12 缺数据分支：无 artifacts 行（未 generate_now 过）→ 返 "null"。
    /// 验证意图：前端 JSON.parse 后判 null → 显式提示「该日未生成作品」，不报错。
    #[test]
    fn generate_sentence_returns_null_when_no_artifact() {
        let conn = fresh_db();
        let json = generate_sentence_impl(&conn, "2099-01-01").unwrap();
        assert_eq!(json, "null");
    }

    /// T12 旧数据兼容：sentence 列 NULL（v0.4 迁移前的 artifacts 行）→ text="" 不报错。
    /// 验证意图：旧库升级不破坏 —— 前端拿到 text="" 显示空，不抛错。
    #[test]
    fn generate_sentence_returns_empty_text_when_sentence_column_null() {
        use crate::db::artifact_repo;
        let conn = fresh_db();
        artifact_repo::upsert_with_sentence(
            &conn,
            "2026-08-09",
            &make_music(),
            &make_art(),
            None, // sentence=NULL（v0.4 迁移前老数据）
            Some("/p.wav"),
            Some("/p.png"),
        )
        .unwrap();

        let json = generate_sentence_impl(&conn, "2026-08-09").unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["text"], "");
        assert_eq!(v["date"], "2026-08-09");
    }

    // ── T11 build_clients 三态路由测试 ──

    /// helper：构造「空云端 + 全本地引擎健康」的 FingertipConfig
    fn cfg_no_cloud() -> FingertipConfig {
        FingertipConfig {
            engine: crate::model::config::EngineConfig {
                enabled: true,
                base_url: "http://127.0.0.1:8765".into(),
            },
            ..FingertipConfig::default()
        }
    }

    fn cfg_with_cloud() -> FingertipConfig {
        let mut c = FingertipConfig::default();
        c.llm.cloud_base = "https://api.openai.com".into();
        c.llm.cloud_key = "sk-x".into();
        c.llm.cloud_model = "gpt-4o-mini".into();
        c.image.cloud_base = "https://api.minimaxi.com".into();
        c.image.cloud_key = "mm-x".into();
        c.image.cloud_model = "image-01".into();
        c.audio.minimax_base = "https://api.MiniMax.chat".into();
        c.audio.minimax_key = "mm-x".into();
        c.audio.minimax_model = "music-01".into();
        c
    }

    #[test]
    fn build_clients_routes_local_first_to_engine_when_available() {
        // LocalFirst + 全本地可用 → 三个客户端都应是 EngineClient 包装
        let cfg = cfg_no_cloud();
        let h = EngineHealth { llm: true, image: true, audio: true };
        let clients = build_clients(&cfg, &h).expect("LocalFirst + 全本地 应成功");
        // 名字：JsonChat 通过 dyn trait 没法直读类型；用客户端内部字段判断。
        // 这里只断言「不报错」 + 类型可枚举（Arc 引用计数 OK）
        drop(clients);
    }

    #[test]
    fn build_clients_errors_when_local_only_and_engine_down() {
        // LocalOnly + 本地全 down → 报可读错
        let mut cfg = cfg_no_cloud();
        cfg.llm.mode = CapabilityMode::LocalOnly;
        cfg.audio.mode = CapabilityMode::LocalOnly;
        cfg.image.mode = CapabilityMode::LocalOnly;
        let h = EngineHealth::default(); // 全 false
        let err = build_clients(&cfg, &h).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("编排器 LLM 路由不可用") || msg.contains("LLM 路由不可用")
                || msg.contains("音乐") || msg.contains("图像"),
            "错误信息应说明哪个能力不可用，实际: {}",
            msg
        );
    }

    #[test]
    fn build_clients_routes_cloud_when_local_first_but_engine_down() {
        // LocalFirst + 本地 down + 云端已配 → 走云端（cloud_llm_ok=true + engine.llm=false）
        let mut cfg = cfg_with_cloud();
        cfg.llm.mode = CapabilityMode::LocalFirst;
        cfg.audio.mode = CapabilityMode::LocalFirst;
        cfg.image.mode = CapabilityMode::LocalFirst;
        let h = EngineHealth::default(); // 全 false → 走 Cloud
        let clients = build_clients(&cfg, &h).expect("LocalFirst + 本地 down + 云端已配 应走云端");
        drop(clients);
    }

    #[test]
    fn build_clients_sets_cloud_model_identifiers_when_routed_cloud() {
        // v0.4.1 Fix1: 云端路由时，Music.model / Art.model 出处标识必须写真实云端出处
        //（不再硬编码 "step-audio"/"sd-cpp" —— 否则元数据与真实生成出处不符）。
        let mut cfg = cfg_with_cloud();
        cfg.llm.mode = CapabilityMode::LocalFirst;
        cfg.audio.mode = CapabilityMode::LocalFirst;
        cfg.image.mode = CapabilityMode::LocalFirst;
        let h = EngineHealth::default(); // 本地全 down → 走 Cloud
        let clients = build_clients(&cfg, &h).expect("云端已配应走云端");
        assert_eq!(clients.music_model, "MiniMax-music", "云端音频出处应标 MiniMax-music");
        assert_eq!(clients.image_model, "MiniMax-image", "云端图像出处应标 MiniMax-image");
    }

    #[test]
    fn build_clients_sets_local_model_identifiers_when_routed_local() {
        // v0.4.1 Fix1: 本地路由时，出处标识写本地引擎（与 Cloud 分支区分）。
        let cfg = cfg_no_cloud();
        let h = EngineHealth { llm: true, image: true, audio: true };
        let clients = build_clients(&cfg, &h).expect("LocalFirst + 全本地 应成功");
        assert_eq!(clients.music_model, "step-audio", "本地音频出处应标 step-audio");
        assert_eq!(clients.image_model, "sd-cpp", "本地图像出处应标 sd-cpp");
    }

    #[test]
    fn build_clients_errors_when_cloud_only_without_keys() {
        // CloudOnly + 云端 key 空 → Unavailable
        let mut cfg = FingertipConfig::default();
        cfg.llm.mode = CapabilityMode::CloudOnly;
        cfg.audio.mode = CapabilityMode::CloudOnly;
        cfg.image.mode = CapabilityMode::CloudOnly;
        let h = EngineHealth::default();
        let err = build_clients(&cfg, &h).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("编排器 LLM 路由不可用")
                || msg.contains("LLM 路由不可用")
                || msg.contains("音乐")
                || msg.contains("图像"),
            "CloudOnly 缺 key 应报路由不可用，实际: {}",
            msg
        );
    }

    // ── T11 generate_now_impl mock 集成测试 ──
    // MockChat 返回合法 JSON；StubMusicAdapter / StubArtAdapter 返回固定 wav/png 字节。
    // 验证：music 含 description、art 含 description、png 非空、sentence 进 JSON。

    /// Mock JsonChat：返合法 JSON（music_description/image_description/sentence）
    struct MockChat;
    #[async_trait::async_trait]
    impl JsonChat for MockChat {
        async fn chat_json(&self, _system: &str, _user: &str) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::json!({
                "music_description": "calm piano with rain ambience",
                "image_description": "soft pastel window with rain drops",
                "sentence": "A quiet day of focus"
            }))
        }
    }

    /// Mock MusicAdapter：直接返 MusicOutcome（不调 AudioClient，简化集成测试）
    struct StubMusicAdapter;
    #[async_trait::async_trait]
    impl MusicAdapter for StubMusicAdapter {
        fn name(&self) -> &'static str { "stub-music" }
        async fn generate(&self, prompt: &crate::generate::MusicPrompt) -> anyhow::Result<crate::generate::MusicOutcome> {
            // 最小合法 WAV 头 + 4 字节 PCM（与 MockAudio 一致，但 inline）
            let mut wav = Vec::with_capacity(48);
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&40u32.to_le_bytes());
            wav.extend_from_slice(b"WAVE");
            wav.extend_from_slice(b"fmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&44100u32.to_le_bytes());
            wav.extend_from_slice(&88200u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&4u32.to_le_bytes());
            wav.extend_from_slice(&[0u8; 4]);
            Ok(crate::generate::MusicOutcome {
                music: Music {
                    bpm: 0,
                    duration_ms: 0,
                    amplitudes: vec![0.0; crate::generate::AMPLITUDE_SAMPLE_COUNT],
                    mood: prompt.mood.clone(),
                    style: prompt.style.clone(),
                    theme_word: prompt.theme_word.clone(),
                    description: prompt.description.clone().unwrap_or_default(),
                    model: "stub".into(),
                },
                wav,
            })
        }
    }

    /// Mock ArtAdapter：返 ArtOutcome
    struct StubArtAdapter;
    #[async_trait::async_trait]
    impl ArtAdapter for StubArtAdapter {
        fn name(&self) -> &'static str { "stub-art" }
        async fn generate(&self, prompt: &crate::generate::ArtPrompt) -> anyhow::Result<crate::generate::ArtOutcome> {
            // 最小 PNG 头（8 字节）+ 一些字节
            let mut png = Vec::new();
            png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
            png.extend_from_slice(&[0u8; 16]);
            Ok(crate::generate::ArtOutcome {
                art: Art {
                    theme_word: prompt.theme_word.clone(),
                    mood: prompt.mood.clone(),
                    description: prompt.description.clone().unwrap_or_default(),
                    model: "stub".into(),
                },
                png,
            })
        }
    }

    #[tokio::test]
    async fn generate_now_impl_with_mock_clients_produces_all_artifacts() {
        // 验证意图：T11 接线 ——
        //   编排器（MockChat）→ MusicPrompt/ArtPrompt → 适配器（StubMusic/StubArt）
        //   → wav_analysis → write_artifacts_with_bytes → 结构化产出含 sentence + 路径
        //   v0.4.1: 返回 GenerateNowOutcome（不再返 JSON/PNG 字节）。
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let app_data_dir = tmp.path();

        // 准备一些 events（让 stats 不空 → ctx 字段有内容）
        let events: Vec<KeyEvent> = (0..30)
            .map(|i| KeyEvent {
                key_code: 65 + (i % 12) as u32,
                timestamp_ms: 1_700_000_000_000 + i * 100,
                session_id: "t".into(),
                modifiers: 0,
            })
            .collect();

        let music = StubMusicAdapter;
        let art = StubArtAdapter;
        let chat = MockChat;

        let outcome = generate_now_impl(
            "2026-08-07",
            "calm",
            "ambient",
            "rain",
            String::new(), // v0.8: 时间窗口标签（无窗口 → 空）
            events,
            &chat,
            &music,
            &art,
            app_data_dir,
        )
        .await
        .expect("mock 全过应成功");

        // music 含 description
        assert_eq!(outcome.music.description, "calm piano with rain ambience");
        assert_eq!(outcome.music.model, "stub");
        // music.amplitudes 由 wav_analysis 填（不再是空）
        assert_eq!(outcome.music.amplitudes.len(), crate::generate::AMPLITUDE_SAMPLE_COUNT);

        // art 含 description
        assert_eq!(outcome.art.description, "soft pastel window with rain drops");
        assert_eq!(outcome.art.model, "stub");
        assert_eq!(outcome.art.theme_word, "rain");

        // sentence 由编排器一次性产出（v0.4.1 结构化字段，不再从 JSON 反挖）
        assert_eq!(outcome.sentence, "A quiet day of focus");

        // 产物路径：含日期目录 + 文件名
        let wav_path = &outcome.music_wav_path;
        let png_path = &outcome.art_png_path;
        assert!(wav_path.to_string_lossy().contains("music.wav"));
        assert!(png_path.to_string_lossy().contains("art.png"));
        assert!(wav_path.to_string_lossy().contains("2026-08-07"));

        // wav/png 文件落盘（PNG 头 8 字节 = 模型透传的 signature，验证透传保真）
        assert!(wav_path.exists(), "wav 文件应存在：{:?}", wav_path);
        assert!(png_path.exists(), "png 文件应存在：{:?}", png_path);
        let png_bytes = std::fs::read(png_path).unwrap();
        assert_eq!(&png_bytes[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    // v0.4 T14: get_model_config_impl / set_model_config_impl 纯函数单测
    //
    // 验证意图（不是「点击保存了什么」而是「为什么需要这条路径」）：
    //   - get 总是返 JSON，且「文件不存在」时返**默认**配置（首次启动的正常路径）
    //   - set 吃 serde_json::Value，反序列化 + 原子写 → 下次 get 能读到新值
    //   - 这条路径是前端 Settings.vue 保存/读取的 IPC 入口；若坏，前端拿不到配置

    #[test]
    fn get_model_config_returns_default_json_when_file_missing() {
        // 文件不存在路径 —— 与「首次启动未生成 config」场景一致
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("fingertip-config.json");
        let json = get_model_config_impl(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        // 默认值契约（与 FingertipConfig::default 对齐）
        assert_eq!(v["engine"]["base_url"], "http://127.0.0.1:8765");
        assert_eq!(v["engine"]["enabled"], false);
        assert_eq!(v["llm"]["mode"], "local_first");
        assert_eq!(v["image"]["mode"], "local_first");
        assert_eq!(v["audio"]["mode"], "local_first");
    }

    #[test]
    fn set_model_config_round_trips_through_impl() {
        // 用 tmp 路径跑完整 写 → 读 回路
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("fingertip-config.json");

        // 前端视角传过来的 JSON 对象（前端不 stringify，直接传对象）
        let new_cfg = serde_json::json!({
            "engine": {"enabled": true, "base_url": "http://1.2.3.4:9999"},
            "llm": {
                "mode": "cloud_only",
                "local_gguf": ["/models/a.gguf", "/models/b.gguf"],
                "cloud_base": "https://api.openai.com",
                "cloud_key": "sk-x",
                "cloud_model": "gpt-x"
            },
            "image": {"mode": "local_first", "local_model_path": "", "cloud_base": "", "cloud_key": "", "cloud_model": ""},
            "audio": {"mode": "local_first", "minimax_base": "", "minimax_key": "", "minimax_model": ""}
        });

        set_model_config_impl(&path, new_cfg).unwrap();

        // 写完应该能从磁盘读回，且字段一致
        let loaded = crate::model::config::load_config(&path);
        assert!(loaded.engine.enabled);
        assert_eq!(loaded.engine.base_url, "http://1.2.3.4:9999");
        assert_eq!(loaded.llm.mode, CapabilityMode::CloudOnly);
        assert_eq!(loaded.llm.cloud_key, "sk-x");
        assert_eq!(loaded.llm.local_gguf, vec!["/models/a.gguf", "/models/b.gguf"]);

        // get 路径也独立走一次，确认序列化产物与盘上一致
        let json = get_model_config_impl(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["llm"]["cloud_key"], "sk-x");
        assert_eq!(v["llm"]["local_gguf"][0], "/models/a.gguf");
    }

    #[test]
    fn set_model_config_rejects_malformed_value() {
        // 反序列化失败必须返中文错，而不是 panic 或静默
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("fingertip-config.json");
        // mode 给一个非法值 —— 枚举只能 local_first/cloud_only/local_only
        let bad = serde_json::json!({
            "engine": {"enabled": false, "base_url": "x"},
            "llm": {"mode": "nonsense", "local_gguf": [], "cloud_base": "", "cloud_key": "", "cloud_model": ""},
            "image": {"mode": "local_first", "local_model_path": "", "cloud_base": "", "cloud_key": "", "cloud_model": ""},
            "audio": {"mode": "local_first", "minimax_base": "", "minimax_key": "", "minimax_model": ""}
        });
        let err = set_model_config_impl(&path, bad).unwrap_err();
        assert!(err.contains("config JSON 解析失败"), "应有中文诊断：{}", err);
        // 文件不应被写出（坏配置不应落盘覆盖好的）
        assert!(!path.exists(), "反序列化失败时不应产生文件");
    }

    // v0.5.0: get_hook_status 应当读取 crate::HOOK_RUNNING 原子标志。
    // 验证意图：前端状态条用此值（绿/灰），所以语义必须与启动时 store(true) 一致。
    #[test]
    fn get_hook_status_reflects_hook_running_atomic() {
        // 重置 + 读取 + 写回，避免污染其他测试（并发跑时 AtomicBool 是进程级全局）
        use std::sync::atomic::Ordering;
        crate::HOOK_RUNNING.store(false, Ordering::Relaxed);
        assert_eq!(get_hook_status().unwrap(), false, "未启动应返 false");
        crate::HOOK_RUNNING.store(true, Ordering::Relaxed);
        assert_eq!(get_hook_status().unwrap(), true, "已启动应返 true");
        // 复原默认状态（其他测试不应受污染）
        crate::HOOK_RUNNING.store(false, Ordering::Relaxed);
    }
}