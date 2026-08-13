import { createRouter, createWebHashHistory } from 'vue-router'

// Vue Router（hash mode 用于 Tauri 桌面）
// 验证意图：路由与 UI 解耦，6 个页面可独立开发测试
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', component: () => import('@/views/Today.vue') },
    { path: '/artworks', component: () => import('@/views/Artworks.vue') },
    { path: '/history', component: () => import('@/views/History.vue') },
    { path: '/settings', component: () => import('@/views/Settings.vue') },
    { path: '/about', component: () => import('@/views/About.vue') },
    { path: '/submit', component: () => import('@/views/SubmitMood.vue') }
  ]
})