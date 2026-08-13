import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { loadStoredOffset, saveStoredOffset } from '@/utils/timezone'
import type { GenerateNowResult } from '@/types/artwork'

// v0.4: generate_now 的输出 contract 直接用 types/artwork.ts 共享类型
//   sentence 是字符串（不再 Sentence 对象）/ art_png_path 是文件路径 / 画作用 <img> 渲染
type GenerationResult = GenerateNowResult

// 简化的应用状态：当日总结 + 心情词 + 生成状态 + 时区偏移
// 验证意图：跨视图共享状态（Today / SubmitMood / Settings）
export const useAppStore = defineStore('app', () => {
  const todaySummary = ref<any>(null)
  const moodWord = ref('')
  const generating = ref(false)
  // v0.2.2 时区：相对 UTC 的分钟偏移（例如北京 = +480）
  const timezoneOffsetMinutes = ref<number>(loadStoredOffset())
  // 生成层参数（Today Recalculate / SubmitMood 生成后存，Artworks.vue 渲染用）
  const generationResult = ref<GenerationResult | null>(null)
  // v0.3 Stage 5: 下载目录（用户配置 → localStorage 持久化 → ensureDefaultDir 初始化）
  const downloadDir = ref<string>('')
  // v0.3.1: 默认偏好风格（Settings 改后持久化，Today/SubmitMood 调 generate_now 时用）
  // 不传时 fallback 'ambient' —— 与后端 generation/style_presets 兼容
  const style = ref<string>(loadStoredStyle())
  // v0.8: 时间窗口 —— SubmitMood 选完后存这里，Artworks regenerate 时复用
  const timeRangeStartMs = ref<number>(0)
  const timeRangeEndMs = ref<number>(0)

  // 持久化：tz 变更立即写 localStorage
  watch(timezoneOffsetMinutes, (v) => saveStoredOffset(v))
  // v0.3.1: style 变更立即写 localStorage
  watch(style, (v) => saveStoredStyle(v))

  /**
   * v0.3 下载目录 setter —— 写 store + localStorage（确保 App 重启后保留）
   * 验证意图：用户改了目录 → 立即生效 + 不依赖 ensureDefaultDir 自动覆盖
   */
  function setDownloadDir(dir: string): void {
    downloadDir.value = dir
    try {
      localStorage.setItem('fingertip.downloadDir', dir)
    } catch (e) {
      // localStorage 配额满 / 隐私模式禁用 → 至少保留内存中本次会话生效
      console.warn('[app store] failed to persist downloadDir:', e)
    }
  }

  /**
   * v0.3 启动时调用：从 localStorage 读回 downloadDir（必须在 ensureDefaultDir 之前）。
   * 验证意图：用户上次配过目录 → 启动时不自动覆盖；只在新装首次启动时才走默认目录创建。
   */
  function loadDownloadDir(): void {
    try {
      const stored = localStorage.getItem('fingertip.downloadDir')
      if (stored) downloadDir.value = stored
    } catch (e) {
      console.warn('[app store] failed to load downloadDir:', e)
    }
  }

  return {
    todaySummary,
    moodWord,
    generating,
    generationResult,
    timezoneOffsetMinutes,
    downloadDir,
    setDownloadDir,
    loadDownloadDir,
    style,
    timeRangeStartMs,
    timeRangeEndMs,
  }
})

// v0.3.1: style 持久化 helper
export const STYLE_STORAGE_KEY = 'fingertip_style'
const VALID_STYLES = ['ambient', 'jazz', 'cinematic', 'lo-fi', 'lofi']

function loadStoredStyle(): string {
  try {
    const raw = localStorage.getItem(STYLE_STORAGE_KEY)
    if (raw && VALID_STYLES.includes(raw)) return raw
  } catch {
    // 隐私模式 / 无 storage → 静默
  }
  return 'ambient'
}

function saveStoredStyle(style: string): void {
  try {
    localStorage.setItem(STYLE_STORAGE_KEY, style)
  } catch {
    // 隐私模式 / 配额满 → 静默
  }
}