// Settings.vue 模型接入区块测试（v0.4 T14）
//
// 验证意图（不是「按钮存在不存在」而是「Settings 真的能把模型配置交给后端」）：
//   1. 区块标签可见：模型接入 / LLM / 图像 / MiniMax —— 用户能凭 UI 找到入口
//   2. 默认值填入 input（默认值与 Rust FingertipConfig::default 字段一一对应）
//   3. 点「保存模型配置」真的 invoke('set_model_config', { config: ... }) ——
//      IPC 路径断掉会让用户以为「保存成功」实则没生效（静默丢）
//   4. 后端报错时显示「保存失败：...」中文诊断（失败要大声）
//
// 设计决策：
//   - **不**注入 `window.__TAURI_INTERNALS__`：Tauri 2 真实 SDK 会调
//     `__TAURI_INTERNALS__.invoke`（不经 mock），需要把 mock 装在那个对象上才能
//     让「onMounted 走读路径」生效；为了测试稳定，让 Settings 走 web 分支
//     （isTauri=false → onMounted 早返），默认值与表单渲染由 setup() 直接断言，
//     save 路径独立测（onSaveModel 不受 isTauri gate）
//   - n-* 组件用带 `template` 的 stub（不布尔 stub），让 wrapper.findAll('button')
//     能拿到真实 <button> 元素 —— 避免「测试假过」（r2-minor-followups 教训）

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { h } from 'vue'

// ---- mock 边界 ----

// 让 mock 实现始终存在一个明确的 `set_model_config` 行为开关；
//   测试通过修改 `setModelConfigBehavior` 切换成功/失败，不依赖 `mockImplementationOnce`
//   避免 vitest 2.1.x 在第二次动态 import 时把 mock module 当作空对象重解析的怪行为
type SetModelConfigBehavior = 'ok' | 'fail'
let setModelConfigBehavior: SetModelConfigBehavior = 'ok'

const mockInvoke = vi.fn(async (cmd: string, _args?: Record<string, unknown>) => {
  if (cmd === 'get_model_config') {
    return JSON.stringify({
      engine: { enabled: true, base_url: 'http://127.0.0.1:8765' },
      llm: {
        mode: 'local_first',
        local_gguf: ['/a.gguf', '/b.gguf'],
        cloud_base: '',
        cloud_key: '',
        cloud_model: '',
      },
      image: {
        mode: 'local_first',
        local_model_path: '',
        cloud_base: '',
        cloud_key: '',
        cloud_model: '',
      },
      audio: {
        mode: 'local_first',
        minimax_base: '',
        minimax_key: '',
        minimax_model: '',
      },
    })
  }
  if (cmd === 'get_autostart') return false
  if (cmd === 'set_model_config') {
    if (setModelConfigBehavior === 'fail') throw new Error('disk full')
    return null
  }
  return null
})
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...(args as [string, Record<string, unknown>?])),
  convertFileSrc: vi.fn((p: string) => `asset://${p}`),
}))

async function mountSettings() {
  const { default: Settings } = await import('@/views/Settings.vue')
  const wrapper = mount(Settings, {
    global: {
      stubs: {
        'n-card': { props: ['title'], template: '<div><slot /></div>' },
        'n-space': { template: '<div><slot /></div>' },
        'n-divider': { template: '<hr />' },
        'n-text': { props: ['type', 'depth'], template: '<span><slot /></span>' },
        'n-alert': { template: '<div><slot /></div>' },
        // 表单元素：渲染为真实 button/input/select —— findAll('button') 才能匹配。
//   stub 内部用 render 函数手动渲染 button，**不**让 Vue 把父级 @click 自动
//   转发到 button DOM（那会让 click 事件触发两次 onSaveModel）：
        'n-button': {
          props: ['loading', 'type', 'quaternary', 'size'],
          inheritAttrs: false,
          render() {
            // h() 第三个参数里直接挂 props.onClick 作为 DOM onclick 属性：
            //   trigger('click') → DOM 原生 click → onClick handler 只跑一次
            const onClick = (this as { $: { onClick?: unknown }; $attrs: { onClick?: (e: Event) => void } }).$attrs.onClick
            return h(
              'button',
              { disabled: this.loading, onClick },
              this.$slots.default?.(),
            )
          },
        },
        'n-input': {
          props: ['value', 'type', 'placeholder', 'readonly', 'showPasswordOn'],
          template:
            '<input :value="value" :type="type === \'password\' ? \'password\' : \'text\'" :placeholder="placeholder" :readonly="readonly" @input="$emit(\'update:value\', $event.target.value)" />',
        },
        'n-switch': {
          props: ['value', 'loading', 'disabled'],
          template:
            '<button type="button" :disabled="loading || disabled" @click="$emit(\'update:value\', !value)">{{ value ? \'on\' : \'off\' }}</button>',
        },
        'n-select': {
          props: ['value', 'options'],
          template:
            '<select :value="value" @change="$emit(\'update:value\', $event.target.value)"><option v-for="o in options" :key="o.value" :value="o.value">{{ o.label }}</option></select>',
        },
      },
    },
  })
  await flushPromises()
  return wrapper
}

describe('Settings.vue 模型接入区块', () => {
  beforeEach(() => {
    // 关键：不要 mockClear（vitest 2.1.x 在某些情况下会重置 vi.mock 模块缓存，
    //   导致后续动态 import 拿到空对象）。手动控制行为开关即可：
    setModelConfigBehavior = 'ok'
    setActivePinia(createPinia())
  })

  it('渲染「模型接入」section + 关键标签 + 默认 base_url', async () => {
    const wrapper = await mountSettings()
    const text = wrapper.text()
    expect(text).toContain('模型接入')
    expect(text).toContain('LLM')
    expect(text).toContain('图像')
    expect(text).toContain('MiniMax')
    // 默认值填入 input（与 Rust FingertipConfig::default 对齐）
    const urlInput = wrapper.find('input[placeholder="http://127.0.0.1:8765"]')
    expect((urlInput.element as HTMLInputElement).value).toBe('http://127.0.0.1:8765')
  })

  it('点「保存模型配置」真的调 set_model_config + 错误路径文案渲染', async () => {
    const wrapper = await mountSettings()
    const saveBtn = wrapper.findAll('button').find((b) => b.text().includes('保存模型配置'))
    expect(saveBtn, '保存按钮应渲染').toBeDefined()

    // 成功路径
    await saveBtn!.trigger('click')
    await flushPromises()
    expect(mockInvoke).toHaveBeenCalledWith(
      'set_model_config',
      expect.objectContaining({ config: expect.any(Object) }),
    )
    expect(wrapper.text()).toContain('已保存')

    // 错误路径：切换行为开关 → mock 抛 disk full
    setModelConfigBehavior = 'fail'
    await saveBtn!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('保存失败：')
    expect(wrapper.text()).toContain('disk full')
  })
})