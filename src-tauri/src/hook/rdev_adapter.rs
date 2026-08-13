//! rdev::Event → KeyEvent 适配器（纯函数，可测试）
//!
//! 验证意图：把 rdev 的全平台枚举事件翻译成领域层 KeyEvent。
//! 拆出来便于 TDD：避开 rdev::listen 的 block 调用，直接测"事件 → KeyEvent"映射。

use crate::hook::event::KeyEvent;
use std::time::SystemTime;

/// 把 rdev 事件翻译为 KeyEvent。
///
/// - KeyPress 且 key 可映射 → Some(KeyEvent)
/// - KeyRelease → None（不重复统计）
/// - Mouse / Wheel / 修饰键等 → None
pub fn rdev_event_to_key_event(event: &rdev::Event, session_id: &str) -> Option<KeyEvent> {
    match event.event_type {
        rdev::EventType::KeyPress(key) => {
            crate::keymap::vk_code(key).map(|code| KeyEvent {
                key_code: code,
                timestamp_ms: event
                    .time
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
                session_id: session_id.to_string(),
                modifiers: 0, // TODO: Phase 1.3 增强（Shift/Ctrl 元数据）
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_event(key: rdev::Key) -> rdev::Event {
        rdev::Event {
            time: SystemTime::now(),
            name: None,
            event_type: rdev::EventType::KeyPress(key),
        }
    }

    fn make_release(key: rdev::Key) -> rdev::Event {
        rdev::Event {
            time: SystemTime::now(),
            name: None,
            event_type: rdev::EventType::KeyRelease(key),
        }
    }

    #[test]
    fn letter_a_keypress_translates_to_keycode_65() {
        // 验证意图：字母键正确映射，session_id 透传
        let ev = make_event(rdev::Key::KeyA);
        let ke = rdev_event_to_key_event(&ev, "session-1").expect("KeyA should map");
        assert_eq!(ke.key_code, 65);
        assert_eq!(ke.session_id, "session-1");
        assert!(ke.timestamp_ms > 0, "timestamp must be unix epoch ms");
    }

    #[test]
    fn number_5_keypress_translates_to_53() {
        // 验证意图：数字键
        let ev = make_event(rdev::Key::Num5);
        let ke = rdev_event_to_key_event(&ev, "s").unwrap();
        assert_eq!(ke.key_code, 53);
    }

    #[test]
    fn backspace_translates_to_8() {
        // 验证意图：Backspace 计入事件（用于"删除/修改"统计）
        let ev = make_event(rdev::Key::Backspace);
        let ke = rdev_event_to_key_event(&ev, "s").unwrap();
        assert_eq!(ke.key_code, 8);
    }

    #[test]
    fn key_release_returns_none() {
        // 验证意图：KeyRelease 不重复统计（按一次计数）
        let ev = make_release(rdev::Key::KeyA);
        assert!(rdev_event_to_key_event(&ev, "s").is_none());
    }

    #[test]
    fn unsupported_key_returns_none() {
        // 验证意图：未映射的键（F1 等）返回 None（不入聚合）
        let ev = make_event(rdev::Key::F1);
        assert!(rdev_event_to_key_event(&ev, "s").is_none());
    }

    #[test]
    fn modifier_shift_returns_none() {
        // 验证意图：修饰键不入聚合
        let ev = make_event(rdev::Key::ShiftLeft);
        assert!(rdev_event_to_key_event(&ev, "s").is_none());
    }

    #[test]
    fn unused_args_zero_timestamp_fallback() {
        // 验证意图：时间戳为 epoch 时也返回 0（不 panic）
        let mut ev = make_event(rdev::Key::KeyA);
        ev.time = UNIX_EPOCH; // 1970-01-01
        let ke = rdev_event_to_key_event(&ev, "s").unwrap();
        assert_eq!(ke.timestamp_ms, 0);
    }
}
