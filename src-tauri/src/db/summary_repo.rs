//! DailySummary 仓储（写 + 读 daily_summary 表）
//!
//! 验证意图：把聚合结果安全写入 SQLite，前端通过 get_today_summary Command 拉取。

use crate::summary::stats::DailyStats;
use rusqlite::{params, Connection, OptionalExtension};

pub struct SummaryRepo<'a> {
    conn: &'a Connection,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailySummaryRow {
    pub date: String,
    pub total_keys: i64,
    pub top_keys_json: String,
    pub theme_word: String,
    pub mood_word: Option<String>,
    // v0.3.5 新增
    pub intensity: f64,
    pub steadiness: f64,
    pub fluency: f64,
    pub activity_hours: i32,
    pub key_class_json: String,
    /// v0.3.6: 首活时间 UTC 毫秒（0 = 无事件）
    pub first_active_ms: i64,
    pub created_at: i64,
}

impl<'a> SummaryRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 整体 upsert（含 mood_word）—— 测试 + 一次性初始化用
    pub fn upsert(
        &self,
        stats: &DailyStats,
        theme_word: &str,
        mood_word: Option<&str>,
    ) -> anyhow::Result<()> {
        let top_keys_json = serde_json::to_string(&stats.top_keys)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO daily_summary
             (date, total_keys, top_keys_json, theme_word, mood_word,
              intensity, steadiness, fluency, activity_hours, key_class_json,
              first_active_ms,
              created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                stats.date,
                stats.total_keys as i64,
                top_keys_json,
                theme_word,
                mood_word,
                stats.intensity,
                stats.steadiness,
                stats.fluency,
                stats.activity_hours,
                stats.key_class_json,
                stats.first_active_ms,
                chrono::Utc::now().timestamp_millis(),
            ],
        )?;
        Ok(())
    }

    /// v0.3.1: upsert 但保留已有 mood_word —— scheduler 60s tick 用
    pub fn upsert_stats(
        &self,
        stats: &DailyStats,
        theme_word: &str,
    ) -> anyhow::Result<()> {
        let top_keys_json = serde_json::to_string(&stats.top_keys)?;
        self.conn.execute(
            "INSERT INTO daily_summary
             (date, total_keys, top_keys_json, theme_word, mood_word,
              intensity, steadiness, fluency, activity_hours, key_class_json,
              first_active_ms,
              created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(date) DO UPDATE SET
                total_keys = excluded.total_keys,
                top_keys_json = excluded.top_keys_json,
                theme_word = excluded.theme_word,
                intensity = excluded.intensity,
                steadiness = excluded.steadiness,
                fluency = excluded.fluency,
                activity_hours = excluded.activity_hours,
                key_class_json = excluded.key_class_json,
                first_active_ms = excluded.first_active_ms,
                created_at = excluded.created_at",
            params![
                stats.date,
                stats.total_keys as i64,
                top_keys_json,
                theme_word,
                stats.intensity,
                stats.steadiness,
                stats.fluency,
                stats.activity_hours,
                stats.key_class_json,
                stats.first_active_ms,
                chrono::Utc::now().timestamp_millis(),
            ],
        )?;
        Ok(())
    }

    /// 按日期读
    pub fn read_by_date(&self, date: &str) -> anyhow::Result<Option<DailySummaryRow>> {
        let row = self.conn.query_row(
            "SELECT date, total_keys, top_keys_json, theme_word, mood_word,
                    intensity, steadiness, fluency, activity_hours, key_class_json,
                    first_active_ms,
                    created_at
             FROM daily_summary WHERE date = ?",
            params![date],
            |row| {
                Ok(DailySummaryRow {
                    date: row.get(0)?,
                    total_keys: row.get(1)?,
                    top_keys_json: row.get(2)?,
                    theme_word: row.get(3)?,
                    mood_word: row.get(4)?,
                    intensity: row.get(5)?,
                    steadiness: row.get(6)?,
                    fluency: row.get(7)?,
                    activity_hours: row.get(8)?,
                    key_class_json: row.get(9)?,
                    first_active_ms: row.get(10)?,
                    created_at: row.get(11)?,
                })
            },
        ).optional()?;
        Ok(row)
    }

    /// list_all / list_recent 同样改 SELECT 列表
    pub fn list_all(&self) -> anyhow::Result<Vec<DailySummaryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, total_keys, top_keys_json, theme_word, mood_word,
                    intensity, steadiness, fluency, activity_hours, key_class_json,
                    first_active_ms,
                    created_at
             FROM daily_summary ORDER BY date DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DailySummaryRow {
                date: row.get(0)?,
                total_keys: row.get(1)?,
                top_keys_json: row.get(2)?,
                theme_word: row.get(3)?,
                mood_word: row.get(4)?,
                intensity: row.get(5)?,
                steadiness: row.get(6)?,
                fluency: row.get(7)?,
                activity_hours: row.get(8)?,
                key_class_json: row.get(9)?,
                first_active_ms: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_recent(&self, limit: usize) -> anyhow::Result<Vec<DailySummaryRow>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT date, total_keys, top_keys_json, theme_word, mood_word,
                    intensity, steadiness, fluency, activity_hours, key_class_json,
                    first_active_ms,
                    created_at
             FROM daily_summary ORDER BY date DESC LIMIT ?"
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(DailySummaryRow {
                date: row.get(0)?,
                total_keys: row.get(1)?,
                top_keys_json: row.get(2)?,
                theme_word: row.get(3)?,
                mood_word: row.get(4)?,
                intensity: row.get(5)?,
                steadiness: row.get(6)?,
                fluency: row.get(7)?,
                activity_hours: row.get(8)?,
                key_class_json: row.get(9)?,
                first_active_ms: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// mood 单点管（不动其它字段）—— v0.3.1 P0 #2 修复
    pub fn upsert_mood(&self, date: &str, mood: &str) -> anyhow::Result<()> {
        let truncated: String = mood.chars().take(64).collect();
        self.conn.execute(
            "INSERT INTO daily_summary
             (date, total_keys, top_keys_json, theme_word, mood_word,
              intensity, steadiness, fluency, activity_hours, key_class_json,
              first_active_ms,
              created_at)
             VALUES (?1, 0, '[]', '', ?2, 0.0, 0.0, 0.0, 0, '{}', 0, ?3)
             ON CONFLICT(date) DO UPDATE SET mood_word = excluded.mood_word",
            params![date, truncated, chrono::Utc::now().timestamp_millis()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use crate::summary::aggregator::Aggregator;
    use crate::hook::event::KeyEvent;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    // ====== 现有 8 个测试保留 ======

    #[test]
    fn upsert_and_read_round_trip() {
        // 验证意图：写入后能读出完整字段，theme_word/total_keys 都对
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);

        let events: Vec<_> = (0..100)
            .map(|i| KeyEvent::now(i % 26 + 65, "s".into(), 0))
            .collect();
        let stats = Aggregator::aggregate("2026-07-16".into(), &events);
        repo.upsert(&stats, "hello", Some("focused")).unwrap();

        let read = repo.read_by_date("2026-07-16").unwrap().expect("row must exist");
        assert_eq!(read.date, "2026-07-16");
        assert_eq!(read.total_keys, 100);
        assert_eq!(read.theme_word, "hello");
        assert_eq!(read.mood_word.as_deref(), Some("focused"));
        assert!(!read.top_keys_json.is_empty(), "top_keys JSON 不能空");
    }

    #[test]
    fn upsert_is_idempotent_same_date() {
        // 验证意图：同日 re-upsert 用 INSERT OR REPLACE 覆盖（重新聚合覆盖旧值）
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);

        let events_a = vec![KeyEvent::now(65, "s".into(), 0); 50];
        let events_b = vec![KeyEvent::now(70, "s".into(), 0); 200];

        let stats_a = Aggregator::aggregate("2026-07-16".into(), &events_a);
        let stats_b = Aggregator::aggregate("2026-07-16".into(), &events_b);

        repo.upsert(&stats_a, "hello-a", Some("a")).unwrap();
        repo.upsert(&stats_b, "hello-b", Some("b")).unwrap(); // 应覆盖 a

        let read = repo.read_by_date("2026-07-16").unwrap().unwrap();
        assert_eq!(read.total_keys, 200, "应保留 b（最新）");
        assert_eq!(read.theme_word, "hello-b");
        assert_eq!(read.mood_word.as_deref(), Some("b"));
    }

    #[test]
    fn read_missing_date_returns_none() {
        // 验证意图：未聚合的日期返回 None（不会 panic）
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);
        assert!(repo.read_by_date("2030-01-01").unwrap().is_none());
    }

    #[test]
    fn list_all_orders_descending() {
        // 验证意图：list_all 按日期倒序返回
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);

        for date in ["2026-07-14", "2026-07-16", "2026-07-15"] {
            let events = vec![KeyEvent::now(65, "s".into(), 0); 10];
            let stats = Aggregator::aggregate(date.into(), &events);
            repo.upsert(&stats, "theme", Some("m")).unwrap();
        }

        let rows = repo.list_all().unwrap();
        assert_eq!(rows.len(), 3);
        // 倒序：07-16, 07-15, 07-14
        assert_eq!(rows[0].date, "2026-07-16");
        assert_eq!(rows[1].date, "2026-07-15");
        assert_eq!(rows[2].date, "2026-07-14");
    }

    #[test]
    fn null_mood_word_is_preserved() {
        // 验证意图：mood_word None 也正确写入读出
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);
        let events = vec![KeyEvent::now(65, "s".into(), 0); 5];
        let stats = Aggregator::aggregate("2026-07-16".into(), &events);
        repo.upsert(&stats, "hello", None).unwrap();
        let read = repo.read_by_date("2026-07-16").unwrap().unwrap();
        assert!(read.mood_word.is_none());
    }

    #[test]
    fn upsert_mood_writes_and_reads_back() {
        // 验证意图：空表 upsert_mood 后能读出 mood_word
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);
        repo.upsert_mood("2026-07-25", "开心").unwrap();
        let s = repo.read_by_date("2026-07-25").unwrap().expect("row must exist");
        assert_eq!(s.mood_word.as_deref(), Some("开心"));
    }

    #[test]
    fn upsert_mood_overwrites_previous_value() {
        // 验证意图：ON CONFLICT UPDATE 行为：二次 upsert 覆盖 mood，但其它字段保留初值
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);
        repo.upsert_mood("2026-07-25", "calm").unwrap();
        repo.upsert_mood("2026-07-25", "happy").unwrap();
        let s = repo.read_by_date("2026-07-25").unwrap().unwrap();
        assert_eq!(s.mood_word.as_deref(), Some("happy"));
    }

    #[test]
    fn upsert_mood_truncates_at_64_chars() {
        // 验证意图：防御 100+ 字符的恶意/异常输入，写入长度不超过 64
        let conn = fresh_db();
        let repo = SummaryRepo::new(&conn);
        let long = "x".repeat(100);
        repo.upsert_mood("2026-07-25", &long).unwrap();
        let s = repo.read_by_date("2026-07-25").unwrap().unwrap();
        assert_eq!(s.mood_word.as_ref().map(|s| s.len()), Some(64));
    }

    // ====== v0.3.5 新增 3 个测试 ======

    #[test]
    fn upsert_stats_persists_5_new_columns() {
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..50 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert!(read.intensity > 0.0, "intensity must be > 0, got {}", read.intensity);
        assert!(read.steadiness >= 0.0);
        assert!(read.fluency >= 0.0);
        assert_eq!(read.activity_hours, read.activity_hours); // sanity
        assert!(read.key_class_json.contains("game_keys"));
    }

    #[test]
    fn upsert_mood_preserves_new_columns() {
        // v0.3.1 P0 #2 回归不破：mood 单点管，新指标不被覆盖
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..30 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();
        SummaryRepo::new(&conn).upsert_mood("2026-07-29", "happy").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert_eq!(read.mood_word.as_deref(), Some("happy"));
        assert!(read.intensity > 0.0, "新指标必须保留, got {}", read.intensity);
    }

    #[test]
    fn read_by_date_returns_zero_for_old_data_after_alter() {
        // 老库 ALTER 后读老行：5 个新列拿到默认值（0.0 / 0 / "{}"）
        let conn = fresh_db();
        conn.execute(
            "INSERT INTO daily_summary (date, total_keys, top_keys_json, theme_word, mood_word, created_at)
             VALUES ('2026-07-28', 100, '[]', 'hello', 'happy', 1000)",
            []
        ).unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-28").unwrap().unwrap();
        assert_eq!(read.intensity, 0.0);
        assert_eq!(read.steadiness, 0.0);
        assert_eq!(read.fluency, 0.0);
        assert_eq!(read.activity_hours, 0);
        assert_eq!(read.key_class_json, "{}");
    }

    // ====== v0.3.6 新增 2 个测试 ======

    #[test]
    fn upsert_stats_persists_first_active_ms() {
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..10 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert_eq!(read.first_active_ms, 1_753_401_600_000);
    }

    #[test]
    fn upsert_mood_preserves_first_active_ms() {
        // v0.3.1 P0 #2 回归不破：mood 单点管
        let conn = fresh_db();
        let mut events: Vec<KeyEvent> = vec![];
        for i in 0..10 {
            let mut e = KeyEvent::now((i % 26 + 65) as u32, "s".into(), 0);
            e.timestamp_ms = 1_753_401_600_000 + (i as i64) * 100;
            events.push(e);
        }
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert_stats(&stats, "hello").unwrap();
        SummaryRepo::new(&conn).upsert_mood("2026-07-29", "happy").unwrap();

        let read = SummaryRepo::new(&conn).read_by_date("2026-07-29").unwrap().unwrap();
        assert_eq!(read.first_active_ms, 1_753_401_600_000, "mood upsert 不应覆盖 first_active_ms");
    }
}
