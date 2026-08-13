use rusqlite::Connection;

/// 数据库迁移入口。
///
/// 验证意图：建表语句集中管理，所有表一次性建立，避免漏建。
/// Phase 1: key_events 表（原始事件）
/// Phase 3: daily_summary 表（日聚合，预留）
/// v0.3.2: artifacts 表（生成产物持久化，Music/Art JSON）
/// v0.3.4: artifacts 加 music_wav_path / art_png_path 字段（写文件落盘）
/// v0.3.5: daily_summary 加 intensity/steadiness/fluency/activity_hours/key_class_json 5 列
/// v0.3.6: daily_summary 加 first_active_ms 列（首按时间）
/// v0.4:   artifacts 加 sentence 列（编排器产出的一次成型描述，前端 Artworks 挂载时直接读）
/// v0.6.0: artifacts 加 4 列（english_sentence / theme_explanation / time_range_label / funny_summary）——
///         编排器 + 重新生成 + 落地页分享用
pub fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS key_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key_code INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            session_id TEXT NOT NULL,
            modifiers INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_key_events_ts ON key_events(timestamp_ms);
        CREATE INDEX IF NOT EXISTS idx_key_events_session ON key_events(session_id);

        CREATE TABLE IF NOT EXISTS daily_summary (
            date TEXT PRIMARY KEY,
            total_keys INTEGER NOT NULL,
            top_keys_json TEXT NOT NULL,
            theme_word TEXT NOT NULL,
            mood_word TEXT,
            -- v0.3.5 新增
            intensity REAL NOT NULL DEFAULT 0.0,
            steadiness REAL NOT NULL DEFAULT 0.0,
            fluency REAL NOT NULL DEFAULT 0.0,
            activity_hours INTEGER NOT NULL DEFAULT 0,
            key_class_json TEXT NOT NULL DEFAULT '{}',
            -- v0.3.6 新增
            first_active_ms INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS artifacts (
            date TEXT PRIMARY KEY,
            music_json TEXT NOT NULL,
            art_json TEXT NOT NULL,
            music_wav_path TEXT,
            art_png_path TEXT,
            -- v0.4 新增：编排器产出的一次成型描述
            sentence TEXT,
            -- v0.6.0 新增：英文句子 + 主题词解释 + 时间窗口标签 + AI 键盘诊断（搞笑）
            english_sentence TEXT,
            theme_explanation TEXT,
            time_range_label TEXT,
            funny_summary TEXT,
            created_at INTEGER NOT NULL
        );
        ",
    )?;

    // v0.3.5: 兼容老库（无 migration 版本号机制）
    // 检查 intensity 列是否存在；不存在则 ADD COLUMN
    let has_intensity: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('daily_summary') WHERE name='intensity'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !has_intensity {
        log::warn!("daily_summary 缺 5 个新列（v0.3.5）—— 自动 ALTER TABLE ADD COLUMN");
        conn.execute_batch(
            "ALTER TABLE daily_summary ADD COLUMN intensity REAL NOT NULL DEFAULT 0.0;
             ALTER TABLE daily_summary ADD COLUMN steadiness REAL NOT NULL DEFAULT 0.0;
             ALTER TABLE daily_summary ADD COLUMN fluency REAL NOT NULL DEFAULT 0.0;
             ALTER TABLE daily_summary ADD COLUMN activity_hours INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE daily_summary ADD COLUMN key_class_json TEXT NOT NULL DEFAULT '{}';"
        )?;
    }

    // v0.3.6: first_active_ms 列
    let has_first_active_ms: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('daily_summary') WHERE name='first_active_ms'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !has_first_active_ms {
        log::warn!("daily_summary 缺 first_active_ms 列（v0.3.6）—— 自动 ALTER TABLE ADD COLUMN");
        conn.execute_batch(
            "ALTER TABLE daily_summary ADD COLUMN first_active_ms INTEGER NOT NULL DEFAULT 0;"
        )?;
    }

    // v0.4: artifacts 加 sentence 列（兼容老库）
    let has_sentence: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('artifacts') WHERE name='sentence'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !has_sentence {
        log::warn!("artifacts 缺 sentence 列（v0.4）—— 自动 ALTER TABLE ADD COLUMN");
        conn.execute_batch(
            "ALTER TABLE artifacts ADD COLUMN sentence TEXT;"
        )?;
    }

    // v0.6.0: artifacts 加 4 列（english_sentence / theme_explanation / time_range_label / funny_summary）
    // 一次 ALTER 全部加上，按列名逐一检测确保幂等
    let new_columns: &[(&str, &str)] = &[
        ("english_sentence", "TEXT"),
        ("theme_explanation", "TEXT"),
        ("time_range_label", "TEXT"),
        ("funny_summary", "TEXT"),
    ];
    for (col_name, col_type) in new_columns {
        let has_col: bool = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_table_info('artifacts') WHERE name='{}'", col_name),
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if !has_col {
            log::warn!("artifacts 缺 {} 列（v0.6.0）—— 自动 ALTER TABLE ADD COLUMN", col_name);
            conn.execute_batch(
                &format!("ALTER TABLE artifacts ADD COLUMN {} {};", col_name, col_type)
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn count_columns(conn: &Connection, table: &str) -> i64 {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{}')", table),
            [],
            |r| r.get(0),
        ).unwrap()
    }

    #[test]
    fn fresh_db_has_all_5_new_columns() {
        // 验证意图：v0.3.5 新库一次性建出 5 列
        let conn = fresh_db();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('daily_summary')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for name in &["intensity", "steadiness", "fluency", "activity_hours", "key_class_json"] {
            assert!(cols.iter().any(|c| c == name), "missing column: {}", name);
        }
    }

    #[test]
    fn old_db_gets_columns_via_alter() {
        // 验证意图：模拟老库（6 列 daily_summary）→ run_migrations 后 ALTER 出 5 列
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE daily_summary (
                date TEXT PRIMARY KEY,
                total_keys INTEGER NOT NULL,
                top_keys_json TEXT NOT NULL,
                theme_word TEXT NOT NULL,
                mood_word TEXT,
                created_at INTEGER NOT NULL
            );"
        ).unwrap();

        // 确认是 6 列（老 schema）
        assert_eq!(count_columns(&conn, "daily_summary"), 6);

        // 跑 run_migrations → 应自动 ALTER 加 5 列
        run_migrations(&conn).unwrap();

        // 变成 11 列  → 改成 12 列
        assert_eq!(count_columns(&conn, "daily_summary"), 12);

        // 5 个新列都存在
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('daily_summary')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for name in &["intensity", "steadiness", "fluency", "activity_hours", "key_class_json"] {
            assert!(cols.iter().any(|c| c == name), "ALTER failed to add: {}", name);
        }
    }

    #[test]
    fn old_db_existing_row_keeps_old_data_with_defaults() {
        // 验证意图：老库已有行不会被 ALTER 删；新列默认值生效
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE daily_summary (
                date TEXT PRIMARY KEY,
                total_keys INTEGER NOT NULL,
                top_keys_json TEXT NOT NULL,
                theme_word TEXT NOT NULL,
                mood_word TEXT,
                created_at INTEGER NOT NULL
            );
            INSERT INTO daily_summary VALUES ('2026-07-28', 100, '[]', 'hello', 'happy', 1000);"
        ).unwrap();

        run_migrations(&conn).unwrap();

        // 老行还在，新列有默认值
        let row: (i64, f64, String) = conn.query_row(
            "SELECT total_keys, intensity, theme_word FROM daily_summary WHERE date = '2026-07-28'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(row.0, 100);
        assert_eq!(row.1, 0.0);
        assert_eq!(row.2, "hello");
    }

    #[test]
    fn fresh_db_has_first_active_ms_column() {
        let conn = fresh_db();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('daily_summary')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(cols.iter().any(|c| c == "first_active_ms"), "missing column: first_active_ms");
    }

    #[test]
    fn old_db_with_first_active_ms_already_present_skips_alter() {
        // 验证意图：v0.3.6 已升级的库不应重复 ALTER（避免无意义的 schema 检查开销）
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE daily_summary (
                date TEXT PRIMARY KEY,
                total_keys INTEGER NOT NULL,
                top_keys_json TEXT NOT NULL,
                theme_word TEXT NOT NULL,
                mood_word TEXT,
                intensity REAL NOT NULL DEFAULT 0.0,
                steadiness REAL NOT NULL DEFAULT 0.0,
                fluency REAL NOT NULL DEFAULT 0.0,
                activity_hours INTEGER NOT NULL DEFAULT 0,
                key_class_json TEXT NOT NULL DEFAULT '{}',
                first_active_ms INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );"
        ).unwrap();

        // 跑 run_migrations —— 不应失败，列已存在
        run_migrations(&conn).unwrap();
        assert_eq!(count_columns(&conn, "daily_summary"), 12);
    }

    // v0.4: artifacts.sentence 列
    #[test]
    fn artifacts_table_has_sentence_column() {
        // 验证意图：新库 CREATE TABLE 直接含 sentence 列
        let conn = fresh_db();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('artifacts')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(
            cols.iter().any(|c| c == "sentence"),
            "missing column: sentence, got columns: {:?}",
            cols
        );
    }

    #[test]
    fn old_artifacts_gets_sentence_via_alter() {
        // 验证意图：v0.3.x 老的 artifacts 表（无 sentence）→ run_migrations 自动 ALTER
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifacts (
                date TEXT PRIMARY KEY,
                music_json TEXT NOT NULL,
                art_json TEXT NOT NULL,
                music_wav_path TEXT,
                art_png_path TEXT,
                created_at INTEGER NOT NULL
            );
            INSERT INTO artifacts VALUES ('2026-08-01', '{}', '{}', NULL, NULL, 1000);"
        ).unwrap();

        // 确认是老 schema（无 sentence 列）
        assert_eq!(count_columns(&conn, "artifacts"), 6);

        // 跑 run_migrations → 应自动 ALTER 加 sentence 列
        run_migrations(&conn).unwrap();

        // v0.6.0 起 11 列（原 7 + 4 新列）
        assert_eq!(count_columns(&conn, "artifacts"), 11);

        // 老行还在
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM artifacts WHERE date = '2026-08-01'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "老行未被 ALTER 删");
    }

    #[test]
    fn old_artifacts_with_sentence_already_present_skips_alter() {
        // 验证意图：v0.4 已升级的库不应重复 ALTER（幂等）
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifacts (
                date TEXT PRIMARY KEY,
                music_json TEXT NOT NULL,
                art_json TEXT NOT NULL,
                music_wav_path TEXT,
                art_png_path TEXT,
                sentence TEXT,
                created_at INTEGER NOT NULL
            );"
        ).unwrap();

        // 跑 run_migrations —— 不应失败，列已存在
        run_migrations(&conn).unwrap();
        // v0.6.0 起 artifacts 是 11 列：原 7 + english_sentence + theme_explanation + time_range_label + funny_summary
        assert_eq!(count_columns(&conn, "artifacts"), 11);
    }

    // v0.6.0: artifacts 加 4 列（english_sentence / theme_explanation / time_range_label / funny_summary）
    #[test]
    fn artifacts_table_has_v060_4_new_columns() {
        // 验证意图：新库 CREATE TABLE 直接含 4 列
        let conn = fresh_db();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('artifacts')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for name in &["english_sentence", "theme_explanation", "time_range_label", "funny_summary"] {
            assert!(cols.iter().any(|c| c == name), "missing column: {}, got: {:?}", name, cols);
        }
        // 共 11 列：date + music_json + art_json + music_wav_path + art_png_path + sentence +
        //          english_sentence + theme_explanation + time_range_label + funny_summary + created_at
        assert_eq!(count_columns(&conn, "artifacts"), 11, "v0.6.0 新库应 11 列");
    }

    #[test]
    fn old_artifacts_gets_v060_columns_via_alter() {
        // 验证意图：v0.4 老的 artifacts 表（7 列）→ run_migrations 自动 ALTER 加 4 列
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifacts (
                date TEXT PRIMARY KEY,
                music_json TEXT NOT NULL,
                art_json TEXT NOT NULL,
                music_wav_path TEXT,
                art_png_path TEXT,
                sentence TEXT,
                created_at INTEGER NOT NULL
            );
            INSERT INTO artifacts VALUES ('2026-08-01', '{}', '{}', NULL, NULL, NULL, 1000);"
        ).unwrap();

        // 确认是老 schema（7 列）
        assert_eq!(count_columns(&conn, "artifacts"), 7);

        // 跑 run_migrations → 应自动 ALTER 加 4 列
        run_migrations(&conn).unwrap();

        assert_eq!(count_columns(&conn, "artifacts"), 11);

        // 老行还在 + 新字段默认值 NULL
        let (sentence, funny): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT sentence, funny_summary FROM artifacts WHERE date = '2026-08-01'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(sentence.is_none());
        assert!(funny.is_none(), "ALTER 后老行 funny_summary 应为 NULL");
    }

    #[test]
    fn old_artifacts_with_v060_columns_already_present_skips_alter() {
        // 验证意图：v0.6.0 已升级的库不应重复 ALTER（幂等）
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifacts (
                date TEXT PRIMARY KEY,
                music_json TEXT NOT NULL,
                art_json TEXT NOT NULL,
                music_wav_path TEXT,
                art_png_path TEXT,
                sentence TEXT,
                english_sentence TEXT,
                theme_explanation TEXT,
                time_range_label TEXT,
                funny_summary TEXT,
                created_at INTEGER NOT NULL
            );"
        ).unwrap();

        run_migrations(&conn).unwrap();
        assert_eq!(count_columns(&conn, "artifacts"), 11);
    }
}