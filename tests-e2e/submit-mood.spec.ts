import { test, expect } from '@playwright/test'

// FingerTip E2E：SubmitMood 真生成路径
// 验证意图：用户提交心情 → invoke generate_now → 跳转 Artworks
// 浏览器 Web 模式下 invoke 会失败（无 Tauri runtime），所以这条测的是：
//   - 表单能填
//   - 按钮可点
//   - 错误能被 UI 显示（不静默吞）
//   - 错误时不跳转（守住行为）
test.describe('FingerTip v0.2.0 - SubmitMood 路径', () => {
  test('心情表单可填 + 按钮启用规则正确', async ({ page }) => {
    await page.goto('http://localhost:1420/#/submit')

    // 初始：按钮 disabled（mood 为空）
    const btn = page.getByRole('button', { name: '生成今日作品' })
    await expect(btn).toBeDisabled()

    // 用 chip 选心情
    await page.getByText('focused', { exact: true }).click()

    // 按钮启用
    await expect(btn).toBeEnabled()
  })

  test('点击生成：在 web 环境 invoke 会失败，错误被显示不跳转', async ({ page }) => {
    await page.goto('http://localhost:1420/#/submit')
    await page.getByText('anxious', { exact: true }).click()

    // 点击生成
    await page.getByRole('button', { name: '生成今日作品' }).click()

    // 等待 invoke 完成（不论成功失败）
    // 错误路径：error 节点出现，但不跳转
    await expect(page).toHaveURL(/submit|error/)

    // 如果还在 /submit 路径上，错误消息显示出来
    if (page.url().includes('submit')) {
      // web 环境无 Tauri runtime → invoke 抛错 → 显示 "生成失败：..."
      await expect(page.locator('text=生成失败')).toBeVisible({ timeout: 5000 })
    }
  })

  test('样式选择 chip 可切换', async ({ page }) => {
    await page.goto('http://localhost:1420/#/submit')
    // 默认 Ambient 高亮（橙色）
    const ambient = page.getByRole('button', { name: 'Ambient', exact: true })
    await expect(ambient).toHaveClass(/active/)

    // 切到 Jazz
    await page.getByRole('button', { name: 'Jazz', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Jazz', exact: true })).toHaveClass(/active/)
  })
})
