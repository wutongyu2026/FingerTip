// useTonePlayback mock 测试
// 验证意图：Tone.Player 读取后端生成的本地 WAV，并暴露播放状态与进度。

import { describe, it, expect, beforeEach, vi } from 'vitest'

const playerInstances: any[] = []
const loaded = vi.fn(async () => undefined)
// 注意：tone.now mock 用 vi.hoisted 保证 mock 工厂和测试都可访问同一个 fn。
// 真实 Tone.Player 没有 transport 属性 —— 之前 useTonePlayback 误用
// `player.transport?.seconds` 永远为 0，play 时进度条不更新（P1 bug）。
// 修复后进度基于 Tone.now() 计算。
const mockNow = vi.hoisted(() => vi.fn(() => 0))
const mockStart = vi.hoisted(() => vi.fn(async () => undefined))

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: vi.fn((path: string) => `asset://${path}`),
}))

vi.mock('tone', () => ({
  Player: vi.fn().mockImplementation(() => {
    const instance = {
      buffer: { duration: 1.25 },
      start: vi.fn(),
      stop: vi.fn(),
      dispose: vi.fn(),
      toDestination: vi.fn(),
    }
    instance.toDestination.mockReturnValue(instance)
    playerInstances.push(instance)
    return instance
  }),
  loaded,
  now: mockNow,
  start: mockStart,
}))

describe('useTonePlayback (Tone.Player WAV)', () => {
  beforeEach(() => {
    playerInstances.length = 0
    loaded.mockClear()
    mockNow.mockReset()
    mockNow.mockReturnValue(0)
    mockStart.mockClear()
  })

  it('currentMs_durations_default_to_zero', async () => {
    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()

    expect(player.currentMs.value).toBe(0)
    expect(player.durationMs.value).toBe(0)
  })

  it('load_initializes_player', async () => {
    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()

    await player.load('/fake/path/test.wav')

    expect(player._scheduledCount()).toBe(1)
    expect(player.durationMs.value).toBe(1250)
    expect(loaded).toHaveBeenCalledOnce()
  })

  it('play starts the loaded player', async () => {
    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()

    await player.load('/fake/path/test.wav')
    await player.play()

    expect(playerInstances[0].start).toHaveBeenCalledOnce()
    expect(player.isPlaying.value).toBe(true)
  })

  // v0.3.9-fix: 浏览器/Tauri webview 的 AudioContext 默认 suspended，
  // play() 必须先 Tone.start() 恢复，否则 Tone.now() 不推进、进度卡 0、无声（桌面实测 bug）。
  it('play 先调 Tone.start() 恢复 AudioContext', async () => {
    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()

    await player.load('/fake/path/test.wav')
    await player.play()

    expect(mockStart).toHaveBeenCalledOnce()
  })

  it('stop stops playback and resets progress', async () => {
    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()

    await player.load('/fake/path/test.wav')
    await player.play()
    player.currentMs.value = 500
    await player.stop()

    expect(playerInstances[0].stop).toHaveBeenCalledOnce()
    expect(player.isPlaying.value).toBe(false)
    expect(player.currentMs.value).toBe(0)
  })

  it('play and stop are no-ops before load', async () => {
    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()

    await player.play()
    await player.stop()

    expect(player.isPlaying.value).toBe(false)
    expect(player._scheduledCount()).toBe(0)
  })

  it('exportWav is deprecated', async () => {
    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()

    await expect(player.exportWav()).rejects.toThrow(/exportWav deprecated/)
  })

  // v0.3.7 R5 P1: 进度条基于 Tone.now() 计算，不依赖 player.transport（Player 没有该属性）。
  // 验证意图：play() 后 250ms → currentMs ~ 250ms，不再卡 0。
  it('play 后 currentMs 反映真实播放进度（基于 Tone.now）', async () => {
    vi.useFakeTimers()
    mockNow.mockReturnValue(0)

    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()
    await player.load('/fake/path/test.wav')

    // play() 此时 Tone.now()=0（startedAt=0）
    await player.play()

    // 模拟时间流逝 250ms
    mockNow.mockReturnValue(0.25)
    vi.advanceTimersByTime(100)

    expect(player.currentMs.value).toBeGreaterThan(0)
    expect(player.currentMs.value).toBeCloseTo(250, -1)

    vi.useRealTimers()
  })

  // v0.3.7 R5 P1: 进度条到 durationMs 时自动停。
  it('currentMs 到达 durationMs 时自动停止并钳到 durationMs', async () => {
    vi.useFakeTimers()
    mockNow.mockReturnValue(0)

    const { useTonePlayback } = await import('@/composables/useTonePlayback')
    const player = useTonePlayback()
    await player.load('/fake/path/test.wav')  // duration = 1.25s → 1250ms

    await player.play()

    // 模拟时间流逝 2s（超过 duration 1.25s）
    mockNow.mockReturnValue(2)
    vi.advanceTimersByTime(100)

    // currentMs 钳到 durationMs，isPlaying 自动 false
    expect(player.currentMs.value).toBe(1250)
    expect(player.isPlaying.value).toBe(false)

    vi.useRealTimers()
  })
})
