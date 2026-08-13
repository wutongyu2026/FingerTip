import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

// 前端测试配置（独立于 vite.config.ts）
// 验证意图：单元/组件测试在 jsdom 环境跑，与 Vite dev/build 解耦
// 排除 tests-e2e/（那是 Playwright 的测试，不归 vitest 管）
// 排除 .claude/** 防止 worktree 内的依赖内部 *.test.ts（如 Tone.js 自带 ~100 个）
//   被 vitest 误当成项目测试扫描，导致 429+ 测试文件失败
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) }
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test-setup.ts'],
    exclude: [
      '**/node_modules/**',
      '**/dist/**',
      'tests-e2e/**',
      'src-tauri/**',
      '.claude/**',
    ]
  }
})
