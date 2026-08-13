// keyCode (Windows Virtual Key Code, u32) → 用户可读的按键符号
//
// 验证意图：
// 后端 keymap.rs 用 rdev::Key → VK code。键值落在不同区间：
//   - 48-57：数字 0-9
//   - 65-90：字母 A-Z
//   - 8/9/13/27/32：Backspace/Tab/Enter/Esc/Space（首版聚合进入 top_keys_json）
//   - 其它不可显字符（修饰键、功能键）：本任务范围内不进入 top_keys_json，
//     但前端必须有兜底，不允许把任意 code 静默渲成 "?" —— 否则用户看到一排问号会以为"没显示"。
//
// 实现注意：Unicode 符号全部用 String.fromCharCode(0xXXXX) 构造，
// 避免源文件被不同编码（GBK/ANSI/UTF-8 BOM）误读导致的字符损坏问题。
// 这是因为 esbuild/tsc 在不同 charset 下解析源码时，源码字面量可能被破坏。

export function keyCodeToGlyph(code: number): string {
  // 防御性输入：负数 / NaN / Infinity 全部走兜底
  if (!Number.isFinite(code) || code < 0 || !Number.isInteger(code)) {
    return '?'
  }

  // —— 字母 A-Z (65-90) ——
  if (code >= 65 && code <= 90) {
    return String.fromCharCode(code)
  }

  // —— 数字 0-9 (48-57) ——
  if (code >= 48 && code <= 57) {
    return String.fromCharCode(code)
  }

  const mappedGlyphs: Readonly<Record<number, string>> = {
    8: String.fromCharCode(0x232B),
    9: String.fromCharCode(0x21E5),
    13: String.fromCharCode(0x21B5),
    16: String.fromCharCode(0x21E7),
    17: String.fromCharCode(0x2303),
    18: String.fromCharCode(0x2325),
    20: String.fromCharCode(0x21EA),
    27: 'esc',
    32: 'space',
    37: String.fromCharCode(0x2190),
    38: String.fromCharCode(0x2191),
    39: String.fromCharCode(0x2192),
    40: String.fromCharCode(0x2193),
    91: String.fromCharCode(0x2318),
    92: String.fromCharCode(0x2318),
    112: 'F1',
    113: 'F2',
    114: 'F3',
    115: 'F4',
    116: 'F5',
    117: 'F6',
    118: 'F7',
    119: 'F8',
    120: 'F9',
    121: 'F10',
    122: 'F11',
    123: 'F12',
  }
  const mappedGlyph = mappedGlyphs[code]
  if (mappedGlyph) return mappedGlyph

  if (code >= 32 && code <= 126) return String.fromCharCode(code)
  return '?'
}
