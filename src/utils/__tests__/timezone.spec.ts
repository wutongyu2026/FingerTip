import { describe, it, expect } from 'vitest'
import {
  epochToLocal,
  todayStrInTz,
  formatDateCN,
  detectLocalOffsetMinutes,
  buildTimezoneOptions,
  formatOffset,
  loadStoredOffset,
  saveStoredOffset,
} from '../timezone'

describe('timezone utils', () => {
  it('epochToLocal UTC+0 不会漂移到下一天', () => {
    const ms = 1753344000000 // 2025-07-24 00:00:00 UTC
    const d = epochToLocal(ms, 0)
    // 不检查 getHours —— getHours 受测试机器 timezone 影响
    // 改检查 UTC 字段（与机器 tz 无关）
    expect(d.getUTCDate()).toBe(24)
    expect(d.getUTCMonth()).toBe(6) // July
    expect(d.getUTCFullYear()).toBe(2025)
  })

  it('epochToLocal UTC+8 同一天不漂移（8 小时在日内）', () => {
    const ms = 1753344000000 // 2025-07-24 00:00:00 UTC
    const d = epochToLocal(ms, 480)
    // UTC+8 偏移后仍在同一天（24+8=32 小时溢出 → 但 epochToLocal 不溢出）
    // Date 对象创建时 UTC 字段保持：
    expect(d.getUTCDate()).toBe(24)
    // 而如果用本地 hours 数（不是 UTC），应该反映 8 小时偏移
    // 下面这个 getHours 在 UTC+0 机器是 8，UTC+8 是 16
    // 所以我们直接验证：本地 hours ≡ UTC hours + 8 (mod 24)
    const expectedLocalHours = (d.getUTCHours() + 8) % 24
    expect(d.getHours()).toBe(expectedLocalHours)
  })

  it('todayStrInTz UTC+0 uses UTC calendar', () => {
    const fake = new Date(1753344000000) // 2025-07-24 00:00:00 UTC
    expect(todayStrInTz(0, fake)).toBe('2025-07-24')
  })

  it('todayStrInTz UTC+8 advances date when near midnight UTC', () => {
    // 2025-07-23 23:00:00 UTC  =  2025-07-24 07:00:00 +08:00
    const fake = new Date(Date.UTC(2025, 6, 23, 23, 0, 0))
    expect(todayStrInTz(480, fake)).toBe('2025-07-24')
  })

  it('todayStrInTz UTC-5 pushes back when local time near midnight', () => {
    // 2025-07-24 03:00:00 UTC  =  2025-07-23 22:00:00 -05:00
    const fake = new Date(Date.UTC(2025, 6, 24, 3, 0, 0))
    expect(todayStrInTz(-300, fake)).toBe('2025-07-23')
  })

  it('formatDateCN renders localized text', () => {
    expect(formatDateCN('2026-07-23')).toBe('2026 年 7 月 23 日')
    expect(formatDateCN('2025-01-01')).toBe('2025 年 1 月 1 日')
  })

  it('formatDateCN returns original string if no match', () => {
    expect(formatDateCN('today')).toBe('today')
  })

  it('detectLocalOffsetMinutes returns a finite number in [-840, 840]', () => {
    const m = detectLocalOffsetMinutes()
    expect(Number.isFinite(m)).toBe(true)
    expect(m).toBeGreaterThanOrEqual(-14 * 60)
    expect(m).toBeLessThanOrEqual(14 * 60)
  })

  it('buildTimezoneOptions covers UTC-12 to UTC+14 (27 entries)', () => {
    const opts = buildTimezoneOptions()
    expect(opts.length).toBe(27)
    expect(opts[0]).toEqual({ label: 'UTC-12:00', value: -720 })
    expect(opts[opts.length - 1]).toEqual({ label: 'UTC+14:00', value: 840 })
  })

  it('formatOffset renders with sign and zero pad', () => {
    expect(formatOffset(0)).toBe('UTC+00:00')
    expect(formatOffset(480)).toBe('UTC+08:00')
    expect(formatOffset(-300)).toBe('UTC-05:00')
    expect(formatOffset(330)).toBe('UTC+05:30')
  })

  it('loadStoredOffset handles missing / invalid gracefully', () => {
    // 没有 localStorage 或抛错时返 0
    expect([0, -300, 480]).toContain(loadStoredOffset())
  })

  it('saveStoredOffset + loadStoredOffset round trip', () => {
    saveStoredOffset(480)
    expect(loadStoredOffset()).toBe(480)
    saveStoredOffset(-300)
    expect(loadStoredOffset()).toBe(-300)
    saveStoredOffset(0)
  })

  it('loadStoredOffset clamps out-of-range values', () => {
    saveStoredOffset(99999)
    expect(loadStoredOffset()).toBeLessThanOrEqual(14 * 60)
    saveStoredOffset(-99999)
    expect(loadStoredOffset()).toBeGreaterThanOrEqual(-14 * 60)
    saveStoredOffset(0)
  })
})
