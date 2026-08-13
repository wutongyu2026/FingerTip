<template>
  <section class="ft-stagger ft-stagger-1">
    <div class="ft-page-header">
      <div class="ft-page-header-text">
        <div class="ft-page-eyebrow">{{ dateDisplay }}</div>
        <h1 class="ft-page-title">今日作品</h1>
        <p class="ft-page-subtitle">从你的键盘节奏中诞生的 AI 创作。</p>
      </div>
      <div class="ft-privacy-badge">本地处理 · 无云端</div>
    </div>

    <!-- 今日画作 -->
    <div class="ft-art-grid">
      <div class="ft-art-block ft-art-block--art ft-stagger ft-stagger-2">
        <div class="ft-art-block-header ft-art-block-header--row">
          <div>
            <div class="ft-art-block-eyebrow">画作</div>
            <div class="ft-art-block-title">{{ artTitle }}</div>
            <p v-if="art?.description" class="ft-art-block-desc">{{ art.description }}</p>
          </div>
          <!-- v0.8: 重新生成画作按钮 -->
          <button
            v-if="art"
            class="ft-regenerate-icon-btn"
            :disabled="regeneratingArt"
            @click="onRegenerateArt"
            :title="regeneratingArt ? '正在重新生成…' : '不满意？重新生成画作'"
          >
            <span v-if="regeneratingArt" class="ft-regenerate-spinner"></span>
            <span v-else>🔄</span>
          </button>
        </div>
        <div v-if="regenerateArtError" class="ft-regenerate-error" role="alert">{{ regenerateArtError }}</div>
        <div class="ft-art-canvas">
          <!-- v-if="art"：有产物对象就显示按钮；若缺 png 路径（旧数据），点击会给出可见错误而非静默 -->
          <button v-if="art" class="ft-art-download" :disabled="exportingArt" @click="onDownloadArt" aria-label="下载画作 png">⬇</button>
          <img
            v-if="artPngUrl"
            :src="artPngUrl"
            alt="今日画作"
            class="ft-art-canvas-el"
          />
          <div v-else class="ft-art-empty">
            <span class="ft-art-empty-mark" aria-hidden="true">▢</span>
            <span class="ft-art-empty-text">尚未生成今日画作</span>
            <span class="ft-art-empty-hint">去「心情」页提交今日感觉</span>
          </div>
        </div>
      </div>

      <!-- 今日音乐 -->
      <div class="ft-art-block ft-art-block--music ft-stagger ft-stagger-3">
        <div class="ft-art-block-header ft-art-block-header--row">
          <div>
            <div class="ft-art-block-eyebrow">音乐</div>
            <div class="ft-art-block-title">{{ musicTitle }}</div>
            <p v-if="music?.description" class="ft-art-block-desc">{{ music.description }}</p>
          </div>
          <!-- v0.8: 重新生成音乐按钮 -->
          <button
            v-if="music"
            class="ft-regenerate-icon-btn"
            :disabled="regeneratingMusic"
            @click="onRegenerateMusic"
            :title="regeneratingMusic ? '正在重新生成…' : '不满意？重新生成音乐'"
          >
            <span v-if="regeneratingMusic" class="ft-regenerate-spinner"></span>
            <span v-else>🔄</span>
          </button>
        </div>
        <div v-if="regenerateMusicError" class="ft-regenerate-error" role="alert">{{ regenerateMusicError }}</div>
        <div v-if="music" class="ft-music-player">
          <!-- 段 1：标题 + 副标题（真实数据，无 mock） -->
          <div class="ft-music-info">
            <div class="ft-music-title">{{ trackTitle }}</div>
            <div class="ft-music-meta">{{ trackMeta }}</div>
          </div>
          <!-- 段 2：波形（独立一行） + 进度 -->
          <div class="ft-music-waveform-row">
            <span
              v-for="(amp, i) in (music.amplitudes ?? []).slice(0, 36)"
              :key="i"
              class="ft-music-wave-bar"
              :style="{ height: (4 + (amp ?? 0) * 36) + 'px' }"
            />
          </div>
          <!-- 段 3：控件（播放 + 时长 + 下载） -->
          <div class="ft-music-controls">
            <button class="ft-music-play" :aria-label="isPlaying ? '停止' : '播放'" @click="isPlaying ? onStop() : onPlay()">
              <svg v-if="!isPlaying" width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
              <svg v-else width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="5" width="4" height="14" rx="1"/><rect x="14" y="5" width="4" height="14" rx="1"/></svg>
            </button>
            <div class="ft-music-time">
              <span class="ft-music-time-current">{{ formatTime(currentMs) }}</span>
              <span class="ft-music-time-sep">/</span>
              <span class="ft-music-time-total">{{ formatTime(durationMs || music.duration_ms || 0) }}</span>
            </div>
            <button class="ft-music-download" :disabled="exportingMusic" @click="onDownloadMusic" aria-label="下载音乐 wav">
              <span aria-hidden="true">⬇</span>
              <span>wav</span>
            </button>
          </div>
          <!-- 「失败要大声」：下载失败 / 缺产物时可见错误，不再静默 -->
          <div v-if="downloadError" class="ft-download-error" role="alert">{{ downloadError }}</div>
        </div>
        <div v-else class="ft-art-empty ft-art-empty--music">
          <span class="ft-art-empty-mark" aria-hidden="true">♪</span>
          <span class="ft-art-empty-text">尚未生成今日音乐</span>
          <span class="ft-art-empty-hint">生成今日作品时同步产出</span>
        </div>
      </div>
    </div>
  </section>

  <!-- v0.4: 句子面板 —— 直接读 store.generationResult?.sentence 字符串 -->
  <section v-if="sentenceText || englishSentence" class="ft-sentence-section ft-stagger ft-stagger-4">
    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">今日句子</div>
        <div class="ft-panel-meta">
          <span>由 Top 5 按键 + 主题词生成</span>
          <span v-if="themeExplanation" class="ft-theme-explanation">主题词「{{ themeWordDisplay }}」：{{ themeExplanation }}</span>
        </div>
        <!-- v0.8: 重新生成句子按钮 -->
        <button
          class="ft-regenerate-sentence-btn"
          :disabled="regenerating"
          @click="onRegenerateSentence"
          :title="regenerating ? '正在重新生成…' : '不满意？让 AI 重新写一句'"
        >
          <span v-if="regenerating" class="ft-regenerate-spinner"></span>
          {{ regenerating ? '重新生成中…' : '🔄 不满意？换一句' }}
        </button>
      </div>
      <div v-if="regenerateError" class="ft-regenerate-error" role="alert">{{ regenerateError }}</div>
      <!-- v0.6.3: 中英分行（编排器产出 english_sentence 时显示） -->
      <p v-if="sentenceText" class="ft-sentence-text">{{ sentenceText }}</p>
      <p v-if="englishSentence" class="ft-sentence-text-en">{{ englishSentence }}</p>
    </div>
  </section>

  <!-- v0.6.0: AI 键盘诊断（funny_summary）—— 编排器产出的搞笑总结 -->
  <section v-if="funnySummary" class="ft-funny-section ft-stagger ft-stagger-5">
    <div class="ft-funny-card">
      <div class="ft-funny-label">AI 键盘诊断</div>
      <p class="ft-funny-text">{{ funnySummary }}</p>
    </div>
  </section>

  <!-- v0.8: 分享卡片（同学完整版）—— 后端 create_share 生成海报卡片 PNG + 上传媒体 + 二维码 -->
  <section class="ft-qr-section ft-stagger ft-stagger-6">
    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">分享今日作品</div>
        <div class="ft-panel-meta">海报 + 扫码直达分享页 · 可直接播放</div>
      </div>
      <button
        class="ft-qr-btn"
        :disabled="qrGenerating"
        @click="onGenerateQr"
      >
        {{ qrGenerating ? '生成中…' : qrArtifact ? '重新生成' : '生成卡片图片' }}
      </button>
      <div v-if="qrError" class="ft-qr-error">{{ qrError }}</div>
      <div v-if="qrArtifact" class="ft-qr-result">
        <img v-if="qrCardUrl" :src="qrCardUrl" alt="分享卡片预览" class="ft-qr-img" />
        <div class="ft-qr-local">
          <span class="ft-qr-local-icon">✅</span>
          <p class="ft-qr-local-title">分享卡片已生成</p>
          <p class="ft-qr-local-hint">
            {{ qrArtifact.audio_ok ? '🎵 扫码直达分享页，可直接播放音乐' : '⚠️ 媒体上传失败，二维码回退到产品页' }}
          </p>
        </div>
        <div class="ft-qr-actions">
          <a class="ft-copy-btn" :href="qrArtifact.share_url" target="_blank" rel="noopener">打开分享页</a>
          <button class="ft-copy-btn" @click="onCopyShareLink">{{ qrCopied ? '已复制 ✓' : '复制分享链接' }}</button>
          <button class="ft-copy-btn" @click="onSaveCard">保存卡片图片</button>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue"
import { convertFileSrc } from "@tauri-apps/api/core"
import { useAppStore } from "@/stores/app"
import { useTonePlayback } from "@/composables/useTonePlayback"
import { downloadBlob, downloadWavFromPath } from "@/utils/download"
import { formatDateCN, todayStrInTz } from "@/utils/timezone"
import type { Art, Music } from "@/types/artwork"

interface QrArtifact {
  local_path: string
  audio_ok: boolean
  share_url: string
}

const store = useAppStore()
const player = useTonePlayback()
const { currentMs, durationMs } = player

const isPlaying = ref(false)
const exportingMusic = ref(false)
const exportingArt = ref(false)
// code-review 🔴1「失败要大声」：下载失败 / 缺产物文件时显示给用户，不再只 console.warn
const downloadError = ref<string | null>(null)

const qrArtifact = ref<QrArtifact | null>(null)
const qrGenerating = ref(false)
const qrError = ref<string | null>(null)
const qrCopied = ref(false)

// v0.8: 海报卡片预览 —— 后端 create_share 生成卡片 PNG 到本地 temp，这里 convertFileSrc 显示
const qrCardUrl = computed<string | null>(() => {
  const p = qrArtifact.value?.local_path
  if (!p) return null
  try {
    return convertFileSrc(p)
  } catch {
    return null
  }
})

// v0.8: 重新生成（regenerate_sentence / regenerate_art / regenerate_music）状态 + 错误
const regenerating = ref(false)
const regeneratingArt = ref(false)
const regeneratingMusic = ref(false)
const regenerateError = ref<string | null>(null)
const regenerateArtError = ref<string | null>(null)
const regenerateMusicError = ref<string | null>(null)
// 破解浏览器图片缓存：路径没变但内容已更新 → 加 query 版本号
const artVersion = ref(0)

// v0.4: typed art / music（读 store.generationResult）
//   Art 不再有 width/height/pixels；画作渲染改用 <img> + art_png_path。
const art = ref<Art | null>(null)
const music = ref<Music | null>(null)

// v0.4: art 用 art_png_path 渲染。用 Tauri's convertFileSrc 把本地文件路径转成可被 <img> 加载的 URL。
const artPngUrl = computed<string | null>(() => {
  const p = store.generationResult?.art_png_path
  if (!p) return null
  // convertFileSrc 在非 Tauri 环境（unit test / SSR）可能抛 —— 用 try/catch 防御
  try {
    // v0.8: artVersion 破缓存 —— regenerate_art 后路径不变但内容更新，加 ?v=N 强制刷新
    const base = convertFileSrc(p)
    return artVersion.value ? `${base}?v=${artVersion.value}` : base
  } catch {
    return null
  }
})

// v0.4: 句子直接由模板渲染（不再独立 invoke generate_sentence）
// 编排器在 generate_now 阶段就把 sentence 写入 artifacts + 透传到 store.generationResult.sentence
const sentenceText = computed<string>(() => store.generationResult?.sentence ?? '')

// v0.6.0: AI 键盘诊断（funny_summary）—— 编排器产出，2 句话、40-80 字的搞笑总结
// v0.9: 最多显示 2 句 —— 旧数据可能是 3 句长段，前端截断兜底（提示词侧同步收紧）
const funnySummary = computed<string>(() => {
  const raw = store.generationResult?.funny_summary ?? ''
  return raw.split(/(?<=[。！？!?])/).slice(0, 2).join('')
})

// v0.6.3: 编排器产出的英文句子 + 主题词解释（可选显示）
const englishSentence = computed<string>(() => store.generationResult?.english_sentence ?? '')
const themeExplanation = computed<string>(() => store.generationResult?.theme_explanation ?? '')

// 主题词显示优先从最新 Music/Art 读（regenerate_* 已更新）
const themeWordDisplay = computed<string>(() =>
  store.generationResult?.music?.theme_word
  || store.generationResult?.art?.theme_word
  || ''
)

// —— v0.4.2: 标题/副标题全部读真实数据，不再硬编码 mock（Abstract in orange / Ambient pulse 已删）——
// 画作 / 音乐卡标题：用今日主题词；无产物时显示状态占位（非伪造数据）
const artTitle = computed<string>(() => art.value?.theme_word || '等待生成')
const musicTitle = computed<string>(() => music.value?.theme_word || '等待生成')

// 音乐播放器内标题：风格 + 主题词（如 "Ambient · 晨光"）
const trackTitle = computed<string>(() => {
  const m = music.value
  if (!m) return ''
  const style = prettyStyle(m.style)
  return m.theme_word ? `${style} · ${m.theme_word}` : style
})

// 音乐播放器副标题：风格 · 时长 · 由心情 X + 主题词 Y 驱动
const trackMeta = computed<string>(() => {
  const m = music.value
  if (!m) return ''
  const parts: string[] = [prettyStyle(m.style)]
  const secs = Math.max(0, Math.round(m.duration_ms / 1000))
  if (secs > 0) {
    parts.push(secs >= 60 ? `${Math.floor(secs / 60)} 分 ${secs % 60} 秒预览` : `${secs} 秒预览`)
  }
  const drivers = [
    m.mood ? `心情 '${m.mood}'` : '',
    m.theme_word ? `主题词 '${m.theme_word}'` : '',
  ].filter(Boolean)
  if (drivers.length) parts.push(`由 ${drivers.join(' + ')} 驱动`)
  return parts.join(' · ')
})

// style 字段是小写（'ambient' / 'lo-fi'）→ 展示用大写（'Ambient' / 'Lo-fi'）
function prettyStyle(s: string): string {
  if (!s) return ''
  if (s.toLowerCase() === 'lofi') return 'Lo-fi'
  return s.charAt(0).toUpperCase() + s.slice(1)
}

function formatTime(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000))
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, "0")}`
}

// v0.2.2：用 generationResult.date 或今日显示，按用户时区
const dateDisplay = computed(() => {
  const d = store.generationResult?.date
  if (d) return formatDateCN(d)
  return formatDateCN(todayStrInTz(store.timezoneOffsetMinutes))
})

async function onPlay() { await player.play(); isPlaying.value = true }
async function onStop() { await player.stop(); isPlaying.value = false }

async function onDownloadMusic() {
  downloadError.value = null
  if (!store.generationResult) return
  // v0.3.7 R5 修复：读后端产物 music_wav_path → 拼 Blob → 走 Save As。
  // 之前调 player.exportWav() 但 exportWav 已被标记 deprecated 抛 Error。
  const wavPath = store.generationResult.music_wav_path
  if (!wavPath) {
    // 「失败要大声」：旧数据 / 未生成时明确告诉用户，而不是静默无反应
    downloadError.value = '今日音乐尚未生成 wav 文件，请先生成今日作品'
    return
  }
  exportingMusic.value = true
  try {
    const filename = defaultFilename('music', 'wav')
    const path = await downloadWavFromPath(wavPath, filename)
    if (path) console.info(`[artworks] saved music to ${path}`)
  } catch (e: any) {
    downloadError.value = `下载失败：${e?.message ?? e}`
    console.warn('downloadWav failed:', e)
  } finally {
    exportingMusic.value = false
  }
}

async function onDownloadArt() {
  // v0.4: Art 不再有 width/height/pixels —— 不再走 canvas.toBlob。
  // 走 art_png_path → readFile → Blob → downloadBlob → Save As，与 onDownloadMusic 模式对齐。
  downloadError.value = null
  const pngPath = store.generationResult?.art_png_path
  if (!pngPath) {
    // 「失败要大声」：旧数据 / 未生成时明确告诉用户，而不是静默无反应
    downloadError.value = '今日画作尚未生成 png 文件，请先生成今日作品'
    return
  }
  exportingArt.value = true
  try {
    const { readFile } = await import('@tauri-apps/plugin-fs')
    const bytes = await readFile(pngPath)
    const blob = new Blob([bytes], { type: 'image/png' })
    const filename = defaultFilename('art', 'png')
    const path = await downloadBlob(filename, blob, 'png', 'image/png')
    if (path) console.info(`[artworks] saved art to ${path}`)
  } catch (e: any) {
    downloadError.value = `下载失败：${e?.message ?? e}`
    console.warn('downloadArt failed:', e)
  } finally {
    exportingArt.value = false
  }
}

// —— v0.8: 重新生成（regenerate_*）—— 对齐同学 v0.7 完整管线 ——

/// 用户不满意当前句子时，只重跑编排器拿新句子，不动音乐/画作。
/// 传 startMs/endMs（时间窗口）—— 后端时间窗口指定时现场重算 theme_word。
async function onRegenerateSentence() {
  regenerateError.value = null
  const result = store.generationResult
  if (!result?.date) {
    regenerateError.value = '没有可重新生成的作品，请先生成'
    return
  }
  regenerating.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('regenerate_sentence', {
      date: result.date,
      mood: result.mood ?? '',
      style: result.style ?? 'ambient',
      startMs: store.timeRangeStartMs || null,
      endMs: store.timeRangeEndMs || null,
    })
    if (json) {
      const parsed = JSON.parse(json)
      if (store.generationResult) {
        store.generationResult.sentence = parsed.sentence
        store.generationResult.english_sentence = parsed.english_sentence
        store.generationResult.theme_explanation = parsed.theme_explanation
        if (parsed.funny_summary) {
          store.generationResult.funny_summary = parsed.funny_summary
        }
      }
    }
  } catch (e: any) {
    regenerateError.value = `重新生成失败：${e?.message ?? e}`
    console.warn('[artworks] regenerate_sentence failed:', e)
  } finally {
    regenerating.value = false
  }
}

/// 重新生成画作（用已有 description 再次调图像模型）
async function onRegenerateArt() {
  regenerateArtError.value = null
  const date = store.generationResult?.date
  if (!date) {
    regenerateArtError.value = '没有可重新生成的作品，请先生成'
    return
  }
  regeneratingArt.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('regenerate_art', {
      date,
      startMs: store.timeRangeStartMs || null,
      endMs: store.timeRangeEndMs || null,
    })
    if (json) {
      const parsed = JSON.parse(json)
      if (store.generationResult) {
        store.generationResult.art = parsed.art
        store.generationResult.art_png_path = parsed.art_png_path
      }
      art.value = parsed.art
      artVersion.value++
    }
  } catch (e: any) {
    regenerateArtError.value = `画作重新生成失败：${e?.message ?? e}`
    console.warn('[artworks] regenerate_art failed:', e)
  } finally {
    regeneratingArt.value = false
  }
}

/// 重新生成音乐（用已有 description 再次调音频模型）
async function onRegenerateMusic() {
  regenerateMusicError.value = null
  const date = store.generationResult?.date
  if (!date) {
    regenerateMusicError.value = '没有可重新生成的作品，请先生成'
    return
  }
  regeneratingMusic.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('regenerate_music', {
      date,
      startMs: store.timeRangeStartMs || null,
      endMs: store.timeRangeEndMs || null,
    })
    if (json) {
      const parsed = JSON.parse(json)
      if (store.generationResult) {
        store.generationResult.music = parsed.music
        store.generationResult.music_wav_path = parsed.music_wav_path
      }
      music.value = parsed.music
      const wavPath = parsed.music_wav_path as string
      if (wavPath) {
        try {
          await player.load(wavPath)
        } catch (e) {
          console.warn('[artworks] reload wav failed:', e)
        }
      }
    }
  } catch (e: any) {
    regenerateMusicError.value = `音乐重新生成失败：${e?.message ?? e}`
    console.warn('[artworks] regenerate_music failed:', e)
  } finally {
    regeneratingMusic.value = false
  }
}

async function onGenerateQr() {
  const date = store.generationResult?.date
  if (!date) {
    qrError.value = '没有今日作品，请先生成'
    return
  }
  qrGenerating.value = true
  qrError.value = null
  qrCopied.value = false
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    // v0.8: 传 english_sentence —— 后端完整版海报用它（本地重新生成过句子时透传最新值）
    const json = await invoke<string>('upload_and_generate_qr', {
      date,
      englishSentence: store.generationResult?.english_sentence ?? null,
    })
    if (json) {
      qrArtifact.value = JSON.parse(json)
    }
  } catch (e: any) {
    qrError.value = `上传失败：${e?.message ?? e}`
    console.warn('[artworks] upload_and_generate_qr failed:', e)
  } finally {
    qrGenerating.value = false
  }
}

async function onCopyShareLink() {
  const url = qrArtifact.value?.share_url
  if (!url) return
  try {
    await navigator.clipboard.writeText(url)
    qrCopied.value = true
    setTimeout(() => { qrCopied.value = false }, 2000)
  } catch (e: any) {
    qrError.value = `复制失败：${e?.message ?? e}`
    console.warn('[artworks] copy share link failed:', e)
  }
}

async function onSaveCard() {
  const path = qrArtifact.value?.local_path
  if (!path) return
  try {
    const { readFile } = await import('@tauri-apps/plugin-fs')
    const bytes = await readFile(path)
    const blob = new Blob([bytes], { type: 'image/png' })
    await downloadBlob(defaultFilename('share-card', 'png'), blob, 'png', 'image/png')
  } catch (e: any) {
    qrError.value = `保存卡片失败：${e?.message ?? e}`
    console.warn('[artworks] save share card failed:', e)
  }
}

// v0.3 Stage 5 Batch B Task 5.5: 统一默认文件名 FingerTip-{type}-YYYYMMDD-HHmmss.{ext}
function defaultFilename(prefix: string, ext: string): string {
  const d = new Date()
  const ymd =
    `${d.getFullYear()}` +
    `${String(d.getMonth() + 1).padStart(2, '0')}` +
    `${String(d.getDate()).padStart(2, '0')}`
  const hms =
    `${String(d.getHours()).padStart(2, '0')}` +
    `${String(d.getMinutes()).padStart(2, '0')}` +
    `${String(d.getSeconds()).padStart(2, '0')}`
  return `FingerTip-${prefix}-${ymd}-${hms}.${ext}`
}

// v0.4: 删 drawCanvas() 死代码 —— Art 改用 <img> 渲染 art_png_path，T13 完成。

onMounted(async () => {
  // 后端输出 { music: Music, art: Art, music_wav_path, art_png_path, sentence, ... }（v0.4+）
  const result = store.generationResult
  if (!result) return
  art.value = result.art ?? null
  music.value = result.music ?? null

  // v0.3.4+ 优先读本地 WAV（更快 + 音质稳定）
  // v0.3.2 旧数据（无 wav_path）跳过播放器（留给后续 upgrade）
  const wavPath = result.music_wav_path
  if (wavPath) {
    try {
      await player.load(wavPath)
    } catch (e) {
      console.warn('[artworks] load local wav failed:', e)
    }
  }

  // v0.4: 句子由 store.generationResult.sentence 直接渲染（template `v-if="sentenceText"`），
  // 不再独立 invoke generate_sentence（编排器已在 generate_now 阶段写入 artifacts.sentence）。
})
</script>

<style scoped>
/* v0.8: 重新生成按钮（对齐同学 v0.7 完整管线） */
.ft-art-block-header--row {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: var(--sp-3);
}
.ft-regenerate-icon-btn {
  flex: 0 0 auto;
  width: 34px;
  height: 34px;
  border-radius: 50%;
  border: 1px solid var(--border-default);
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 15px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: color 150ms, border-color 150ms, transform 150ms;
}
.ft-regenerate-icon-btn:hover:not(:disabled) {
  color: var(--accent-warm);
  border-color: var(--accent-warm);
  transform: rotate(30deg);
}
.ft-regenerate-icon-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.ft-regenerate-sentence-btn {
  flex: 0 0 auto;
  border: 1px solid var(--border-default);
  background: none;
  border-radius: var(--r-sm);
  padding: 5px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: color 150ms, border-color 150ms;
}
.ft-regenerate-sentence-btn:hover:not(:disabled) {
  color: var(--accent-warm);
  border-color: var(--accent-warm);
}
.ft-regenerate-sentence-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.ft-regenerate-error {
  color: var(--accent-warm);
  font-size: 12px;
  margin: 6px 0;
}
.ft-regenerate-spinner {
  display: inline-block;
  width: 10px;
  height: 10px;
  border: 2px solid currentColor;
  border-top-color: transparent;
  border-radius: 50%;
  animation: ft-spin 0.7s linear infinite;
  margin-right: 4px;
  vertical-align: -1px;
}
@keyframes ft-spin {
  to { transform: rotate(360deg); }
}
.ft-art-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-5);
}
.ft-art-block {
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: var(--r-md);
  padding: var(--sp-6) var(--sp-8);
  box-shadow: var(--shadow-1);
}
.ft-art-block--art, .ft-art-block--music { margin-bottom: 0; }
.ft-art-block-header {
  margin-bottom: var(--sp-6);
}
.ft-art-block-eyebrow {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  margin-bottom: var(--sp-3);
}
.ft-art-block-title {
  font-family: var(--font-hand);
  font-size: 28px;
  font-weight: 700;
  color: var(--accent-warm);
  line-height: 1.1;
  padding-bottom: var(--sp-2);
}

/* 画作 — 真 canvas 容器 */
.ft-art-canvas {
  /* 撑满卡片宽度（不依赖 aspect-ratio），固定高度让左右视觉重量一致 */
  width: 100%;
  height: 320px;
  background: var(--bg-elevated);
  border-radius: var(--r-md);
  position: relative;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 12px rgba(214, 123, 79, 0.15);
}
/* canvas 元素：等比缩放填满容器（source 256x256 → CSS 拉伸） */
.ft-art-canvas-el {
  width: 100%;
  height: 100%;
  object-fit: contain;
  image-rendering: pixelated;
  border-radius: var(--r-md);
}

/* 卡片描述（画作/音乐的自然语言描述） */
.ft-art-block-desc {
  margin: var(--sp-1) 0 0;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-secondary);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* 画作下载按钮 —— 悬浮在画作右上角（v0.4.2 补样式：此前未定义会裸渲染） */
.ft-art-download {
  position: absolute;
  top: var(--sp-3);
  right: var(--sp-3);
  z-index: 2;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  border: 1px solid var(--border-default);
  background: var(--bg-overlay);
  color: var(--text-primary);
  font-size: 15px;
  line-height: 1;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: transform 200ms, box-shadow 200ms, color 200ms, border-color 200ms;
  box-shadow: var(--shadow-1);
}
.ft-art-download:hover:not(:disabled) {
  transform: scale(1.06);
  box-shadow: var(--shadow-2);
  color: var(--accent-warm);
  border-color: var(--accent-warm);
}
.ft-art-download:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 空态（画作 / 音乐共用）—— 设计意图明确的占位，非假数据 */
.ft-art-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--sp-2);
  width: 100%;
  height: 100%;
  min-height: 200px;
  padding: var(--sp-8) var(--sp-4);
  text-align: center;
  color: var(--text-tertiary);
}
.ft-art-empty--music {
  min-height: 220px;
}
.ft-art-empty-mark {
  font-size: 30px;
  line-height: 1;
  color: var(--border-strong);
  margin-bottom: var(--sp-2);
}
.ft-art-empty-text {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
}
.ft-art-empty-hint {
  font-size: 12px;
  opacity: 0.75;
}

/* 音乐播放器 —— 三段式 v0.2.3 hotfix：信息 / 波形 / 控件 */
.ft-music-player {
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
  padding: var(--sp-6) var(--sp-8);
  background: var(--bg-elevated);
  border-radius: var(--r-md);
}
.ft-music-info {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
.ft-music-title {
  font-weight: 600;
  font-size: 15px;
  letter-spacing: 0.01em;
  color: var(--text-primary);
}
.ft-music-meta {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.55;
}
/* 波形独立行 —— 撑满宽度 36 条波 */
.ft-music-waveform-row {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 2px;
  height: 36px;
  padding: 0 var(--sp-1);
}
.ft-music-wave-bar {
  flex: 1;
  background: var(--accent-warm);
  border-radius: 2px;
  opacity: 0.5;
  min-width: 2px;
}
/* 控件：播放 + 时长 + 下载 */
.ft-music-controls {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
}
.ft-music-play {
  width: 44px;
  height: 44px;
  flex-shrink: 0;
  border-radius: 50%;
  border: none;
  background: var(--accent-warm);
  color: #FFFFFF;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: transform 200ms, box-shadow 200ms;
  box-shadow: 0 2px 8px rgba(214, 123, 79, 0.3);
}
.ft-music-play:hover {
  transform: scale(1.05);
  box-shadow: 0 4px 14px rgba(214, 123, 79, 0.45);
}
.ft-music-time {
  display: flex;
  align-items: baseline;
  gap: var(--sp-1);
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--text-secondary);
  margin-right: auto; /* 推到播放按钮右侧，把下载按钮顶到行末 */
}
.ft-music-time-current {
  color: var(--text-primary);
  font-weight: 600;
}
.ft-music-time-sep {
  opacity: 0.5;
}
.ft-music-time-total {
  opacity: 0.7;
}
.ft-music-download {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  padding: 10px 16px;
  border-radius: 100px;
  border: 1px solid var(--border-default);
  background: var(--bg-surface);
  color: var(--text-primary);
  cursor: pointer;
  font-size: 13px;
  font-family: inherit;
  font-weight: 500;
  transition: all 200ms;
}
.ft-music-download:hover:not(:disabled) {
  border-color: var(--accent-warm);
  color: var(--accent-warm);
  background: var(--accent-warm-soft);
}
.ft-music-download:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.ft-sentence-section {
  margin-top: var(--sp-6);
}
.ft-sentence-text {
  font-family: var(--font-hand);
  font-size: 28px;
  line-height: 1.4;
  color: var(--text-primary);
  margin: var(--sp-4) 0;
  padding: var(--sp-4) var(--sp-6);
  background: var(--bg-elevated);
  border-radius: var(--r-md);
  border-left: 3px solid var(--accent-warm);
}
.ft-qr-section {
  margin-top: var(--sp-6);
}
.ft-qr-btn {
  background: var(--text-primary);
  color: var(--bg-base);
  border: none;
  border-radius: var(--r-sm);
  padding: var(--sp-3) var(--sp-4);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  font-family: inherit;
  transition: transform 150ms, opacity 150ms;
}
.ft-qr-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  opacity: 0.9;
}
.ft-qr-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.ft-qr-error {
  margin-top: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  background: rgba(183, 62, 62, 0.1);
  border-radius: var(--r-sm);
  color: var(--accent-danger);
  font-size: 13px;
}
/* 下载错误 —— 与 qr-error 同款，保证失败在音乐卡片内可见 */
.ft-download-error {
  margin-top: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  background: rgba(183, 62, 62, 0.1);
  border-radius: var(--r-sm);
  color: var(--accent-danger);
  font-size: 13px;
}

/* v0.6.0: AI 键盘诊断 —— 橙色渐变 + 左边框，与落地页 .funny-card 对齐 */
.ft-funny-section { margin-top: var(--sp-6); }
.ft-funny-card {
  background: linear-gradient(135deg, rgba(214, 123, 79, 0.08), rgba(214, 123, 79, 0.03));
  border: 1px solid rgba(214, 123, 79, 0.2);
  border-left: 3px solid var(--accent-warm);
  border-radius: var(--r-md);
  /* v0.9: 左右留白 —— 内边距加大 + 内容居中限宽，不再从卡片左缘一路排到右缘 */
  padding: var(--sp-6) var(--sp-8);
  text-align: center;
}
.ft-funny-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--accent-warm);
  margin-bottom: var(--sp-3);
}
.ft-funny-text {
  /* v0.9: 居中限宽（约 44 个汉字一行），两侧自然留白 */
  margin: 0 auto;
  max-width: 44em;
  font-size: 14px;
  line-height: 1.75;
  color: var(--text-primary);
  letter-spacing: 0.01em;
}

/* v0.6.3: 句子卡片 —— 中英分行 + 主题词解释 */
.ft-sentence-text-en {
  font-family: var(--font-hand);
  font-size: 18px;
  font-style: italic;
  line-height: 1.5;
  color: var(--text-secondary);
  margin: var(--sp-2) 0 0;
  letter-spacing: 0.02em;
}
.ft-theme-explanation {
  display: inline-block;
  margin-left: var(--sp-3);
  padding-left: var(--sp-3);
  border-left: 1px solid var(--border-default);
  color: var(--text-secondary);
  font-style: italic;
}
.ft-qr-result {
  margin-top: var(--sp-4);
  text-align: center;
}
/* v0.8: 海报卡片是 16:9 横版（后端 CARD_W=1280 / CARD_H=720）—— 预览用宽横版 */
.ft-qr-img {
  width: 100%;
  max-width: 520px;
  aspect-ratio: 16 / 9;
  object-fit: cover;
  border-radius: var(--r-md);
  border: 1px solid var(--border-default);
  background: white;
}
.ft-qr-local {
  margin-top: var(--sp-4);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
}
.ft-qr-local-icon {
  font-size: 22px;
}
.ft-qr-local-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.ft-qr-local-hint {
  font-size: 12px;
  color: var(--text-secondary);
}
.ft-qr-actions {
  margin-top: var(--sp-4);
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
  justify-content: center;
}
.ft-copy-btn {
  border: 1px solid var(--border-default);
  background: var(--bg-surface);
  color: var(--text-secondary);
  padding: 7px 14px;
  border-radius: var(--r-sm);
  font-size: 13px;
  cursor: pointer;
  text-decoration: none;
  transition: color 150ms, border-color 150ms;
}
.ft-copy-btn:hover {
  color: var(--accent-warm);
  border-color: var(--accent-warm);
}
.ft-qr-warning {
  margin-top: var(--sp-2);
  font-size: 11px;
  color: var(--text-tertiary);
}
</style>