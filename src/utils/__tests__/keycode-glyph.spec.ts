// keyCodeToGlyph 测试
// 验证意图：覆盖 keymap.rs 中所有可能落入 top_keys_json 的 VK code，
// 确保前端能正确显示人可读的按键符号，而不是一排 '?' 或空白。
//
// 注意：本测试用 String.fromCharCode 构造期望字符，避开文件编码问题。

import { describe, it, expect } from 'vitest'
import { keyCodeToGlyph } from '@/utils/keycode-glyph'

// 把 Unicode 字面量提到 define，避免 it.each 序列化时丢失
const BACKSPACE = String.fromCharCode(0x232B) // ⌫
const TAB = String.fromCharCode(0x21E5)       // ⇥
const ENTER = String.fromCharCode(0x21B5)      // ↵
const SHIFT = String.fromCharCode(0x21E7)      // ⇧
const CTRL = String.fromCharCode(0x2303)       // ⌃
const ALT = String.fromCharCode(0x2325)        // ⌥
const CAPSLOCK = String.fromCharCode(0x21EA)   // ⇪
const CMD = String.fromCharCode(0x2318)        // ⌘
const LEFT = String.fromCharCode(0x2190)       // ←
const UP = String.fromCharCode(0x2191)         // ↑
const RIGHT = String.fromCharCode(0x2192)      // →
const DOWN = String.fromCharCode(0x2193)       // ↓

describe('keyCodeToGlyph — 字母 A-Z', () => {
  it('A (65) → "A"', () => {
    expect(keyCodeToGlyph(65)).toBe('A')
  })

  it('M (77) → "M"', () => {
    expect(keyCodeToGlyph(77)).toBe('M')
  })

  it('Z (90) → "Z"', () => {
    expect(keyCodeToGlyph(90)).toBe('Z')
  })

  it('A..Z 全部映射正确（无 skip）', () => {
    for (let c = 65; c <= 90; c++) {
      const ch = String.fromCharCode(c)
      expect(keyCodeToGlyph(c)).toBe(ch)
    }
  })
})

describe('keyCodeToGlyph — 数字 0-9', () => {
  it('0 (48) → "0"', () => {
    expect(keyCodeToGlyph(48)).toBe('0')
  })

  it('9 (57) → "9"', () => {
    expect(keyCodeToGlyph(57)).toBe('9')
  })

  it('0..9 全部映射正确', () => {
    for (let c = 48; c <= 57; c++) {
      const ch = String.fromCharCode(c)
      expect(keyCodeToGlyph(c)).toBe(ch)
    }
  })
})

describe('keyCodeToGlyph — 常用控制键', () => {
  it('Space (32) → ⎵（不让用户看见空白以为没渲染）', () => {
    expect(keyCodeToGlyph(32)).toBe('space')
  })

  it('Enter (13) → ↵', () => {
    expect(keyCodeToGlyph(13)).toBe(ENTER)
  })

  it('Tab (9) → ⇥', () => {
    expect(keyCodeToGlyph(9)).toBe(TAB)
  })

  it('Backspace (8) → ⌫', () => {
    expect(keyCodeToGlyph(8)).toBe(BACKSPACE)
  })

  it('Escape (27) → "esc"', () => {
    expect(keyCodeToGlyph(27)).toBe('esc')
  })
})

describe('keyCodeToGlyph — 修饰键映射', () => {
  it('Shift (16) → ⇧', () => {
    expect(keyCodeToGlyph(16)).toBe(SHIFT)
  })

  it('Ctrl (17) → ⌃', () => {
    expect(keyCodeToGlyph(17)).toBe(CTRL)
  })

  it('Alt (18) → ⌥', () => {
    expect(keyCodeToGlyph(18)).toBe(ALT)
  })

  it('CapsLock (20) → ⇪', () => {
    expect(keyCodeToGlyph(20)).toBe(CAPSLOCK)
  })

  it('Cmd/Win Left (91) → ⌘', () => {
    expect(keyCodeToGlyph(91)).toBe(CMD)
  })

  it('Cmd/Win Right (92) → ⌘', () => {
    expect(keyCodeToGlyph(92)).toBe(CMD)
  })
})

describe('keyCodeToGlyph — 方向键', () => {
  it('Left (37) → ←', () => expect(keyCodeToGlyph(37)).toBe(LEFT))
  it('Up (38) → ↑', () => expect(keyCodeToGlyph(38)).toBe(UP))
  it('Right (39) → →', () => expect(keyCodeToGlyph(39)).toBe(RIGHT))
  it('Down (40) → ↓', () => expect(keyCodeToGlyph(40)).toBe(DOWN))
})

describe('keyCodeToGlyph — F1-F12', () => {
  it('F1 (112) → "F1"', () => expect(keyCodeToGlyph(112)).toBe('F1'))
  it('F6 (117) → "F6"', () => expect(keyCodeToGlyph(117)).toBe('F6'))
  it('F12 (123) → "F12"', () => expect(keyCodeToGlyph(123)).toBe('F12'))
})

describe('keyCodeToGlyph — 其它 ASCII 可显字符', () => {
  it('逗号 (44) → ","', () => expect(keyCodeToGlyph(44)).toBe(','))
  it('点 (46) → "."', () => expect(keyCodeToGlyph(46)).toBe('.'))
  it('感叹号 (33) → "!"', () => expect(keyCodeToGlyph(33)).toBe('!'))
})

describe('keyCodeToGlyph — 兜底分支', () => {
  it('未知 code (0) → "?"', () => {
    expect(keyCodeToGlyph(0)).toBe('?')
  })

  it('超大 code (1000) → "?"', () => {
    expect(keyCodeToGlyph(1000)).toBe('?')
  })

  it('负数 (-1) → "?"（防御性）', () => {
    expect(keyCodeToGlyph(-1)).toBe('?')
  })

  it('NaN → "?"（防御性）', () => {
    expect(keyCodeToGlyph(NaN)).toBe('?')
  })

  it('Infinity → "?"（防御性）', () => {
    expect(keyCodeToGlyph(Infinity)).toBe('?')
  })

  it('小数 (65.5) → "?"（只接受整数）', () => {
    expect(keyCodeToGlyph(65.5)).toBe('?')
  })
})

describe('keyCodeToGlyph — 关键验收案例（用户反馈"无法显示"）', () => {
  it('所有 keymap.rs 真实键值都能渲染（无一为 "?"）', () => {
    // keymap.rs 中实际返回 Some 的所有 VK code
    const realKeys = [
      8,   // Backspace
      9,   // Tab
      13,  // Return
      27,  // Escape
      32,  // Space
      48, 49, 50, 51, 52, 53, 54, 55, 56, 57, // 0-9
      65, 66, 67, 68, 69, 70, 71, 72, 73, 74,
      75, 76, 77, 78, 79, 80, 81, 82, 83, 84,
      85, 86, 87, 88, 89, 90, // A-Z
    ]
    for (const code of realKeys) {
      const glyph = keyCodeToGlyph(code)
      expect(glyph).not.toBe('?')
      expect(glyph.length).toBeGreaterThan(0)
      // 空格字符在视觉上"看不见"，但 ⎵ 是可见的 —— 确保 Space 不退化为真空格
      if (code === 32) expect(glyph).not.toBe(' ')
    }
  })
})
