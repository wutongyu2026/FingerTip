<template>
  <section class="ft-stagger ft-stagger-1">
    <div class="ft-page-header">
      <div class="ft-page-header-text">
        <div class="ft-page-eyebrow">过去 7 天</div>
        <h1 class="ft-page-title">你的节奏档案</h1>
        <p class="ft-page-subtitle">每一天都被一个词、一段音乐、一幅画珍藏。</p>
      </div>
    </div>

    <div v-if="loading" class="ft-history-loading">
      加载中…
    </div>

    <div v-else-if="days.length === 0" class="ft-history-empty">
      <div class="ft-empty-text">还没有历史记录</div>
      <div class="ft-empty-hint">提交心情或开始一日按键，让节奏档案开始生长</div>
    </div>

    <div v-else class="ft-history-grid">
      <div
        v-for="day in days"
        :key="day.date"
        class="ft-day-card"
        @click="goToArtworks(day)"
      >
        <div class="ft-day-date">{{ formatDateCN(day.date) }}</div>
        <div class="ft-day-theme">{{ day.theme_word || '—' }}</div>
        <div class="ft-day-mood">
          {{ day.mood_word || '—' }} · {{ day.total_keys.toLocaleString() }} keys
        </div>
        <div class="ft-day-dots" :title="`强度 ${day.intensity?.toFixed(0)} · 平稳 ${day.steadiness?.toFixed(2)} · 流畅 ${(day.fluency*100).toFixed(0)}% · 活跃 ${day.activity_hours}h`">
          <span class="ft-day-dot" :class="{ 'is-fast': (day.intensity ?? 0) >= 800 }" title="快"></span>
          <span class="ft-day-dot" :class="{ 'is-stable': (day.steadiness ?? 1) <= 0.8 }" title="稳"></span>
          <span class="ft-day-dot" :class="{ 'is-fluent': (day.fluency ?? 1) < 0.10 }" title="流"></span>
          <span class="ft-day-dot" :class="{ 'is-active': (day.activity_hours ?? 0) > 4 }" title="活"></span>
          <span class="ft-day-dot-labels">
            <span>快</span><span>稳</span><span>流</span><span>活</span>
          </span>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { formatDateCN } from '@/utils/timezone'
import type { GenerateNowResult } from '@/types/artwork'

interface DailySummaryRow {
  date: string
  total_keys: number
  theme_word: string
  mood_word: string | null
  top_keys_json: string
  // v0.3.5 新增
  intensity: number
  steadiness: number
  fluency: number
  activity_hours: number
  key_class_json: string
}

const days = ref<DailySummaryRow[]>([])
const loading = ref(true)
const router = useRouter()
const store = useAppStore()

async function refresh(): Promise<void> {
  loading.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('list_summaries', { limit: 7 })
    if (json && json !== 'null') {
      const parsed = JSON.parse(json)
      days.value = Array.isArray(parsed) ? parsed : []
    } else {
      days.value = []
    }
  } catch {
    // web 环境或未配置 invoke —— 静默空态
    days.value = []
  } finally {
    loading.value = false
  }
}

/**
 * v0.3.2: 点击 day card 拉取该日历史作品（Music + Art），存 store 后跳 /artworks。
 *
 * 后端 artifacts 表存了 generate_now 每次产出的 JSON —— 走与 generate_now 同一渲染路径，
 * Artworks.vue 不需要区分"实时生成" vs "历史回看"。
 */
async function goToArtworks(day: DailySummaryRow): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('get_artifact', { date: day.date })
    if (json && json !== 'null') {
      const parsed = JSON.parse(json) as GenerateNowResult
      // spread 保留后端透传的 music_wav_path/art_png_path（Artworks 播放/下载依赖）
      store.generationResult = {
        ...parsed,
        date: parsed.date ?? day.date,
        mood: parsed.mood ?? day.mood_word ?? '',
        style: parsed.style ?? '',
      }
      router.push('/artworks')
    } else {
      // 该日没生成过作品（只聚合没生成）—— 静默不跳
      console.info(`[history] no artifact for ${day.date}, skip`)
    }
  } catch (e) {
    console.warn('get_artifact failed:', e)
  }
}

onMounted(refresh)
</script>

<style scoped>
.ft-day-dots {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: var(--sp-3);
  padding-top: var(--sp-3);
  border-top: 1px dashed var(--border-subtle);
}
.ft-day-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--border-default);
  transition: background 200ms;
}
.ft-day-dot.is-fast { background: var(--accent-warm); }
.ft-day-dot.is-stable,
.ft-day-dot.is-fluent,
.ft-day-dot.is-active { background: var(--accent-grow); }
.ft-day-dot-labels {
  display: flex;
  gap: 4px;
  margin-left: auto;
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}
.ft-day-dot-labels span {
  min-width: 12px;
  text-align: center;
}
</style>
