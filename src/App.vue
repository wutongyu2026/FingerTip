<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { NConfigProvider, NMessageProvider, NSpace, NButton } from "naive-ui";
import type { GlobalThemeOverrides } from "naive-ui";
import { RouterView, useRouter, useRoute } from "vue-router";
import { useAppStore } from '@/stores/app';

const router = useRouter();
const route = useRoute();
const nav = (path: string) => router.push(path);
const isActive = (path: string) => route.path === path;

const navItems = [
  { path: '/', label: '今日' },
  { path: '/artworks', label: '作品' },
  { path: '/submit', label: '心情' },
  { path: '/history', label: '历史' },
  { path: '/settings', label: '设置' },
  { path: '/about', label: '关于' }
]

// v0.4.2 美化：Naive UI 主题覆盖 —— 主色统一为设计系统的暖橙（#D67B4F），
// 让激活导航 / primary 按钮不再突兀地显示 Naive 默认蓝。
const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#D67B4F',
    primaryColorHover: '#E08A5E',
    primaryColorPressed: '#C46C42',
    primaryColorSuppl: '#D67B4F',
  },
}

// v0.5.0: 键盘 Hook 状态条 —— 启动时 invoke get_hook_status 显示绿/灰点。
// null = 加载中（web 模式 / invoke 失败），true = Hook 已启动，false = Hook 启动失败。
const hookRunning = ref<boolean | null>(null)

const store = useAppStore();
let unlistenNavigate: (() => void) | null = null

onMounted(async () => {
  store.loadDownloadDir();
  try {
    const { ensureDefaultDir } = await import('@/utils/download');
    await ensureDefaultDir();
  } catch (e) {
    // web 模式 / 单元测试 jsdom 下 ensureDefaultDir 内部已静默 fallback；
    // 此处 catch 是双保险 —— 不让启动流程被目录创建失败打断
    console.warn('[App] ensureDefaultDir failed:', e);
  }

  // v0.5.0: 拉一次 hook 状态 —— web 模式 invoke 会失败，保持 null（不显示状态条）
  if (typeof window === 'undefined') return
  const w = window as any
  if (!('__TAURI_INTERNALS__' in w)) return  // web fallback —— 状态条不渲染
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    hookRunning.value = await invoke<boolean>('get_hook_status')
  } catch (e) {
    console.warn('[App] get_hook_status failed:', e)
  }

  // v0.3.3: 监听后端 tray 菜单的 'navigate' 事件，跳到对应路由
  // 只有 Tauri 运行时才有 listen API；web 模式 / 测试环境静默跳过
  try {
    const { listen } = await import('@tauri-apps/api/event')
    unlistenNavigate = await listen<string>('navigate', (event) => {
      const path = event.payload
      if (typeof path === 'string' && path.startsWith('/')) {
        router.push(path)
      }
    })
  } catch (e) {
    console.warn('[App] listen navigate failed:', e)
  }
})

onUnmounted(() => {
  if (unlistenNavigate) {
    unlistenNavigate()
    unlistenNavigate = null
  }
})
</script>

<template>
  <n-config-provider :theme-overrides="themeOverrides">
    <n-message-provider>
      <div class="app-shell">
        <n-space class="nav-bar" :wrap-item="false" align="center" justify="space-between">
          <n-space :wrap-item="false">
            <n-button
              v-for="item in navItems"
              :key="item.path"
              quaternary
              :type="isActive(item.path) ? 'primary' : 'default'"
              @click="nav(item.path)"
            >
              {{ item.label }}
            </n-button>
          </n-space>
          <!-- v0.5.0: Hook 状态条 —— 仅 Tauri 模式渲染，绿点=已启动，灰点=失败 -->
          <div v-if="hookRunning !== null" class="ft-hook-status" :class="{ 'ft-hook-status--ok': hookRunning, 'ft-hook-status--fail': !hookRunning }">
            <span class="ft-hook-dot" aria-hidden="true"></span>
            <span class="ft-hook-label">{{ hookRunning ? 'Hook 已启动' : 'Hook 未启动' }}</span>
          </div>
        </n-space>
        <main class="content">
          <RouterView />
        </main>
      </div>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  background: #faf8f4; /* 与 tokens.css --bg-base 对齐（避免宽屏两侧灰与暖白不一致） */
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
    "Microsoft YaHei", sans-serif;
}
.app-shell {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  padding: 12px 20px;
  /* v0.4.2 美化：宽屏下内容居中（Tauri 窗口 1100 时近似满宽，无感知） */
  max-width: 1160px;
  margin: 0 auto;
  width: 100%;
  box-sizing: border-box;
  /* 不限制高度 + 不 overflow —— 让 webview 自己管理滚动（单一滚动条） */
}
.nav-bar {
  margin-bottom: 8px;
  flex-shrink: 0;
}
.content {
  flex: 1;
  /* 不再有 overflow-y —— 滚动由 webview 自己处理（外层滚动条），
     避免内嵌 + 外层两个滚动条共存的混乱。 */
}

/* v0.5.0: Hook 状态条 —— 右上角小胶囊，绿/灰点 + 文字 */
.ft-hook-status {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2, 8px);
  padding: 4px 12px;
  border-radius: 100px;
  font-size: 12px;
  font-weight: 500;
  border: 1px solid var(--border-default, rgba(42,40,35,0.1));
  background: var(--bg-surface, #ffffff);
}
.ft-hook-status--ok { color: #4C7A4A; border-color: rgba(76,122,74,0.35); background: rgba(76,122,74,0.06); }
.ft-hook-status--fail { color: var(--text-tertiary, #A39C92); }
.ft-hook-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
  box-shadow: 0 0 0 3px color-mix(in srgb, currentColor 18%, transparent);
}
.ft-hook-label { letter-spacing: 0.02em; }

/* webview 滚动条样式（统一一处 + 极简） */
html::-webkit-scrollbar { width: 8px; }
html::-webkit-scrollbar-track { background: transparent; }
html::-webkit-scrollbar-thumb {
  background: var(--border-default);
  border-radius: 4px;
}
html::-webkit-scrollbar-thumb:hover { background: var(--text-tertiary); }
</style>