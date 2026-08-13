pub mod artifact_repo;
pub mod artifact_writer;
pub mod event_repo;
pub mod migrations;
pub mod summary_repo;
pub mod wav_analysis;

use rusqlite::Connection;
use std::path::Path;

/// v0.2.1 性能 PRAGMA：解决「每键 INSERT 卡顿」三大根因
///
/// 1. `journal_mode = WAL`
///    - 写不阻塞读，多进程读友好
///    - 对小事务 INSERT 最优
/// 2. `synchronous = NORMAL`
///    - WAL 模式下 NORMAL = 仅在 checkpoint 时 fsync，不是每键
///    - 比 FULL（默认）快 5-50 倍，特别在移动硬盘
///    - 崩溃丢 1-2 秒数据 —— 用户已明确接受「近实时」取舍
/// 3. `temp_store = MEMORY`：临时表放内存
/// 4. `cache_size = -20000`（≈ 20MB）：缓存索引页，读写均有加速
/// 5. `mmap_size = 256MB`：热数据 mmap，读零拷贝
///
/// 注意：内存 DB 不接受 journal_mode=wal（SQLite 会忽略而非报错）；
/// 所以对 in_memory 走单独路径
const PRAGMA_PERF: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA temp_store = MEMORY;
    PRAGMA cache_size = -20000;
    PRAGMA mmap_size = 268435456;
";

/// 仅 内存 DB 适用的 PRAGMA（不含 WAL/mmap）
const PRAGMA_PERF_INMEM: &str = "
    PRAGMA synchronous = NORMAL;
    PRAGMA temp_store = MEMORY;
    PRAGMA cache_size = -20000;
";

/// 初始化内存 DB（首版 / 测试用）
pub fn init_in_memory() -> anyhow::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(PRAGMA_PERF_INMEM)?;
    migrations::run_migrations(&conn)?;
    Ok(conn)
}

/// 初始化持久 DB（生产用）
///
/// 文件路径由调用方提供（通常是 app_data_dir/fingertip.db）。
/// 重启后数据保留 —— 用户重启 FingerTip 不丢历史按键 + daily_summary。
pub fn init_at(path: &Path) -> anyhow::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(PRAGMA_PERF)?;
    migrations::run_migrations(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod schema_tests {
    /// Stage 2 Task 2.3 — daily_summary schema 契约验证
    ///
    /// `PRAGMA table_info(daily_summary)` 返回所有列元数据。
    /// 第 0 列是 cid（自增），第 1 列是 name，第 2 列是 type。
    /// 我们取第 1 列（name）验证 mood_word / date / theme_word 都存在。
    ///
    /// 这是 schema-level 契约保证：
    /// - 后端 SummaryRepo 写 mood_word 不依赖运行时错误
    /// - 前端 Stage 4 读 mood 字段有 DB schema 保证
    #[test]
    fn daily_summary_includes_mood_word_column() {
        let conn = crate::db::init_in_memory().unwrap();
        let mut stmt = conn.prepare("PRAGMA table_info(daily_summary)").unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        assert!(
            cols.iter().any(|c| c == "date"),
            "daily_summary must have date column, got: {:?}",
            cols
        );
        assert!(
            cols.iter().any(|c| c == "mood_word"),
            "daily_summary must have mood_word column, got: {:?}",
            cols
        );
        assert!(
            cols.iter().any(|c| c == "theme_word"),
            "daily_summary must have theme_word column, got: {:?}",
            cols
        );
    }
}