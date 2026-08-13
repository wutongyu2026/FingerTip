// 音乐播放 composable（Tone.Player 读本地 WAV）
//
// 验证意图：v0.3.4 起读后端生成的本地 WAV 文件（asset protocol），
//   替代 v0.3.0-v0.3.3 的实时 Tone.Synth 合成（避免大 note 数卡顿 + 音质更稳定）。
//   保留 exportWav() 给 web 模式 / 老数据（v0.3.2 旧 artifacts 无 wav_path）作为 fallback。

import { ref, type Ref } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'

export interface TonePlayback {
  load: (wavPath: string) => Promise<void>
  play: () => Promise<void>
  stop: () => Promise<void>
  isPlaying: Ref<boolean>
  // 真实进度（来自 Tone.Player transport state）
  currentMs: Ref<number>
  durationMs: Ref<number>
  // 内部方法（测试用）
  _scheduledCount: () => number
  // 离线渲染（web fallback 用；R5 deprecated，未来删）
  exportWav: () => Promise<Blob>
}

let _toneCache: typeof import('tone') | null = null
async function getTone(): Promise<typeof import('tone')> {
  if (!_toneCache) {
    _toneCache = await import('tone')
  }
  return _toneCache
}

export function useTonePlayback(): TonePlayback {
  const isPlaying = ref(false)
  const currentMs = ref(0)
  const durationMs = ref(0)
  let player: any = null
  let progressTimer: ReturnType<typeof setInterval> | null = null

  async function load(wavPath: string): Promise<void> {
    const Tone = await getTone()
    const ToneAny = Tone as any

    if (player) {
      try { player.dispose() } catch {}
      player = null
    }
    if (progressTimer !== null) {
      clearInterval(progressTimer)
      progressTimer = null
    }

    const url = convertFileSrc(wavPath)
    player = new ToneAny.Player({ url }).toDestination()
    await ToneAny.loaded()

    durationMs.value = (player.buffer.duration ?? 0) * 1000
  }

  async function play(): Promise<void> {
    if (!player) return
    // v0.3.7 R5 P1 修复：Tone.Player 没有 `transport` 属性（那是 Tone.Transport 全局对象的），
    // 之前 `player.transport?.seconds ?? 0` 永远 0 → 进度条卡 0:00。
    // 改为 play() 时记录 startedAt = Tone.now()，timer 内基于全局音频时钟算真实进度。
    const Tone = await getTone()
    // v0.3.9-fix: 浏览器 / Tauri webview 的 AudioContext 默认 suspended（autoplay 策略），
    // 必须在用户手势里 Tone.start() 恢复，否则 Tone.now() 永远不推进 → 进度卡 0 + 无声（桌面实测）。
    await (Tone as any).start()
    const startedAt = (Tone as any).now()
    player.start()
    isPlaying.value = true
    progressTimer = setInterval(() => {
      if (!player || !isPlaying.value) return
      const elapsedMs = ((Tone as any).now() - startedAt) * 1000
      currentMs.value = elapsedMs
      if (durationMs.value > 0 && currentMs.value >= durationMs.value) {
        currentMs.value = durationMs.value
        isPlaying.value = false
        if (progressTimer !== null) {
          clearInterval(progressTimer)
          progressTimer = null
        }
      }
    }, 100)
  }

  async function stop(): Promise<void> {
    if (!player) return
    player.stop()
    isPlaying.value = false
    currentMs.value = 0
    if (progressTimer !== null) {
      clearInterval(progressTimer)
      progressTimer = null
    }
  }

  return {
    load,
    play,
    stop,
    isPlaying,
    currentMs,
    durationMs,
    _scheduledCount: () => (player ? 1 : 0),
    exportWav: async (): Promise<Blob> => {
      throw new Error('exportWav deprecated: use backend music.wav file via load(path)')
    },
  }
}
