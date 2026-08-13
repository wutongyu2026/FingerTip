import { test, expect } from '@playwright/test'

/**
 * v0.3.9 R6 — 二维码 UI 结构 E2E
 *
 * web 环境 store.generationResult 为 null → onGenerateQr 直接返错误
 * 但 UI 按钮 + 标题应可见
 */
test.describe('FingerTip v0.3.9 — R6 二维码 UI', () => {
  test('Artworks 页"分享今日作品"按钮可见', async ({ page }) => {
    await page.goto('http://localhost:1420/#/artworks')
    await expect(page.getByText('分享今日作品')).toBeVisible()
    await expect(page.getByRole('button', { name: /生成卡片图片|生成中|重新生成/ })).toBeVisible()
  })
})
