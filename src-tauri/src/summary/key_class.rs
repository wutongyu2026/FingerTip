//! v0.3.5: 键位分类汇总
//!
//! 验证意图：把按键按"游戏键 / 文本键 / 功能键"3 类汇总，
//! Today.vue 用来渲染水平条；History.vue 圆点显示比例。
//!
//! 优先级：game_keys (WASD + Space + Enter) > modifier_keys > text_keys
//! 其它键（F1-F12、方向键等）忽略。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct KeyClassSummary {
    pub game_keys: i64,
    pub text_keys: i64,
    pub modifier_keys: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::event::KeyEvent;

    #[test]
    fn classify_keys_game_only() {
        let events = vec![
            KeyEvent::now(87, "w".into(), 0), // W
            KeyEvent::now(65, "a".into(), 0), // A
            KeyEvent::now(83, "s".into(), 0), // S
            KeyEvent::now(68, "d".into(), 0), // D
            KeyEvent::now(32, "space".into(), 0),
            KeyEvent::now(13, "enter".into(), 0),
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 6, text_keys: 0, modifier_keys: 0 });
    }

    #[test]
    fn classify_keys_text_only() {
        let events = vec![
            KeyEvent::now(66, "b".into(), 0), // B
            KeyEvent::now(67, "c".into(), 0), // C
            KeyEvent::now(49, "1".into(), 0), // 1
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 0, text_keys: 3, modifier_keys: 0 });
    }

    #[test]
    fn classify_keys_modifier_only() {
        let events = vec![
            KeyEvent::now(8, "bs".into(), 0),
            KeyEvent::now(46, "del".into(), 0),
            KeyEvent::now(16, "shift".into(), 0),
            KeyEvent::now(17, "ctrl".into(), 0),
            KeyEvent::now(18, "alt".into(), 0),
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 0, text_keys: 0, modifier_keys: 5 });
    }

    #[test]
    fn classify_keys_wins_over_text() {
        // WASD 即使是 ASCII 字母也归 game_keys，不双计到 text_keys
        let events = vec![
            KeyEvent::now(87, "w".into(), 0),
            KeyEvent::now(65, "a".into(), 0),
            KeyEvent::now(83, "s".into(), 0),
            KeyEvent::now(68, "d".into(), 0),
            KeyEvent::now(66, "b".into(), 0), // B = 纯 text_key
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary { game_keys: 4, text_keys: 1, modifier_keys: 0 });
    }

    #[test]
    fn classify_keys_empty() {
        let events: Vec<KeyEvent> = vec![];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s.game_keys + s.text_keys + s.modifier_keys, 0);
    }

    #[test]
    fn classify_keys_ignores_unknown_codes() {
        // F1(112)、方向键(37-40) 等其它键不归入任何类别
        let events = vec![
            KeyEvent::now(112, "f1".into(), 0),
            KeyEvent::now(37, "left".into(), 0),
            KeyEvent::now(38, "up".into(), 0),
        ];
        let s = crate::summary::aggregator::Aggregator::classify_keys(&events);
        assert_eq!(s, KeyClassSummary::default());
    }
}
