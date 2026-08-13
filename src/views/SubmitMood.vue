<template>
  <section class="ft-mood-canvas ft-stagger ft-stagger-1">
    <div class="ft-page-eyebrow">提交心情</div>
    <h2 class="ft-mood-prompt">今天是什么感觉？</h2>
    <p class="ft-mood-hint">它会成为今日 AI 音乐与画作的情绪种子</p>

    <input
      type="text"
      class="ft-mood-input"
      placeholder="一个词..."
      v-model="store.moodWord"
      maxlength="20"
    />

    <div class="ft-mood-chips">
      <div
        v-for="chip in moodChips"
        :key="chip"
        class="ft-mood-chip"
        @click="store.moodWord = chip"
      >
        {{ chip }}
      </div>
    </div>

    <div class="ft-mood-style">
      <button
        v-for="style in styles"
        :key="style"
        class="ft-style-chip"
        :class="{ active: selectedStyle === style }"
        @click="selectedStyle = style"
      >
        {{ style }}
      </button>
    </div>

    <!-- v0.8: 自定义时间窗口（始终可见，默认 48h）—— 同学 v0.7 完整管线移植 -->
    <div class="ft-time-window">
      <div class="ft-time-window-label">选择时间窗口（默认 48 小时）</div>
      <div class="ft-time-window-inputs">
        <div class="ft-time-field">
          <label>从</label>
          <input type="datetime-local" v-model="rangeStart" class="ft-datetime-input" />
        </div>
        <span class="ft-time-sep">—</span>
        <div class="ft-time-field">
          <label>到</label>
          <input type="datetime-local" v-model="rangeEnd" class="ft-datetime-input" />
        </div>
        <button class="ft-time-reset" @click="resetRange" title="恢复默认 48h">↺</button>
      </div>
    </div>

    <button
      class="ft-generate-btn"
      :disabled="!store.moodWord || store.generating"
      @click="onGenerate"
    >
      {{ store.generating ? '生成中…' : '生成今日作品' }}
    </button>

    <div v-if="store.generating || store.generationResult" style="margin-top: var(--sp-6); color: var(--accent-grow); font-size: 13px;">
      已生成，正在前往作品页…
    </div>
    <div v-if="error" style="margin-top: var(--sp-6); color: var(--accent-warm); font-size: 13px;">
      生成失败：{{ error }}
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAppStore } from '@/stores/app'
import type { GenerateNowResult } from '@/types/artwork'

const store = useAppStore()
const router = useRouter()
const moodChips = ['focused', 'tired', 'excited', 'calm', 'anxious', 'grateful', 'curious']
const styles = ['Ambient', 'Jazz', 'Cinematic', 'Lo-fi']
// v0.3.1: 初始值从 store.style 读（Settings 偏好），用户选完新风格后写回 store
// 首字母大写用于 UI 标签，传到后端时 lower()
const selectedStyle = ref(titleCase(store.style))
const error = ref<string | null>(null)

// v0.8: 自定义时间窗口（始终可见，默认 48h）
const rangeStart = ref(defaultStartStr())
const rangeEnd = ref(defaultEndStr())

function defaultStartStr(): string {
  const d = new Date()
  d.setDate(d.getDate() - 1); d.setHours(0, 0, 0, 0)
  return toDatetimeLocalStr(d)
}
function defaultEndStr(): string {
  const d = new Date()
  d.setDate(d.getDate() + 1); d.setHours(0, 0, 0, 0)
  return toDatetimeLocalStr(d)
}
function resetRange() {
  rangeStart.value = defaultStartStr()
  rangeEnd.value = defaultEndStr()
}
function toDatetimeLocalStr(d: Date): string {
  const y = d.getFullYear()
  const mo = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const h = String(d.getHours()).padStart(2, '0')
  const mi = String(d.getMinutes()).padStart(2, '0')
  return `${y}-${mo}-${day}T${h}:${mi}`
}

function titleCase(s: string): string {
  if (!s) return 'Ambient'
  // 兼容 'lofi' 别名（后端 preset_for 接受 'lofi'）—— UI 一律显示 'Lo-fi'
  if (s.toLowerCase() === 'lofi') return 'Lo-fi'
  return s.charAt(0).toUpperCase() + s.slice(1)
}

function todayStr(): string {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

async function onGenerate() {
  if (!store.moodWord) {
    // 按钮 disabled 应拦住，但到达这里说明有状态异常 —— 打日志暴露
    console.warn('[SubmitMood] 未选择心情词但点击到达 onGenerate')
    return
  }
  store.generating = true
  error.value = null
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const today = todayStr()
    const mood = store.moodWord
    const style = selectedStyle.value.toLowerCase()
    // v0.3.1: 用户本次选的风格写回 store（持久化到 localStorage），让 Settings
    // 和下次 SubmitMood 默认值同步
    store.style = style
    // 1) 先持久化心情（set_mood），再生成
    await invoke('set_mood', { date: today, mood })
    console.log('[SubmitMood] set_mood 完成 → 调 generate_now', { today, mood, style })
    // 2) 生成音乐/画作（v0.8: 存时间窗口到 store，供 regenerate 复用）
    const startMs = new Date(rangeStart.value).getTime()
    const endMs = new Date(rangeEnd.value).getTime()
    store.timeRangeStartMs = startMs
    store.timeRangeEndMs = endMs
    const genJson = await invoke<string>('generate_now', {
      date: today, mood, style,
      startMs, endMs,
    })
    console.log('[SubmitMood] generate_now 返回长度 =', genJson?.length ?? 'null')
    if (!genJson) {
      // 后端返回空串 —— 契约外行为，不再静默
      error.value = '后端返回了空结果（见控制台/终端日志）'
      return
    }
    const parsed = JSON.parse(genJson) as GenerateNowResult
    console.log('[SubmitMood] 解析成功: 音乐 model =', parsed.music?.model, '| 图像 model =', parsed.art?.model, '| wav =', parsed.music_wav_path)
    // spread 保留后端透传的 music_wav_path/art_png_path（Artworks 播放/下载依赖）
    store.generationResult = {
      ...parsed,
      date: parsed.date ?? today,
      mood: parsed.mood ?? mood,
      style: parsed.style ?? style,
    }
    router.push('/artworks')
  } catch (e: any) {
    console.error('[SubmitMood] generate_now 失败:', e)
    const raw = e?.message ?? String(e)
    // 空错误消息也给出可见提示（v-if="error" 对空串为假 → 之前会静默）
    error.value = raw.trim() || '未知错误（详情见控制台/终端日志）'
  } finally {
    store.generating = false
  }
}
</script>

<style scoped>
/* v0.8: 自定义时间窗口（同学 v0.7 完整管线移植） */
.ft-time-window {
  margin-top: var(--sp-5);
  text-align: left;
  padding: var(--sp-4);
  background: var(--bg-elevated);
  border: 1px solid var(--border-subtle);
  border-radius: var(--r-md);
}
.ft-time-window-label {
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: var(--sp-3);
}
.ft-time-window-inputs {
  display: flex;
  align-items: flex-end;
  gap: var(--sp-3);
}
.ft-time-field {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.ft-time-field label {
  font-size: 11px;
  color: var(--text-tertiary);
}
.ft-time-sep {
  font-size: 14px;
  color: var(--text-tertiary);
  padding-bottom: 7px;
}
.ft-datetime-input {
  padding: 6px 10px;
  border: 1px solid var(--border-default);
  border-radius: var(--r-sm);
  font-size: 13px;
  font-family: inherit;
  background: var(--bg-surface);
  color: var(--text-primary);
}
.ft-time-reset {
  background: none;
  border: 1px solid var(--border-default);
  border-radius: var(--r-sm);
  padding: 6px 10px;
  font-size: 16px;
  cursor: pointer;
  color: var(--text-secondary);
  margin-bottom: 1px;
}
.ft-time-reset:hover {
  color: var(--accent-warm);
  border-color: var(--accent-warm);
}
</style>