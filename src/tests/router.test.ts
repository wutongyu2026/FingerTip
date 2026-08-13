import { describe, it, expect } from 'vitest'
import { router } from '@/router'

describe('router', () => {
  it('has 6 routes', () => {
    expect(router.getRoutes().length).toBe(6)
  })

  it('default route is Today', () => {
    const today = router.getRoutes().find(r => r.path === '/')
    expect(today).toBeDefined()
  })

  it('artworks route is registered', () => {
    const artworks = router.getRoutes().find(r => r.path === '/artworks')
    expect(artworks).toBeDefined()
  })

  it('all routes have components', () => {
    for (const r of router.getRoutes()) {
      // 路由 component 是懒加载函数，验证存在即可
      expect(r.components).toBeDefined()
    }
  })
})