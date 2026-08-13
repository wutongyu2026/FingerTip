import { test, expect } from '@playwright/test'

/**
 * v0.3.6 R1 首活时间 — UI 结构 E2E
 *
 * 验证意图：Today.vue hero 显示"首活时间"标签
 * web 环境 invoke 抛错 → firstActiveDisplay 返空 → 显示"—"
 */
test.describe('FingerTip v0.3.6 — R1 首活时间 UI', () => {
  test('Today 页 hero 显示"首活时间"标签', async ({ page }) => {
    await page.goto('http://localhost:1420/#/')
    await expect(page.getByText('首活时间')).toBeVisible()
  })
})
