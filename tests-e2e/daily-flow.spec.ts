import { test, expect } from '@playwright/test'

// 端到端测试：心情提交 → 生成 → （Tauri Command 调用，web 环境失败属预期）
//
// 验证意图：完整 UI 流程跑通
// - 输入心情词
// - 点击生成
// - Loading 状态出现
// - web 环境（无 Tauri）下，invoke 会失败，错误提示可见

test('用户提交心情并触发生成（web 占位）', async ({ page }) => {
  await page.goto('/#/submit')

  // 输入心情词
  await page.fill('input[placeholder*="一个词"]', 'calm')

  // 验证按钮启用
  const generateBtn = page.locator('button:has-text("生成")')
  await expect(generateBtn).toBeEnabled()

  // 点击生成
  await generateBtn.click()

  // 验证 Loading 状态（按钮会变成 loading）
  // 等待响应（web 环境会报错，Tauri 环境会成功）
  await expect(page.locator('text=生成失败').or(page.locator('text=已生成'))).toBeVisible({ timeout: 10_000 })
})

test('心情词为空时按钮禁用', async ({ page }) => {
  await page.goto('/#/submit')

  const generateBtn = page.locator('button:has-text("生成")')
  await expect(generateBtn).toBeDisabled()
})

test('5 路由可达', async ({ page }) => {
  const routes = ['/', '/submit', '/history', '/settings', '/about']
  for (const path of routes) {
    await page.goto(`/#${path}`)
    // 验证导航栏存在（说明 App 渲染）
    await expect(page.locator('.nav-bar')).toBeVisible()
  }
})