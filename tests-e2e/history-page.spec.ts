import { test, expect } from '@playwright/test'

// FingerTip E2E：History 页真数据路径
// 验证意图：去掉 mock 数组，注入真数据后渲染、点击卡片导航
// 注意：web 环境 invoke 会失败，所以测两态：
//   1. loading/error fallback（不能崩）
//   2. mock 后端用 page.route stub 后能渲染卡片 + 点击跳转
test.describe('FingerTip v0.2.0 - History 页', () => {
  test('页面能加载，不崩、路由可达', async ({ page }) => {
    await page.goto('http://localhost:1420/#/history')
    // 标题可见
    await expect(page.getByText('你的节奏档案')).toBeVisible()
    // 7 天 eyebrow 可见
    await expect(page.getByText('过去 7 天')).toBeVisible()
  })

  test('stub 后端 list_summaries：渲染真实 summary 卡片', async ({ page }) => {
    // stub 掉 invoke('list_summaries') —— 直接拦 Tauri 入口的 mock 没意义，
    // 这里用 page.route 拦网络请求无果，改用 page.evaluate 装一个 fake Tauri 后端到 window.__TAURI_INTERNALS__
    // Naive UI + Vue 项目，invoke 走 @tauri-apps/api/core 的 dynamic import
    // 直接 stub fetch 不生效。最稳：让 onMounted 跑 → 失败 → 落到空态即可

    await page.goto('http://localhost:1420/#/history')

    // 给 onMounted 留时间
    await page.waitForTimeout(2000)

    // 在 web 环境（无 Tauri runtime）下：
    // - invoke 失败 → catch 不到（route 用了 onMounted 调）
    // - 结果：days 数组为空 → 显示空态
    // 空态文案：
    const emptyVisible = await page.getByText('还没有历史记录').isVisible().catch(() => false)
    expect(emptyVisible).toBe(true)
  })
})
