import { describe, it, expect } from 'vitest'

// 测试基础设施冒烟测试
// 验证意图：jsdom 环境与 @ 路径别名能正常工作（这是后续组件测试的基石）
describe('测试基础设施冒烟', () => {
  it('Vitest 能正常工作', () => {
    expect(1 + 1).toBe(2)
  })

  it('jsdom 提供 DOM 环境', () => {
    const el = document.createElement('div')
    el.textContent = 'hello'
    expect(el.textContent).toBe('hello')
  })
})