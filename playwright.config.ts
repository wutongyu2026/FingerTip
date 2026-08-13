import { defineConfig } from '@playwright/test'

// Playwright E2E 配置
// 注意：Web 环境跑不通 Tauri Command invoke；Tauri 完整 E2E 需 Tauri Driver
// 此配置仅用于验证前端 UI 流程 + 占位错误处理
export default defineConfig({
  testDir: './tests-e2e',
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000
  },
  use: {
    baseURL: 'http://localhost:1420'
  },
  timeout: 30_000
})