//! rdev::Key → Windows Virtual Key Code (u32) 映射
//!
//! 验证意图：把 rdev enum 映射到 VK code，使 SQLite 中存储的 key_code 与
//! Aggregator 期望的整数一致（之前 task 已基于 u32 设计）。
//!
//! 设计：覆盖常用键（字母、数字、Backspace/Space/Enter 等），修饰键/功能键
//! 暂时返回 None（首版不聚合修饰键）

/// 把 rdev::Key 映射为虚拟键码。未覆盖返回 None（不计入事件）。
pub fn vk_code(key: rdev::Key) -> Option<u32> {
    match key {
        // —— 字母 a-z (65-90) ——
        rdev::Key::KeyA => Some(65),
        rdev::Key::KeyB => Some(66),
        rdev::Key::KeyC => Some(67),
        rdev::Key::KeyD => Some(68),
        rdev::Key::KeyE => Some(69),
        rdev::Key::KeyF => Some(70),
        rdev::Key::KeyG => Some(71),
        rdev::Key::KeyH => Some(72),
        rdev::Key::KeyI => Some(73),
        rdev::Key::KeyJ => Some(74),
        rdev::Key::KeyK => Some(75),
        rdev::Key::KeyL => Some(76),
        rdev::Key::KeyM => Some(77),
        rdev::Key::KeyN => Some(78),
        rdev::Key::KeyO => Some(79),
        rdev::Key::KeyP => Some(80),
        rdev::Key::KeyQ => Some(81),
        rdev::Key::KeyR => Some(82),
        rdev::Key::KeyS => Some(83),
        rdev::Key::KeyT => Some(84),
        rdev::Key::KeyU => Some(85),
        rdev::Key::KeyV => Some(86),
        rdev::Key::KeyW => Some(87),
        rdev::Key::KeyX => Some(88),
        rdev::Key::KeyY => Some(89),
        rdev::Key::KeyZ => Some(90),

        // —— 数字 0-9 (48-57) ——
        rdev::Key::Num0 => Some(48),
        rdev::Key::Num1 => Some(49),
        rdev::Key::Num2 => Some(50),
        rdev::Key::Num3 => Some(51),
        rdev::Key::Num4 => Some(52),
        rdev::Key::Num5 => Some(53),
        rdev::Key::Num6 => Some(54),
        rdev::Key::Num7 => Some(55),
        rdev::Key::Num8 => Some(56),
        rdev::Key::Num9 => Some(57),

        // —— 编辑/常用键 ——
        rdev::Key::Backspace => Some(8),
        rdev::Key::Tab => Some(9),
        rdev::Key::Return => Some(13),
        rdev::Key::Escape => Some(27),
        rdev::Key::Space => Some(32),

        // —— 未覆盖的键（包括所有修饰键、功能键等）——
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_a_maps_to_65() {
        // 验证意图：字母 a → ASCII 65（VK_A）
        assert_eq!(vk_code(rdev::Key::KeyA), Some(65));
    }

    #[test]
    fn letter_z_maps_to_90() {
        // 验证意图：字母 z → ASCII 90（VK_Z）
        assert_eq!(vk_code(rdev::Key::KeyZ), Some(90));
    }

    #[test]
    fn number_0_maps_to_48() {
        // 验证意图：数字 0 → ASCII 48（VK_0）
        assert_eq!(vk_code(rdev::Key::Num0), Some(48));
    }

    #[test]
    fn number_9_maps_to_57() {
        // 验证意图：数字 9 → ASCII 57
        assert_eq!(vk_code(rdev::Key::Num9), Some(57));
    }

    #[test]
    fn backspace_maps_to_8() {
        // 验证意图：Backspace → 8（用于"删除/修改"统计）
        assert_eq!(vk_code(rdev::Key::Backspace), Some(8));
    }

    #[test]
    fn space_maps_to_32() {
        // 验证意图：Space → 32（最常用键之一）
        assert_eq!(vk_code(rdev::Key::Space), Some(32));
    }

    #[test]
    fn enter_maps_to_13() {
        // 验证意图：Enter → 13
        assert_eq!(vk_code(rdev::Key::Return), Some(13));
    }

    #[test]
    fn modifier_keys_return_none() {
        // 验证意图：修饰键不入聚合（避免统计噪声）
        assert_eq!(vk_code(rdev::Key::ShiftLeft), None);
        assert_eq!(vk_code(rdev::Key::ShiftRight), None);
        assert_eq!(vk_code(rdev::Key::ControlLeft), None);
        assert_eq!(vk_code(rdev::Key::Alt), None);
        assert_eq!(vk_code(rdev::Key::AltGr), None);
        assert_eq!(vk_code(rdev::Key::MetaLeft), None);
    }

    #[test]
    fn function_keys_return_none() {
        // 验证意图：F1-F12 等暂不入聚合（首版聚焦可读字符）
        assert_eq!(vk_code(rdev::Key::F1), None);
        assert_eq!(vk_code(rdev::Key::F12), None);
    }
}
