//! v0.3.2: 生成产物持久化（artifacts 表）
//!
//! 验证意图：generate_now 每次成功生成的 Music + Art 全字段序列化为 JSON 落库，
//! History.vue 点 day card 即可拉回历史作品（不依赖内存中的 generationResult）。
//!
//! v0.3.4: 加 music_wav_path / art_png_path 字段，generate_now 同步把 wav/png 写文件。
//! 文件路径绝对（用 std::fs::write，不用 Tauri plugin-fs），由 artifact_writer.rs 负责落盘。
//!
//! v0.4: 加 sentence 列 —— 编排器产出的一次成型描述（Music/Art 公用），
//! 前端 Artworks 挂载时直接读，不再二次调 LLM。
//! 新增 `upsert_with_sentence`，旧的 `upsert` / `upsert_with_paths` 保留为 wrapper。
//!
//! 设计取舍：
//!   - Music / Art 完整 JSON 化（serde_json 序列化所有字段：amplitudes / description / model / ...）
//!   - date PK 与 daily_summary 一对一 —— 重生成时 upsert 覆盖
//!   - 无 FK 约束（SQLite 默认不强制，依赖应用层一致性）—— 简单优先

use crate::generate::{Art, Music};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactRow {
    pub date: String,
    pub music_json: String,
    pub art_json: String,
    /// v0.3.4: wav 文件绝对路径（None 表示该日没写文件——v0.3.2 旧数据）
    pub music_wav_path: Option<String>,
    /// v0.3.4: png 文件绝对路径
    pub art_png_path: Option<String>,
    /// v0.4: 编排器产出的一次成型描述（Music/Art 公用）
    pub sentence: Option<String>,
    /// v0.6.0: 编排器产出的英文句子（用于落地页、Artworks 中英分行）
    #[serde(default)]
    pub english_sentence: Option<String>,
    /// v0.6.0: 编排器产出的主题词解释文案（短）
    #[serde(default)]
    pub theme_explanation: Option<String>,
    /// v0.6.0: 用户选择的时间窗口标签（如 "06:00–08:00"），无窗口则 None
    #[serde(default)]
    pub time_range_label: Option<String>,
    /// v0.6.0: AI 键盘诊断（funny_summary）—— 2 句话、40-80 字的搞笑总结
    #[serde(default)]
    pub funny_summary: Option<String>,
    pub created_at: i64,
}

/// v0.4: 写 / 覆盖某一天 artifact（含 sentence + 文件路径）
///
/// `INSERT OR REPLACE` —— 同日重新生成会整体覆盖旧值。
/// `wav_path` / `png_path` 可为 None（保留为 v0.3.2 兼容），实际生产路径会传绝对路径。
/// `sentence` 可为 None（旧数据未带描述）—— 历史读时该字段为 None，前端可 fallback。
///
/// v0.6.0: 加 4 个 Option 参数（english_sentence / theme_explanation / time_range_label / funny_summary）。
/// 调用方不关心这些字段时传 None（保留旧版语义）。通常 regenerate_* 命令透传已有字段。
pub fn upsert_with_sentence(
    conn: &Connection,
    date: &str,
    music: &Music,
    art: &Art,
    sentence: Option<&str>,
    wav_path: Option<&str>,
    png_path: Option<&str>,
) -> anyhow::Result<()> {
    upsert_with_full(
        conn,
        date,
        music,
        art,
        sentence,
        wav_path,
        png_path,
        None,
        None,
        None,
        None,
    )
}

/// v0.6.0: 全字段版 upsert（11 参数）—— 列出全部可选字段，方便 regenerate_* 命令透传。
///
/// 顺序：sentence, wav_path, png_path, english_sentence, theme_explanation, time_range_label, funny_summary。
/// None 表示该列写 NULL（INSERT OR REPLACE 整体覆盖语义下，None = 该列清空）。
///
/// 调用方应使用 `upsert_artifact_outcome` wrapper 避免记参数顺序。
pub fn upsert_with_full(
    conn: &Connection,
    date: &str,
    music: &Music,
    art: &Art,
    sentence: Option<&str>,
    wav_path: Option<&str>,
    png_path: Option<&str>,
    english_sentence: Option<&str>,
    theme_explanation: Option<&str>,
    time_range_label: Option<&str>,
    funny_summary: Option<&str>,
) -> anyhow::Result<()> {
    let music_json = serde_json::to_string(music)?;
    let art_json = serde_json::to_string(art)?;
    conn.execute(
        "INSERT OR REPLACE INTO artifacts \
         (date, music_json, art_json, music_wav_path, art_png_path, sentence, english_sentence, theme_explanation, time_range_label, funny_summary, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            date,
            music_json,
            art_json,
            wav_path,
            png_path,
            sentence,
            english_sentence,
            theme_explanation,
            time_range_label,
            funny_summary,
            chrono::Utc::now().timestamp_millis(),
        ],
    )?;
    Ok(())
}

/// v0.3.4 旧 upsert（带 paths，无 sentence）—— 保留为 wrapper，向 v0.4 upsert_with_sentence 收敛
pub fn upsert_with_paths(
    conn: &Connection,
    date: &str,
    music: &Music,
    art: &Art,
    wav_path: Option<&str>,
    png_path: Option<&str>,
) -> anyhow::Result<()> {
    upsert_with_sentence(conn, date, music, art, None, wav_path, png_path)
}

/// v0.3.2 旧 upsert（无 path / 无 sentence）—— 保留给没有 AppHandle 拿不到 app_data_dir 的测试 / 场景
#[allow(dead_code)]
pub fn upsert(
    conn: &Connection,
    date: &str,
    music: &Music,
    art: &Art,
) -> anyhow::Result<()> {
    upsert_with_sentence(conn, date, music, art, None, None, None)
}

/// v0.6.0: 一站式写库 —— 把 GenerateNowOutcome 所需的所有字段一次性 upsert。
///
/// 调用方传 sentence + 文件路径 + funny_summary + english_sentence + theme_explanation + time_range_label，
/// 避免每处都手写 11 参数。
/// 未提供 funny_summary / english_sentence / theme_explanation 时写空字符串（前端 v-if 兜底）。
/// time_range_label=None 表示没选时间窗口（48h 默认）→ 写 NULL。
pub fn upsert_artifact_outcome(
    conn: &Connection,
    date: &str,
    music: &Music,
    art: &Art,
    sentence: &str,
    wav_path: Option<&str>,
    png_path: Option<&str>,
    funny_summary: Option<&str>,
    english_sentence: Option<String>,
    theme_explanation: Option<String>,
    time_range_label: Option<&str>,
) -> anyhow::Result<()> {
    upsert_with_full(
        conn,
        date,
        music,
        art,
        Some(sentence),
        wav_path,
        png_path,
        english_sentence.as_deref(),
        theme_explanation.as_deref(),
        time_range_label,
        funny_summary,
    )
}

/// 按日期读 artifact（用于 History.vue 点击 day card 拉回作品）
pub fn read_by_date(conn: &Connection, date: &str) -> anyhow::Result<Option<ArtifactRow>> {
    let row = conn
        .query_row(
            "SELECT date, music_json, art_json, music_wav_path, art_png_path, sentence, english_sentence, theme_explanation, time_range_label, funny_summary, created_at \
             FROM artifacts WHERE date = ?",
            params![date],
            |row| {
                Ok(ArtifactRow {
                    date: row.get(0)?,
                    music_json: row.get(1)?,
                    art_json: row.get(2)?,
                    music_wav_path: row.get(3)?,
                    art_png_path: row.get(4)?,
                    sentence: row.get(5)?,
                    english_sentence: row.get(6)?,
                    theme_explanation: row.get(7)?,
                    time_range_label: row.get(8)?,
                    funny_summary: row.get(9)?,
                    created_at: row.get(10)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// 读最近 N 天的 artifact 列表（按日期倒序）
///
/// 验证意图：未来 History.vue 可能想直接列出"有作品"的日期，
/// 避免再 join daily_summary.artifacts is not null。
/// 当前 v0.3.2 仅暴露 API，UI 暂未用。
pub fn list_recent(conn: &Connection, limit: usize) -> anyhow::Result<Vec<ArtifactRow>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT date, music_json, art_json, music_wav_path, art_png_path, sentence, english_sentence, theme_explanation, time_range_label, funny_summary, created_at \
         FROM artifacts ORDER BY date DESC LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(ArtifactRow {
            date: row.get(0)?,
            music_json: row.get(1)?,
            art_json: row.get(2)?,
            music_wav_path: row.get(3)?,
            art_png_path: row.get(4)?,
            sentence: row.get(5)?,
            english_sentence: row.get(6)?,
            theme_explanation: row.get(7)?,
            time_range_label: row.get(8)?,
            funny_summary: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn make_music() -> Music {
        Music {
            bpm: 0,
            duration_ms: 16_000,
            amplitudes: vec![0.5; 64],
            mood: Some("happy".into()),
            style: "jazz".into(),
            theme_word: "test".into(),
            description: "test music".into(),
            model: "local".into(),
        }
    }

    fn make_art() -> Art {
        Art {
            theme_word: "test".into(),
            mood: Some("happy".into()),
            description: "test art".into(),
            model: "local".into(),
        }
    }

    #[test]
    fn upsert_with_sentence_round_trips() {
        // v0.4: 写 sentence → 读回保真
        let conn = fresh_db();
        upsert_with_sentence(
            &conn,
            "2026-08-08",
            &make_music(),
            &make_art(),
            Some("hello world"),
            None,
            None,
        )
        .unwrap();

        let row = read_by_date(&conn, "2026-08-08").unwrap().unwrap();
        assert_eq!(row.sentence.as_deref(), Some("hello world"));
    }

    #[test]
    fn upsert_with_sentence_optional() {
        // v0.4: sentence = None 时字段读回 None
        let conn = fresh_db();
        upsert_with_sentence(
            &conn,
            "2026-08-08",
            &make_music(),
            &make_art(),
            None,
            Some("/data/music.wav"),
            Some("/data/art.png"),
        )
        .unwrap();

        let row = read_by_date(&conn, "2026-08-08").unwrap().unwrap();
        assert!(row.sentence.is_none());
        assert_eq!(row.music_wav_path.as_deref(), Some("/data/music.wav"));
        assert_eq!(row.art_png_path.as_deref(), Some("/data/art.png"));
    }

    #[test]
    fn upsert_with_paths_round_trip() {
        // v0.3.4: 写 wav_path + png_path → 读回保真
        let conn = fresh_db();
        upsert_with_paths(
            &conn,
            "2026-07-28",
            &make_music(),
            &make_art(),
            Some("C:/Users/test/music.wav"),
            Some("C:/Users/test/art.png"),
        )
        .unwrap();

        let row = read_by_date(&conn, "2026-07-28").unwrap().unwrap();
        assert_eq!(row.music_wav_path.as_deref(), Some("C:/Users/test/music.wav"));
        assert_eq!(row.art_png_path.as_deref(), Some("C:/Users/test/art.png"));
        // v0.4 wrapper 应同时写入 sentence=None
        assert!(row.sentence.is_none());
    }

    #[test]
    fn upsert_and_read_round_trip() {
        // 验证意图：写后能完整读回，music_json / art_json 都可反序列化
        let conn = fresh_db();
        let music = make_music();
        let art = make_art();

        upsert(&conn, "2026-07-28", &music, &art).unwrap();

        let row = read_by_date(&conn, "2026-07-28").unwrap().expect("row must exist");
        assert_eq!(row.date, "2026-07-28");

        // 验证 JSON 反序列化后字段保真
        let music_back: Music = serde_json::from_str(&row.music_json).unwrap();
        assert_eq!(music_back.bpm, 0, "v0.4 Music.bpm = 0");
        assert_eq!(music_back.style, "jazz");
        assert_eq!(music_back.model, "local");
        let art_back: Art = serde_json::from_str(&row.art_json).unwrap();
        assert_eq!(art_back.theme_word, "test");
        assert_eq!(art_back.model, "local");
    }

    #[test]
    fn upsert_is_idempotent_same_date() {
        // 验证意图：同日 re-upsert 整体覆盖
        let conn = fresh_db();
        let mut music = make_music();
        music.duration_ms = 16_000;
        upsert_with_sentence(&conn, "2026-07-28", &music, &make_art(), Some("first"), None, None).unwrap();

        music.duration_ms = 20_000;
        upsert_with_sentence(&conn, "2026-07-28", &music, &make_art(), Some("second"), None, None).unwrap();

        let row = read_by_date(&conn, "2026-07-28").unwrap().unwrap();
        assert_eq!(row.sentence.as_deref(), Some("second"));
        let music_back: Music = serde_json::from_str(&row.music_json).unwrap();
        assert_eq!(music_back.duration_ms, 20_000, "二次 upsert 应覆盖 duration_ms");
    }

    #[test]
    fn read_missing_date_returns_none() {
        // 验证意图：未生成的日期返回 None（不 panic）
        let conn = fresh_db();
        assert!(read_by_date(&conn, "2030-01-01").unwrap().is_none());
    }

    #[test]
    fn list_recent_orders_descending() {
        // 验证意图：3 个 artifact 按日期倒序
        let conn = fresh_db();
        for date in ["2026-07-26", "2026-07-28", "2026-07-27"] {
            upsert(&conn, date, &make_music(), &make_art()).unwrap();
        }
        let rows = list_recent(&conn, 10).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].date, "2026-07-28");
        assert_eq!(rows[1].date, "2026-07-27");
        assert_eq!(rows[2].date, "2026-07-26");
    }

    #[test]
    fn list_recent_zero_limit_returns_empty() {
        let conn = fresh_db();
        upsert(&conn, "2026-07-28", &make_music(), &make_art()).unwrap();
        let rows = list_recent(&conn, 0).unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn list_recent_respects_limit() {
        let conn = fresh_db();
        for date in ["2026-07-20", "2026-07-21", "2026-07-22", "2026-07-23", "2026-07-24"] {
            upsert(&conn, date, &make_music(), &make_art()).unwrap();
        }
        let rows = list_recent(&conn, 2).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].date, "2026-07-24");
        assert_eq!(rows[1].date, "2026-07-23");
    }

    #[test]
    fn list_recent_includes_sentence_field() {
        // v0.4: list_recent 应透传 sentence
        let conn = fresh_db();
        upsert_with_sentence(&conn, "2026-08-08", &make_music(), &make_art(), Some("hello"), None, None).unwrap();
        upsert_with_sentence(&conn, "2026-08-09", &make_music(), &make_art(), None, None, None).unwrap();
        let rows = list_recent(&conn, 10).unwrap();
        assert_eq!(rows.len(), 2);
        // 倒序：08-09 在前
        assert_eq!(rows[0].sentence, None);
        assert_eq!(rows[1].sentence.as_deref(), Some("hello"));
    }

    // v0.6.0: upsert_with_full 11 参数版 — funny_summary + 3 兄弟字段
    #[test]
    fn upsert_with_full_round_trips_v060_fields() {
        let conn = fresh_db();
        upsert_with_full(
            &conn,
            "2026-08-12",
            &make_music(),
            &make_art(),
            Some("中文句子"),
            None,
            None,
            Some("English sentence"),
            Some("主题词解释"),
            Some("06:00–08:00"),
            Some("你的键盘在凌晨最活跃，是个夜猫子"),
        )
        .unwrap();
        let row = read_by_date(&conn, "2026-08-12").unwrap().unwrap();
        assert_eq!(row.sentence.as_deref(), Some("中文句子"));
        assert_eq!(row.english_sentence.as_deref(), Some("English sentence"));
        assert_eq!(row.theme_explanation.as_deref(), Some("主题词解释"));
        assert_eq!(row.time_range_label.as_deref(), Some("06:00–08:00"));
        assert_eq!(row.funny_summary.as_deref(), Some("你的键盘在凌晨最活跃，是个夜猫子"));
    }

    #[test]
    fn upsert_with_full_optional_fields_write_null() {
        // v0.6.0: 全部 Option=None → 对应列写 NULL（INSERT OR REPLACE 是全量替换）
        let conn = fresh_db();
        upsert_with_full(
            &conn,
            "2026-08-12",
            &make_music(),
            &make_art(),
            None, None, None, None, None, None, None,
        )
        .unwrap();
        let row = read_by_date(&conn, "2026-08-12").unwrap().unwrap();
        assert!(row.sentence.is_none());
        assert!(row.english_sentence.is_none());
        assert!(row.theme_explanation.is_none());
        assert!(row.time_range_label.is_none());
        assert!(row.funny_summary.is_none());
    }
}