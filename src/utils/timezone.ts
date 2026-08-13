// 时区 utility：基于 UTC 的偏移分钟数（与 chrono 一致）
//
// 用户原话："保留现有时间参数作为基准，用户选择其他时区时基于 UTC+0 进行相对加减计算。"
// 所以我们用「相对 UTC 的偏移分钟数」模型：
//   UTC+0 (伦敦/冬令时)         = 0
//   UTC+8 (北京/新加坡/香港)     = +480
//   UTC-5 (纽约/冬令时)         = -300
//
// 不用 IANA 时区数据库，不引入 chrono-tz 依赖，简单可靠。

/**
 * 把 epoch ms 转换到「该时区下的本地日期 / 时间」
 * @param epochMs 1970-01-01 UTC 起的毫秒数
 * @param offsetMinutes 该时区相对 UTC 的分钟偏移（正数=东）
 * @returns Date（其 .getHours()/getMinutes() 反映「该时区下」的本地时间）
 */
export function epochToLocal(epochMs: number, offsetMinutes: number): Date {
  return new Date(epochMs + offsetMinutes * 60_000)
}

/**
 * 取「该时区下的今天 date_str」Y-M-D
 * 替代 dayjs().format('YYYY-MM-DD') 的轻量实现
 */
export function todayStrInTz(offsetMinutes: number, now: Date = new Date()): string {
  const local = epochToLocal(now.getTime(), offsetMinutes)
  const y = local.getUTCFullYear()
  const m = String(local.getUTCMonth() + 1).padStart(2, '0')
  const d = String(local.getUTCDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

/**
 * 把 2026-07-23 字符串 + offset 转成 "2026 年 7 月 23 日" 显示用
 */
export function formatDateCN(dateStr: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateStr)
  if (!m) return dateStr
  return `${m[1]} 年 ${parseInt(m[2], 10)} 月 ${parseInt(m[3], 10)} 日`
}

/**
 * 浏览器自动检测用户所在时区 offset
 * - 用 -Date.now() 与 getTimezoneOffset 算出本地相对 UTC 的分钟数
 * - getTimezoneOffset 返回"本地时间 - UTC"的分钟数（注意符号！）
 *   北京 UTC+8 → getTimezoneOffset() = -480
 *   纽约 UTC-5 → getTimezoneOffset() = +300 (夏令时)
 *   所以 offsetMinutes = -getTimezoneOffset()
 */
export function detectLocalOffsetMinutes(): number {
  return -new Date().getTimezoneOffset()
}

/**
 * 时区选项（UI 下拉用） —— 覆盖全球主要时区
 * 间隔 1 小时，从 UTC-12 到 UTC+14
 */
export interface TimezoneOption {
  label: string      // 显示文本
  value: number      // offset 分钟数
}

export function buildTimezoneOptions(): TimezoneOption[] {
  const opts: TimezoneOption[] = []
  for (let h = -12; h <= 14; h++) {
    const sign = h >= 0 ? '+' : '-'
    const absH = Math.abs(h)
    const label = `UTC${sign}${String(absH).padStart(2, '0')}:00`
    opts.push({ label, value: h * 60 })
  }
  return opts
}

/**
 * offset 分钟数 → 简短字符串 "UTC+08:00"
 */
export function formatOffset(minutes: number): string {
  const sign = minutes >= 0 ? '+' : '-'
  const abs = Math.abs(minutes)
  const h = Math.floor(abs / 60)
  const m = abs % 60
  return `UTC${sign}${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`
}

export const TIMEZONE_STORAGE_KEY = 'fingertip_timezone_offset'

/** 从 localStorage 读 offset；非法值 fallback 0 */
export function loadStoredOffset(): number {
  try {
    const raw = localStorage.getItem(TIMEZONE_STORAGE_KEY)
    if (raw == null) return 0
    const n = parseInt(raw, 10)
    if (!Number.isFinite(n)) return 0
    // 范围限定 -12h ~ +14h
    const clamped = Math.max(-14 * 60, Math.min(14 * 60, n))
    return clamped
  } catch {
    return 0
  }
}

/** 存 offset 到 localStorage */
export function saveStoredOffset(minutes: number): void {
  try {
    localStorage.setItem(TIMEZONE_STORAGE_KEY, String(minutes))
  } catch {
    // 隐私模式 / 无 storage —— 静默
  }
}
