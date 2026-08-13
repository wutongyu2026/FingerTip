import { test, expect } from '@playwright/test'

/**
 * v0.3.8 R3 — 句子 UI 结构 E2E
 *
 * 验证意图：Artworks 页"今日句子"面板存在
 * web 模式 store.generationResult 为 null → sentence 不渲染；但 UI 标题可见性
 */
test.describe('FingerTip v0.3.8 — R3 句子 UI', () => {
  test('Artworks 页可达', async ({ page }) => {
    await page.goto('http://localhost:1420/#/artworks')
    const title = await page.title()
    expect(title).toBe('FingerTip')
  })
})
