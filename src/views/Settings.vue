<template>
  <n-card title="设置">
    <n-space vertical :size="24">
      <!-- v0.2.4 环境检测提示 -->
      <n-alert v-if="!isTauri" type="warning" :show-icon="true" title="当前在 Web 浏览器中">
        设置项仅在 Tauri 桌面应用中生效；浏览器下的切换不会写入系统。
        要使用完整功能请运行 <code>pnpm tauri dev</code>。
      </n-alert>

      <!-- v0.3 AI 接入固定走云端（MiniMax），设置页不再暴露切换；后端已无 MinimaxCloudAdapter 顶层构造 -->

      <!-- v0.3.1: 偏好风格 —— 双向绑定到 store.style + 持久化到 localStorage -->
      <n-space vertical :size="8">
        <n-text>偏好风格：</n-text>
        <n-select
          v-model:value="style"
          :options="styleOptions"
        />
        <n-text depth="3" style="font-size: 12px">
          今日页 Recalculate 与心情页默认使用此风格。心情页可临时切换（不影响偏好设置）。
        </n-text>
      </n-space>

      <!-- v0.2.2 时区选择器 -->
      <n-space vertical :size="8">
        <n-text>时区：</n-text>
        <n-space align="center" :size="12">
          <n-select
            v-model:value="tzOffset"
            :options="tzOptions"
            style="width: 220px"
            @update:value="onChangeTz"
          />
          <n-text depth="3" style="font-size: 12px">
            当前显示时间基于 {{ currentTzLabel }}
          </n-text>
          <n-button size="tiny" @click="onAutoDetect" :disabled="!isTauri">自动检测</n-button>
        </n-space>
        <n-text depth="3" style="font-size: 12px">
          选择其他时区时，日期 / 时间显示在 UTC+0 基础上做加减运算。
          留空（UTC+0）表示与系统时区无关的时间解读。
        </n-text>
      </n-space>

      <n-divider />

      <n-space vertical :size="6">
        <n-space justify="space-between" align="center">
          <n-text>开机自启动（静默后台模式）</n-text>
          <n-switch
            :value="autostart"
            @update:value="onToggleAutostart"
            :loading="autostartLoading"
            :disabled="!isTauri"
          />
        </n-space>
        <n-text depth="3" style="font-size: 12px">
          启用后 FingerTip 在 Windows 启动时自动运行并隐藏窗口，Hook 静默记录键盘行为。
          <template v-if="isTauri">
            实际写入 <code>HKCU\Software\Microsoft\Windows\CurrentVersion\Run\FingerTip</code>。
            窗口隐藏后请在系统托盘区（左键单击显示）找到图标。
          </template>
        </n-text>
      </n-space>

      <!-- v0.3 Stage 5 Batch B Task 5.4: 下载输出目录 -->
      <n-divider />

      <n-space vertical :size="8">
        <n-text>下载输出目录：</n-text>
        <n-input
          v-model:value="downloadDir"
          placeholder="未设置（首次启动会自动配置）"
          readonly
        />
        <n-space>
          <n-button @click="onPickDownloadDir" :disabled="!isTauri">浏览…</n-button>
          <n-button quaternary @click="onResetDownloadDir">重置默认</n-button>
        </n-space>
        <n-text depth="3" style="font-size: 12px">
          下载作品（画作 / 音乐）时默认打开到此目录。首次启动会自动设为应用数据目录下的 downloads\。
          <template v-if="!isTauri">
            <br />当前为 Web 浏览器，设置不会生效。
          </template>
        </n-text>
      </n-space>

      <!-- v0.4 T14: 模型接入（引擎 / LLM / 图像 / 音频 三态路由配置） -->
      <n-divider />

      <n-space vertical :size="12">
        <n-text strong>模型接入</n-text>
        <n-text depth="3" style="font-size: 12px">
          本地 FingerTip-Engine（Python）优先，云端兑底。配置缺失时可留空，由后端按模式决定路由。
        </n-text>

        <!-- 引擎启用与地址 -->
        <n-space vertical :size="4">
          <n-space align="center" :size="12">
            <n-text>启用本地引擎：</n-text>
            <n-switch v-model:value="modelConfig.engine.enabled" />
          </n-space>
          <n-input
            v-model:value="modelConfig.engine.base_url"
            placeholder="http://127.0.0.1:8765"
          />
        </n-space>

        <n-divider style="margin: 4px 0" />

        <!-- LLM -->
        <n-text>LLM 编排</n-text>
        <n-select v-model:value="modelConfig.llm.mode" :options="modeOptions" />
        <n-text depth="3" style="font-size: 12px">本地 GGUF 模型路径</n-text>
        <n-input
          v-model:value="modelConfig.llm.local_gguf"
          placeholder="逗号分隔多路径（多 GGUF 备用）"
        />
        <n-text depth="3" style="font-size: 12px">云端 LLM Base URL（MiniMax）</n-text>
        <n-input v-model:value="modelConfig.llm.cloud_base" placeholder="https://api.minimaxi.com" />
        <n-text depth="3" style="font-size: 12px">云端 LLM Key（MiniMax）</n-text>
        <n-input
          type="password"
          v-model:value="modelConfig.llm.cloud_key"
          placeholder="MiniMax API key"
          show-password-on="click"
        />
        <n-text depth="3" style="font-size: 12px">云端 LLM Model（MiniMax）</n-text>
        <n-input v-model:value="modelConfig.llm.cloud_model" placeholder="MiniMax-M3" />

        <n-divider style="margin: 4px 0" />

        <!-- 图像 -->
        <n-text>图像生成</n-text>
        <n-select v-model:value="modelConfig.image.mode" :options="modeOptions" />
        <n-text depth="3" style="font-size: 12px">本地模型路径（SD1.5 GGUF）</n-text>
        <n-input v-model:value="modelConfig.image.local_model_path" />
        <n-text depth="3" style="font-size: 12px">云端图像 Base URL（MiniMax）</n-text>
        <n-input v-model:value="modelConfig.image.cloud_base" placeholder="https://api.minimaxi.com" />
        <n-text depth="3" style="font-size: 12px">云端图像 Key（MiniMax）</n-text>
        <n-input
          type="password"
          v-model:value="modelConfig.image.cloud_key"
          show-password-on="click"
          placeholder="MiniMax API key"
        />
        <n-text depth="3" style="font-size: 12px">云端图像 Model（MiniMax）</n-text>
        <n-input v-model:value="modelConfig.image.cloud_model" placeholder="image-01" />

        <n-divider style="margin: 4px 0" />

        <!-- 音频 -->
        <n-text>音频 / TTS</n-text>
        <n-select v-model:value="modelConfig.audio.mode" :options="modeOptions" />
        <n-text depth="3" style="font-size: 12px">MiniMax Base URL</n-text>
        <n-input v-model:value="modelConfig.audio.minimax_base" />
        <n-text depth="3" style="font-size: 12px">MiniMax Key</n-text>
        <n-input
          type="password"
          v-model:value="modelConfig.audio.minimax_key"
          show-password-on="click"
        />
        <n-text depth="3" style="font-size: 12px">MiniMax Model</n-text>
        <n-input v-model:value="modelConfig.audio.minimax_model" placeholder="music-3.0" />

        <!-- 保存 -->
        <n-space align="center" :size="12">
          <n-button type="primary" :loading="savingModel" @click="onSaveModel">
            保存模型配置
          </n-button>
          <n-text v-if="modelSaveOk" type="success" style="font-size: 12px">已保存</n-text>
          <n-text v-if="modelSaveError" type="error" style="font-size: 12px">
            {{ modelSaveError }}
          </n-text>
        </n-space>
      </n-space>
    </n-space>
  </n-card>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { buildTimezoneOptions, detectLocalOffsetMinutes, formatOffset } from '@/utils/timezone'

const store = useAppStore()
// v0.3.1: style 双向绑定到 store（store 写 localStorage 持久化）
const style = computed({
  get: () => store.style,
  set: (v: string) => { store.style = v },
})
const autostart = ref(false)
const autostartLoading = ref(false)

// v0.2.4 环境检测：Tauri 运行时才有 invoke，否则仅作 UI 提示
const isTauri = ref(false)
function detectTauri() {
  // window.__TAURI_INTERNALS__ 由 Tauri 2.x 在原生应用注入；浏览器模式不存在
  isTauri.value = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

// v0.2.2 时区 —— 双向绑定到 store，自动持久化
const tzOffset = computed({
  get: () => store.timezoneOffsetMinutes,
  set: (v: number) => { store.timezoneOffsetMinutes = v },
})
const tzOptions = buildTimezoneOptions()
const currentTzLabel = computed(() => formatOffset(tzOffset.value))

function onChangeTz(value: number) {
  store.timezoneOffsetMinutes = value
}

function onAutoDetect() {
  store.timezoneOffsetMinutes = detectLocalOffsetMinutes()
}

onMounted(async () => {
  detectTauri()
  if (!isTauri.value) return
  // 模型配置读取在所有环境下都试 —— Web 下 invoke 会失败，但失败要静默（提示已由顶部 n-alert 给出）
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('get_model_config')
    if (json && json !== 'null') {
      const raw = JSON.parse(json) as FingertipConfigWire
      // Rust 数组 → 表单「逗号字符串」
      modelConfig.value = fromWire(raw)
    }
  } catch {
    /* 没引擎不报 —— 顶部 alert 已说明环境 */
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    autostart.value = await invoke<boolean>('get_autostart')
  } catch (e) {
    console.warn('get_autostart failed:', e)
  }
})
async function onToggleAutostart(value: boolean) {
  if (!isTauri.value) return
  autostartLoading.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const ok = await invoke<boolean>('set_autostart', { enable: value })
    autostart.value = ok
  } catch (e) {
    console.warn('set_autostart failed:', e)
  } finally {
    autostartLoading.value = false
  }
}

// v0.3.1: 与后端 generation/style_presets 完全对齐（含 lo-fi）
const styleOptions = [
  { label: 'Ambient', value: 'ambient' },
  { label: 'Jazz', value: 'jazz' },
  { label: 'Cinematic', value: 'cinematic' },
  { label: 'Lo-fi', value: 'lo-fi' }
]

// v0.3 Stage 5 Batch B Task 5.4: 下载输出目录
const downloadDir = computed({
  get: () => store.downloadDir,
  set: (v: string) => { store.setDownloadDir(v) },
})

// v0.4 T14: 模型接入表单
//
// 行为契约：
//   - onMounted 拉后端 `get_model_config` → JSON.parse → 填表（前端不发 JSON.stringify）
//   - 表单层用「逗号分隔字符串」承载 Rust 的 `Vec<String>`（local_gguf）
//   - 保存时调用 `set_model_config`，传对象，错误以中文显示在 UI（「失败要大声」）
import {
  FingertipConfigDefault,
  toWire,
  fromWire,
  type FingertipConfig,
  type FingertipConfigWire,
} from '@/types/model-config'

const modelConfig = ref<FingertipConfig>(FingertipConfigDefault())
const savingModel = ref(false)
const modelSaveError = ref<string | null>(null)
const modelSaveOk = ref(false)

const modeOptions = [
  { label: '本地优先', value: 'local_first' },
  { label: '仅本地', value: 'local_only' },
  { label: '仅云端', value: 'cloud_only' },
]

// v0.4.1：保存前校验「仅云端」区块必须 base/key/model 齐全 —— 防 placeholder 陷阱
// 再静默存出 cloud_*_ok=false 的坏配置（生成时路由直接不可用）。
function validateCloudSections(cfg: FingertipConfig): string | null {
  const sections = [
    { name: 'LLM 编排', mode: cfg.llm.mode, base: cfg.llm.cloud_base, key: cfg.llm.cloud_key, model: cfg.llm.cloud_model },
    { name: '图像生成', mode: cfg.image.mode, base: cfg.image.cloud_base, key: cfg.image.cloud_key, model: cfg.image.cloud_model },
    { name: '音频生成', mode: cfg.audio.mode, base: cfg.audio.minimax_base, key: cfg.audio.minimax_key, model: cfg.audio.minimax_model },
  ]
  for (const s of sections) {
    if (s.mode !== 'cloud_only') continue
    const missing: string[] = []
    if (!s.base.trim()) missing.push('Base URL')
    if (!s.key.trim()) missing.push('Key')
    if (!s.model.trim()) missing.push('Model')
    if (missing.length) return `${s.name} 选了「仅云端」但缺 ${missing.join('、')}，无法保存`
  }
  return null
}

async function onSaveModel() {
  // 保存前校验：cloud_only 区块字段必须齐全（失败要大声，不静默存坏配置）
  const problem = validateCloudSections(modelConfig.value)
  if (problem) {
    modelSaveError.value = problem
    modelSaveOk.value = false
    return
  }
  savingModel.value = true
  modelSaveError.value = null
  modelSaveOk.value = false
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    // 表单层 → wire 形态（逗号字符串 → 数组），再 invoke
    await invoke('set_model_config', { config: toWire(modelConfig.value) })
    modelSaveOk.value = true
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    modelSaveError.value = `保存失败：${msg}`
  } finally {
    savingModel.value = false
  }
}

async function onPickDownloadDir() {
  if (!isTauri.value) return
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const dir = await open({
      directory: true,
      defaultPath: store.downloadDir || undefined,
      title: '选择下载输出目录',
    })
    if (typeof dir === 'string') {
      store.setDownloadDir(dir)
    }
  } catch (e) {
    console.warn('pick downloadDir failed:', e)
  }
}

async function onResetDownloadDir() {
  // 先清掉当前值，让 ensureDefaultDir 走「未配置 → 自动创建」分支
  store.setDownloadDir('')
  try {
    const { ensureDefaultDir } = await import('@/utils/download')
    await ensureDefaultDir()
  } catch (e) {
    console.warn('reset downloadDir failed:', e)
  }
}
</script>
