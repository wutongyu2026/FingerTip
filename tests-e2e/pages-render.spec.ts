import { test, expect } from '@playwright/test'

// FingerTip E2E：所有页面能正常渲染、无 console error、版本号显示
// 验证意图：UI 端口 1420 + 6 个视图可访问 + 版本号 vX.Y.Z 形式就位
test.describe('FingerTip - 页面可达性', () => {
  const consoleErrors: string[] = []

  test.beforeEach(async ({ page }) => {
    consoleErrors.length = 0
    page.on('console', (msg) => {
      if (msg.type() === 'error') consoleErrors.push(msg.text())
    })
    page.on('pageerror', (err) => consoleErrors.push(err.message))
  })

  test('所有 6 个页面都能渲染（无 JS error）', async ({ page }) => {
    const routes = ['/', '/#/artworks', '/#/submit', '/#/history', '/#/settings', '/#/about']
    for (const route of routes) {
      await page.goto(`http://localhost:1420${route}`)
      // 路由切换不报错
      expect(consoleErrors, `route ${route} 触发 console error`).toEqual([])
      // 页面有 title
      const title = await page.title()
      expect(title).toBe('FingerTip')
    }
  })

  test('About 页显示版本号 vX.Y.Z 形式', async ({ page }) => {
    await page.goto('http://localhost:1420/#/about')
    // v0.4.2 美化：版本号在独立徽章里（手写标题 + 徽章 vX.Y.Z），用类选择器断言
    await expect(page.locator('.ft-about-version')).toHaveText(/v\d+\.\d+\.\d+/)
  })
})
