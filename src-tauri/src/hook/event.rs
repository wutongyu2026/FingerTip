use serde::{Deserialize, Serialize};

/// 单次按键事件的数据契约。
///
/// 验证意图：在 hook 监听 → buffer 缓冲 → SQLite 持久化三层之间流动的事件结构。
/// 字段保持不可变，避免跨层传递时被意外修改（编码风格规约：immutable）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key_code: u32,
    pub timestamp_ms: i64,
    pub session_id: String,
    pub modifiers: u8,
}

impl KeyEvent {
    pub fn now(key_code: u32, session_id: String, modifiers: u8) -> Self {
        Self {
            key_code,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            session_id,
            modifiers,
        }
    }
}