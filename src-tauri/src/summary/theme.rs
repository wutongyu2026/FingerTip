//! 主题词提取：行为特征驱动的有语义主题词判定。
//!
//! 验证意图：把"今日键盘指纹"浓缩成一个有语义的主题词，
//! 作为 AI 音乐/画作生成的创意种子。
//!
//! ## v0.4.2: 行为特征驱动主题词（同学项目移植）
//!
//! 五级优先级规则：
//!   1. 退格+删除 ≥ 12%  → REWRITE
//!   2. Enter > 8%       → BREAK
//!   3. 空格 > 30%       → PAUSE
//!   4. WASD > 25%       → CONTROL
//!   5. 四维组合表（密集度/平稳度/流畅度/活跃度）→ 16 种英文主题
//!
//! 旧实现（v0.2.4）：返回最高频的单个字母（如 "E"），毫无语义——保留为
//! `extract_theme_word()`（兼容现有测试）。

use std::collections::HashMap;

use crate::hook::event::KeyEvent;

/// 保留旧函数签名（兼容现有测试），不再使用。
/// v0.4.2: 改用 `determine_theme_from_behavior` 替代。
pub fn extract_theme_word(counts: &HashMap<u32, usize>) -> String {
    if counts.is_empty() {
        return String::new();
    }
    let mut best_key: Option<u32> = None;
    let mut best_count: usize = 0;
    for (&k, &v) in counts {
        if !is_printable_ascii(k) {
            continue;
        }
        if best_key.is_none() || v > best_count || (v == best_count && k < best_key.unwrap()) {
            best_key = Some(k);
            best_count = v;
        }
    }
    match best_key {
        Some(k) => (k as u8 as char).to_string(),
        None => String::new(),
    }
}

/// key_code 是否属于可打印 ASCII（32-126）
fn is_printable_ascii(code: u32) -> bool {
    (32..=126).contains(&code)
}

/// v0.8: 根据按键行为特征 + 四个指标，判定有语义的主题词。
///
/// 五级优先级：
///   1. 退格+删除 ≥ 12%  → REWRITE
///   2. Enter > 8%       → BREAK
///   3. 空格 > 30%       → PAUSE
///   4. WASD > 25%       → CONTROL
///   5. 四维组合表（密集度/平稳度/流畅度/活跃度）→ 16 种英文主题
///
/// 用于 generate_now：用户指定时间窗口时，从窗口 events 现场重算 theme_word
/// （不沿用全天 summary 的 theme——窗口可能聚焦于不同行为模式）。
pub fn determine_theme_from_behavior(
    events: &[KeyEvent],
    intensity: f64,
    steadiness: f64,
    fluency: f64,
    activity_hours: i32,
) -> String {
    let total = events.len().max(1) as f64;

    let backspace = events.iter().filter(|e| e.key_code == 8).count() as f64;
    let delete = events.iter().filter(|e| e.key_code == 46).count() as f64;
    let enter = events.iter().filter(|e| e.key_code == 13).count() as f64;
    let space = events.iter().filter(|e| e.key_code == 32).count() as f64;
    let wasd = events.iter().filter(|e| [65, 83, 68, 87].contains(&e.key_code)).count() as f64;

    if (backspace + delete) / total >= 0.12 {
        return "REWRITE".into();
    }
    if enter / total > 0.08 {
        return "BREAK".into();
    }
    if space / total > 0.30 {
        return "PAUSE".into();
    }
    if wasd / total > 0.25 {
        return "CONTROL".into();
    }

    let density = if intensity > 2000.0 { "快" } else { "慢" };
    let smooth = if steadiness <= 0.8 { "平稳" } else { "跳跃" };
    let flow = if fluency < 0.10 { "流畅" } else { "停顿" };
    let active = if activity_hours > 4 { "活跃" } else { "不活跃" };

    match (density, smooth, flow, active) {
        ("快", "平稳", "流畅", "活跃") => "Flow",
        ("快", "平稳", "流畅", "不活跃") => "Sprint",
        ("快", "平稳", "停顿", "活跃") => "Forge",
        ("快", "平稳", "停顿", "不活跃") => "Flash",
        ("快", "跳跃", "流畅", "活跃") => "Rush",
        ("快", "跳跃", "流畅", "不活跃") => "Burst",
        ("快", "跳跃", "停顿", "活跃") => "Turbulence",
        ("快", "跳跃", "停顿", "不活跃") => "Shards",
        ("慢", "平稳", "流畅", "活跃") => "Tide",
        ("慢", "平稳", "流畅", "不活跃") => "Whisper",
        ("慢", "平稳", "停顿", "活跃") => "Trek",
        ("慢", "平稳", "停顿", "不活跃") => "Pause",
        ("慢", "跳跃", "流畅", "活跃") => "Drift",
        ("慢", "跳跃", "流畅", "不活跃") => "Wander",
        ("慢", "跳跃", "停顿", "活跃") => "Spiral",
        ("慢", "跳跃", "停顿", "不活跃") => "Stillness",
        _ => "Unknown",
    }
    .into()
}

/// v0.8: 根据四维行为特征推断默认心情（mood 留空时用）。
///
/// 16 种四维组合 → 英文心情词，与主题词判定共用同一套分类维度。
pub fn infer_mood_from_behavior(
    intensity: f64,
    steadiness: f64,
    fluency: f64,
    activity_hours: i32,
) -> &'static str {
    let density = if intensity > 2000.0 { "快" } else { "慢" };
    let smooth = if steadiness <= 0.8 { "平稳" } else { "跳跃" };
    let flow = if fluency < 0.10 { "流畅" } else { "停顿" };
    let active = if activity_hours > 4 { "活跃" } else { "不活跃" };

    match (density, smooth, flow, active) {
        ("快", "平稳", "流畅", "活跃") => "energetic",
        ("快", "平稳", "流畅", "不活跃") => "focused",
        ("快", "平稳", "停顿", "活跃") => "determined",
        ("快", "平稳", "停顿", "不活跃") => "hurried",
        ("快", "跳跃", "流畅", "活跃") => "excited",
        ("快", "跳跃", "流畅", "不活跃") => "restless",
        ("快", "跳跃", "停顿", "活跃") => "chaotic",
        ("快", "跳跃", "停顿", "不活跃") => "frustrated",
        ("慢", "平稳", "流畅", "活跃") => "calm",
        ("慢", "平稳", "流畅", "不活跃") => "peaceful",
        ("慢", "平稳", "停顿", "活跃") => "thoughtful",
        ("慢", "平稳", "停顿", "不活跃") => "tired",
        ("慢", "跳跃", "流畅", "活跃") => "playful",
        ("慢", "跳跃", "流畅", "不活跃") => "distracted",
        ("慢", "跳跃", "停顿", "活跃") => "anxious",
        ("慢", "跳跃", "停顿", "不活跃") => "melancholy",
        _ => "calm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_key_with_count_returns_readable_summary() {
        // 验证意图：v0.2.4 改进——返回最高频字母（去数字）
        let mut counts = HashMap::new();
        counts.insert(b'h' as u32, 10);
        counts.insert(b'e' as u32, 8);
        counts.insert(b'l' as u32, 5);
        counts.insert(b'o' as u32, 5);
        let word = extract_theme_word(&counts);
        // 最高频 h (10) → "h"
        assert_eq!(word, "h");
    }

    #[test]
    fn filters_non_printable_keys() {
        // 验证意图：非 ASCII 键（功能键、修饰键）被过滤，不污染主题词
        let mut counts = HashMap::new();
        counts.insert(16u32, 100); // Shift - 不应出现
        counts.insert(b'a' as u32, 5);
        counts.insert(b'b' as u32, 3);
        let word = extract_theme_word(&counts);
        assert_eq!(word, "a"); // 最高频 a
        assert!(!word.contains(char::from(16)));
    }

    #[test]
    fn empty_counts_returns_empty_string() {
        let counts: HashMap<u32, usize> = HashMap::new();
        let word = extract_theme_word(&counts);
        assert_eq!(word, "");
    }

    #[test]
    fn only_non_printable_returns_empty() {
        // 验证意图：全是非 ASCII 键（如纯修饰键）应返回空，不显示错乱字符
        let mut counts = HashMap::new();
        counts.insert(16u32, 100); // Shift
        counts.insert(17u32, 50); // Ctrl
        let word = extract_theme_word(&counts);
        assert_eq!(word, "");
    }

    #[test]
    fn tie_breaks_by_smaller_key_code() {
        // 验证意图：同 count 时按 key_code 升序（确定性，避免 HashMap 随机顺序）
        let mut counts = HashMap::new();
        counts.insert(b'z' as u32, 5);
        counts.insert(b'a' as u32, 5);
        let word = extract_theme_word(&counts);
        // a (97) < z (122) 同 count 5 → 选 a
        assert_eq!(word, "a");
    }

    /// 用户原报告："今日主题词一直是 I,N,A 三个字母排列"
    /// 验证意图：v0.2.4 修复后——INA 高频不再输出 "INA" 这种无意义串
    #[test]
    fn high_freq_ina_no_longer_returns_ina_concat() {
        let mut counts = HashMap::new();
        counts.insert(b'I' as u32, 120);
        counts.insert(b'N' as u32, 98);
        counts.insert(b'A' as u32, 85);
        let word = extract_theme_word(&counts);
        // 旧实现: "INA" → 新实现: "I"
        assert_ne!(word, "INA", "v0.2.4 必须不再输出字母拼接");
        assert_eq!(word, "I");
    }
}