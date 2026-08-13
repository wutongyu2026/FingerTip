import { test, expect } from '@playwright/test'

/**
 * v0.3.7 R5 — 本地 WAV 播放 UI 结构 E2E
 *
 * 验证意图：Artworks 页音乐区在无数据时是「诚实空态」而非假的播放控件。
 * v0.4.2：移除 mock 数据后，播放器只在 store.generationResult.music 存在时渲染
 * （web 模式无数据 → 空态）。播放器控件本体由单测 Artworks.download.spec.ts 覆盖。
 */
test.describe('FingerTip v0.3.7 — R5 本地播放 UI', () => {
  test('Artworks 页无音乐数据时显示空态', async ({ page }) => {
    await page.goto('http://localhost:1420/#/artworks')
    // web 模式 generationResult 为 null → 应显示空态提示，而不是渲染带假标题的播放器
    await expect(page.getByText('尚未生成今日音乐')).toBeVisible()
    // 播放按钮不应出现（无产物时暴露播放控件是假象）
    await expect(page.getByRole('button', { name: /播放|停止/ })).toHaveCount(0)
  })
})
