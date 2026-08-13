<template>
  <!-- 第一行：Hero —— 显示真实的主题词（也支持空态） -->
  <section class="ft-hero-section ft-stagger ft-stagger-1">
    <div class="ft-hero-compact">
      <div class="ft-hero-eyebrow">
        <span class="ft-hero-eyebrow-line"></span>
        <span>今日主题词 · {{ heroStateLabel }}</span>
      </div>
      <template v-if="themeWord">
        <div class="ft-theme-word-compact">{{ themeWord }}</div>
      </template>
      <template v-else>
        <div class="ft-hero-empty">
          <div class="ft-theme-word-compact ft-theme-word-compact--empty">—</div>
          <p class="ft-hero-hint">今日主题词会在你的按键中自然浮现</p>
        </div>
      </template>
      <dl class="ft-theme-meta-compact">
        <div class="ft-meta-row">
          <dt>总按键</dt>
          <dd>{{ totalKeys.toLocaleString() }}</dd>
        </div>
        <div class="ft-meta-row">
          <dt>心情</dt>
          <dd>{{ moodWord || '—' }}</dd>
        </div>
        <div class="ft-meta-row">
          <dt>峰值小时</dt>
          <dd>{{ peakHour || '—' }}</dd>
        </div>
        <div class="ft-meta-row">
          <dt>首活时间</dt>
          <dd>{{ firstActiveDisplay || '—' }}</dd>
        </div>
      </dl>
    </div>
  </section>

  <!-- 第二行：5 张统计卡（v0.3.5 新增 intensity/steadiness/fluency/activity；v0.3.10 删手动聚合） -->
  <section class="ft-stats-row ft-stats-row--5 ft-stagger ft-stagger-2">
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">密集度 density</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.intensity != null ? summary.intensity.toFixed(0) : '—' }}
        <span class="ft-unit">键/h</span>
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.intensity ?? 0) >= 800 }">
        <template v-if="(summary?.intensity ?? 0) >= 800">快</template>
        <template v-else-if="(summary?.intensity ?? 0) > 0">慢</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">平稳度 stability</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.steadiness != null ? summary.steadiness.toFixed(2) : '—' }}
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.steadiness ?? 1) <= 0.8 }">
        <template v-if="(summary?.steadiness ?? 1) <= 0.8">平稳</template>
        <template v-else-if="(summary?.steadiness ?? 0) > 0">跳跃</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">流畅度 fluency</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.fluency != null ? (summary.fluency * 100).toFixed(0) : '—' }}<span class="ft-unit">%</span>
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.fluency ?? 1) < 0.10 }">
        <template v-if="(summary?.fluency ?? 1) < 0.10">流畅</template>
        <template v-else-if="(summary?.fluency ?? 0) > 0">停顿</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">活跃度 activity</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">
        {{ summary?.activity_hours ?? '—' }}<span class="ft-unit">h</span>
      </div>
      <div class="ft-stat-mini-delta" :class="{ up: (summary?.activity_hours ?? 0) > 4 }">
        <template v-if="(summary?.activity_hours ?? 0) > 4">活跃</template>
        <template v-else-if="(summary?.activity_hours ?? 0) > 0">不活跃</template>
        <template v-else>—</template>
      </div>
    </div>
    <div class="ft-stat-mini">
      <div class="ft-stat-mini-label">高峰按键</div>
      <div class="ft-stat-mini-value ft-stat-mini-value--mono">{{ peakCount }}</div>
      <div class="ft-stat-mini-delta">最忙小时</div>
    </div>
  </section>

  <!-- 第三行：24h 热力图 + Top 5 节奏指纹 -->
  <section class="ft-today-bottom ft-stagger ft-stagger-3">
    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">一天的节奏地图</div>
        <div class="ft-panel-meta">24h · 每格 = 1h</div>
      </div>
      <div class="ft-hourly-grid" v-if="hourlyCells.length === 24">
        <div v-for="(level, hour) in hourlyCells" :key="hour" class="ft-hour-cell" :data-level="level" :title="`${hour}:00 — ${hourlyLevels[hour] || 0} 键`"></div>
      </div>
      <div v-else class="ft-empty">
        <div class="ft-empty-text">还没有今日按键数据</div>
        <div class="ft-empty-hint">按一些键，约 60 秒后自动聚合</div>
      </div>
      <div class="ft-hour-axis" v-if="hourlyCells.length === 24">
        <span v-for="h in 24" :key="h">{{ h - 1 }}</span>
      </div>
      <div class="ft-heatmap-legend">
        <span>少</span>
        <div class="ft-legend-cells">
          <div class="ft-legend-cell" data-level="1" style="background: rgba(214,123,79,0.18);"></div>
          <div class="ft-legend-cell" data-level="2" style="background: rgba(214,123,79,0.4);"></div>
          <div class="ft-legend-cell" data-level="3" style="background: rgba(214,123,79,0.7);"></div>
          <div class="ft-legend-cell" data-level="4" style="background: #D67B4F;"></div>
        </div>
        <span>多</span>
      </div>
    </div>

    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">节奏指纹</div>
        <div class="ft-panel-meta">Top 5 按键（实时）</div>
      </div>
      <div class="ft-key-list" v-if="topKeys.length > 0">
        <div v-for="key in topKeys" :key="key.code" class="ft-key-row">
          <div class="ft-key-glyph">{{ key.glyph }}</div>
          <div class="ft-key-bar-track">
            <div class="ft-key-bar-fill" :style="{ width: key.percent + '%' }"></div>
          </div>
          <div class="ft-key-percent">{{ key.percent.toFixed(1) }}%</div>
        </div>
      </div>
      <div v-else class="ft-empty">
        <div class="ft-empty-text">还没有按键记录</div>
        <div class="ft-empty-hint">已捕获的键约每分钟自动聚合</div>
      </div>
    </div>
  </section>

  <!-- 第四行：键位分类水平条（v0.3.5 新增） -->
  <section class="ft-key-class-section ft-stagger ft-stagger-4">
    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">键位分类</div>
        <div class="ft-panel-meta">今日按键 · 游戏 / 文本 / 功能</div>
      </div>
      <div class="ft-key-class-bar" v-if="keyClass">
        <div class="ft-key-class-seg" :style="{ width: keyClass.game_ratio + '%' }" :title="`游戏键 ${keyClass.game_keys}`">
          <span class="ft-key-class-label">游戏 {{ keyClass.game_keys }}</span>
        </div>
        <div class="ft-key-class-seg ft-key-class-seg--text" :style="{ width: keyClass.text_ratio + '%' }" :title="`文本键 ${keyClass.text_keys}`">
          <span class="ft-key-class-label">文本 {{ keyClass.text_keys }}</span>
        </div>
        <div class="ft-key-class-seg ft-key-class-seg--mod" :style="{ width: keyClass.modifier_ratio + '%' }" :title="`功能键 ${keyClass.modifier_keys}`">
          <span class="ft-key-class-label">功能 {{ keyClass.modifier_keys }}</span>
        </div>
      </div>
      <div v-else class="ft-empty">
        <div class="ft-empty-text">还没有按键记录</div>
        <div class="ft-empty-hint">已捕获的键约每分钟自动聚合</div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { todayStrInTz } from '@/utils/timezone'
import { keyCodeToGlyph } from '@/utils/keycode-glyph'

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
  // v0.3.6 新增
  first_active_ms: number
}

// 真实数据：onMounted 时通过 Tauri Command 拉取今日 summary
// web 环境会 fallback 到空态（不显示 mock）
const summary = ref<DailySummaryRow | null>(null)
const keyCountNow = ref(0) // 直接读 key_events 表（新鲜计数）
const loading = ref(true)
const store = useAppStore()
const hourlyLevels = ref<number[]>([])  // 真实 hourly[24]，从后端读

// v0.2.2 时区：今日日期用用户配置的 offset 解读
function todayStr(): string {
  return todayStrInTz(store.timezoneOffsetMinutes)
}

async function refresh(): Promise<void> {
  loading.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const offsetMinutes = store.timezoneOffsetMinutes
    const json = await invoke<string>('get_today_summary', { date: todayStr() })
    if (json && json !== 'null') {
      summary.value = JSON.parse(json)
    } else {
      summary.value = null
    }
    // 顺便读 hook 是否工作（v0.2.4 接 offset 参数）
    keyCountNow.value = await invoke<number>('get_today_key_count', { offsetMinutes })
    // 读 24h 真实分布（用于活跃小时、高峰、热力图）
    const hl = await invoke<number[]>('get_today_hourly', { offsetMinutes })
    hourlyLevels.value = Array.isArray(hl) && hl.length === 24 ? hl : []
  } catch {
    // web 环境或未配置 invoke —— 静默空态
  } finally {
    loading.value = false
  }
}

// v0.2.4 时区切换 → 重新拉取 hourly / keyCount（summary 的 date 也变）
watch(() => store.timezoneOffsetMinutes, () => {
  refresh()
})

onMounted(refresh)
// 每 5 秒自动刷新 key_count，让"按了键 → 数字涨"的反馈看得见
let pollHandle: number | undefined
onMounted(() => {
  pollHandle = window.setInterval(() => {
    refresh()
  }, 5000)
})
onUnmounted(() => {
  if (pollHandle !== undefined) window.clearInterval(pollHandle)
})

// —— 计算属性：严格从 summary + keyCountNow + hourlyLevels 派生，不 fallback mock ——
// 当 summary 为 null 时（空态）显示 0 + 空字符串，前端不应误读。
const themeWord = computed(() => summary.value?.theme_word ?? '')
const totalKeys = computed(() => summary.value?.total_keys ?? keyCountNow.value)
const moodWord = computed(() => summary.value?.mood_word ?? '')

// v0.4.2 美化：hero 状态标签 —— 加载中 / 等待数据 / 已聚合，不再空态也显示「已聚合」
const heroStateLabel = computed(() => {
  if (loading.value) return '加载中'
  if (summary.value) return '已聚合'
  return '等待数据'
})

// hourly 已经是顶层 ref，无需 computed 包装

// 把 hourly[24] 转成热力图用的"level 1-4"等级数组（最高值/4 = level 4）
// 每格颜色等级（与之前 placeholder 视觉一致）
const hourlyLevelsForChart = computed<number[]>(() => {
  const hl = hourlyLevels.value
  if (hl.length !== 24) return []
  const max = Math.max(...hl, 1)
  return hl.map((v) => {
    if (v === 0) return 0
    const ratio = v / max
    if (ratio < 0.25) return 1
    if (ratio < 0.5) return 2
    if (ratio < 0.75) return 3
    return 4
  })
})

// 把 hourly[24] 转成 24h cells（按每格像素估算——level 用于颜色）
const hourlyCells = computed(() => hourlyLevelsForChart.value)

const topKeys = computed(() => {
  try {
    const json = summary.value?.top_keys_json
    if (!json) return []
    const parsed = JSON.parse(json) as Array<[number, number]>
    return parsed.slice(0, 5).map(([code, count]) => ({
      code,
      // 委托给 keyCodeToGlyph：字母/数字原样，控制键用 Unicode 符号，
      // 未知 code 才退化为 "?"。避免出现"空白格"或"一排问号"的假象。
      glyph: keyCodeToGlyph(code),
      count,
      // 计算百分比（在本地，不来自后端，避免依赖后端排序）
      percent: totalKeys.value > 0 ? (count / totalKeys.value) * 100 : 0
    }))
  } catch {
    return []
  }
})

// 高峰小时：从 hourly 找最大值对应的小时
const peakHour = computed(() => {
  const hl = hourlyLevels.value
  if (hl.length !== 24) return ''
  let maxV = 0, maxI = -1
  for (let i = 0; i < 24; i++) if (hl[i] > maxV) { maxV = hl[i]; maxI = i }
  if (maxI < 0 || maxV === 0) return ''
  return `${String(maxI).padStart(2, '0')}:00`
})

const peakCount = computed(() => {
  const tk = topKeys.value
  if (tk.length === 0) return '—'
  return tk[0].count.toString()
})

// v0.3.5 键位分类：把 summary.key_class_json (JSON: { game, text, modifier }) 解析为百分比
const keyClass = computed(() => {
  const json = summary.value?.key_class_json
  if (!json) return null
  try {
    const parsed = JSON.parse(json) as { game_keys: number; text_keys: number; modifier_keys: number }
    const total = parsed.game_keys + parsed.text_keys + parsed.modifier_keys
    if (total === 0) return null
    return {
      game_keys: parsed.game_keys,
      text_keys: parsed.text_keys,
      modifier_keys: parsed.modifier_keys,
      game_ratio: (parsed.game_keys / total) * 100,
      text_ratio: (parsed.text_keys / total) * 100,
      modifier_ratio: (parsed.modifier_keys / total) * 100,
    }
  } catch {
    return null
  }
})
// v0.3.6: 首活时间显示（HH:mm，按用户时区）
const firstActiveDisplay = computed(() => {
  const ms = summary.value?.first_active_ms
  if (!ms || ms === 0) return ''
  // 与 peakHour 同步：用 timezoneOffsetMinutes 把 UTC ms 移到用户时区
  const shifted = ms + store.timezoneOffsetMinutes * 60_000
  const d = new Date(shifted)
  const hh = String(d.getUTCHours()).padStart(2, '0')
  const mm = String(d.getUTCMinutes()).padStart(2, '0')
  return `${hh}:${mm}`
})
</script>

<style scoped>
/* ===================================================================
 * Today 页面：1100x760 满屏可见
 * 三段垂直：hero（满宽）→ stats（4 列横排）→ 24h + Top 5
 * 数据全部从后端 Tauri Command 读取，无 mock（首版可接受聚合前 0 数据空态）
 * ================================================================= */

/* 第一行：Hero 满宽 */
.ft-hero-section { margin-bottom: var(--sp-6); }
.ft-hero-compact {
  background: linear-gradient(135deg, var(--bg-surface), #FFFDF9);
  border: 1px solid var(--border-default);
  border-radius: var(--r-lg);
  padding: var(--sp-8) var(--sp-12);
  box-shadow: var(--shadow-1);
}
.ft-hero-eyebrow {
  color: var(--accent-warm);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.05em;
  margin-bottom: var(--sp-3);
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}
.ft-hero-eyebrow-line {
  width: 20px; height: 1px;
  background: var(--accent-warm);
}
.ft-theme-word-compact {
  font-family: var(--font-hand);
  font-size: 96px;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1;
  margin-bottom: var(--sp-6);
  letter-spacing: -0.02em;
}
/* v0.4.2 美化：空态 hero —— em dash 弱化 + 提示行，避免 96px 的「—」显得突兀 */
.ft-hero-empty {
  margin-bottom: var(--sp-6);
}
.ft-theme-word-compact--empty {
  font-size: 72px;
  color: var(--text-tertiary);
  opacity: 0.45;
  margin-bottom: var(--sp-1);
}
.ft-hero-hint {
  margin: 0;
  font-size: 13px;
  color: var(--text-tertiary);
}
.ft-theme-meta-compact {
  display: flex;
  flex-direction: column;
  gap: 0;
  margin: 0;
  padding: var(--sp-4) 0 0;
  border-top: 1px solid var(--border-subtle);
}
.ft-meta-row {
  display: flex; align-items: baseline; gap: var(--sp-4);
  font-size: 14px; padding: var(--sp-2) 0;
}
.ft-meta-row dt {
  color: var(--text-tertiary); font-size: 12px; font-weight: 500;
  min-width: 64px; letter-spacing: 0.04em; margin: 0;
}
.ft-meta-row dd {
  font-family: var(--font-mono); color: var(--text-primary);
  font-weight: 600; font-size: 16px; margin: 0;
}

/* 第二行：Stats */
.ft-stats-row {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--sp-4);
  margin-bottom: var(--sp-6);
}
.ft-stat-mini {
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: var(--r-md);
  padding: var(--sp-6) var(--sp-8);  /* 与 hero 同级（外疏内密），字不贴边 */
  position: relative;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
.ft-stat-mini-label {
  font-size: 11px;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  font-weight: 500;
}
.ft-stat-mini-value {
  font-family: var(--font-mono);
  font-size: 32px;
  font-weight: 700;
  line-height: 1;
  color: var(--text-primary);
}
.ft-stat-mini-value--mono {
  font-family: var(--font-mono);
}
.ft-stat-mini-value--hand {
  font-family: var(--font-hand);
  font-size: 28px;
}
.ft-unit {
  font-size: 13px;
  color: var(--text-tertiary);
  font-weight: 400;
  margin-left: 2px;
}
.ft-stat-mini-delta {
  font-size: 11px;
  color: var(--text-secondary);
  margin-top: auto;
}
.ft-stat-mini-delta.up { color: var(--accent-grow); }
.ft-stat-mini-delta.down { color: var(--accent-warm); }

/* 第三行：Panels */
.ft-today-bottom {
  display: grid;
  grid-template-columns: 1.5fr 1fr;
  gap: var(--sp-6);
}
.ft-today-bottom :deep(.ft-panel) {
  padding: var(--sp-6) var(--sp-8);
}
.ft-panel {
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: var(--r-md);
  padding: var(--sp-6) var(--sp-8);
}
.ft-panel-header {
  display: flex; align-items: center; justify-content: space-between;
  margin-bottom: var(--sp-5);
}
.ft-panel-title { font-size: 15px; font-weight: 600; }
.ft-panel-meta { font-size: 12px; color: var(--text-tertiary); }

/* 时段热力图 */
.ft-hourly-grid {
  display: grid;
  grid-template-columns: repeat(24, 1fr);
  gap: 2px;
}
.ft-hour-cell {
  aspect-ratio: 1;
  background: var(--bg-elevated);
  border-radius: 2px;
  transition: transform 200ms;
}
.ft-hour-cell[data-level="1"] { background: rgba(214, 123, 79, 0.18); }
.ft-hour-cell[data-level="2"] { background: rgba(214, 123, 79, 0.4); }
.ft-hour-cell[data-level="3"] { background: rgba(214, 123, 79, 0.7); }
.ft-hour-cell[data-level="4"] { background: var(--accent-warm); }
.ft-hour-axis {
  display: grid;
  grid-template-columns: repeat(24, 1fr);
  margin-top: var(--sp-1);
  font-size: 9px;
  color: var(--text-tertiary);
  text-align: center;
  font-family: var(--font-mono);
}
.ft-hour-axis span:nth-child(odd) { visibility: hidden; }

/* 空态 */
.ft-empty {
  padding: var(--sp-12) var(--sp-6);
  text-align: center;
  color: var(--text-tertiary);
  background: var(--bg-elevated);
  border-radius: var(--r-md);
  margin-top: var(--sp-3);
}
.ft-empty-text {
  font-size: 14px;
  font-weight: 500;
  margin-bottom: var(--sp-2);
}
.ft-empty-hint { font-size: 12px; opacity: 0.7; }

/* 图例 */
.ft-heatmap-legend {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  margin-top: var(--sp-3);
  font-size: 10px;
  color: var(--text-tertiary);
}
.ft-legend-cells { display: flex; gap: 2px; }
.ft-legend-cell { width: 12px; height: 12px; border-radius: 2px; }

/* Top 5 节奏指纹 */
.ft-key-list { display: flex; flex-direction: column; gap: var(--sp-3); }
.ft-key-row {
  display: grid;
  /* 第一列加宽到 48px 容纳 ⌘/⇧ 多字符 glyph 而不挤压百分比列 */
  grid-template-columns: 48px 1fr 50px;
  align-items: center;
  gap: var(--sp-4);
}
.ft-key-glyph {
  font-family: var(--font-mono);
  /* 14px 给 Unicode 修饰键符号（⎵ ⇥ ↵ ⌫ 等）足够空间而不溢出 */
  font-size: 14px;
  font-weight: 600;
  color: var(--accent-warm);
  min-width: 36px;
  text-align: center;
  line-height: 1.2;
  padding: var(--sp-2);
  background: var(--bg-elevated);
  border-radius: var(--r-sm);
  /* letter-spacing 收紧让单字符 glyph 居中更稳 */
  letter-spacing: 0;
}
.ft-key-bar-track {
  height: 8px;
  background: var(--bg-elevated);
  border-radius: 4px;
  overflow: hidden;
}
.ft-key-bar-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--accent-warm), #B87547);
  border-radius: 4px;
  transition: width 1s cubic-bezier(0.16, 1, 0.3, 1);
}
.ft-key-percent {
  font-family: var(--font-mono);
  font-size: 13px;
  font-weight: 600;
  text-align: right;
  color: var(--text-secondary);
}

/* 第二行：5 卡布局（v0.3.5 起 6 卡，v0.3.10 删手动聚合后 5 卡） */
.ft-stats-row--5 {
  grid-template-columns: repeat(5, 1fr);
}
@media (max-width: 1280px) {
  .ft-stats-row--5 {
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  }
}

/* 第四行：键位分类水平条（v0.3.5） */
.ft-key-class-section {
  margin-top: var(--sp-6);
}
.ft-key-class-bar {
  display: flex;
  height: 36px;
  border-radius: var(--r-sm);
  overflow: hidden;
  background: var(--bg-elevated);
}
.ft-key-class-seg {
  background: var(--accent-warm);
  color: #FFFFFF;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 600;
  transition: width 500ms ease;
  overflow: hidden;
  white-space: nowrap;
}
.ft-key-class-seg--text { background: #B87547; }
.ft-key-class-seg--mod { background: #8A8A8A; }
.ft-key-class-label {
  padding: 0 var(--sp-2);
}
</style>