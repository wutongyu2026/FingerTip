// Artworks.vue 下载错误可见性测试（code-review 🔴1 "失败要大声"）
//
// 验证意图：下载失败 / 缺 wav 文件时，错误必须显示在 UI 上，
//   而不是只 console.warn 让用户对着没反应的按钮反复点（"还是不行"的根因）。
//
// 为什么用组件测试而非 e2e：
//   - e2e 跑 web 模式，store.generationResult 为 null + downloadWavFromPath 提前返 null，
//     根本走不到失败分支。桌面端的失败（capability 拒绝 / 文件丢失）只能在组件层用 mock 模拟。
//   - 这是 code-review 指出的"web 测不到桌面行为"陷阱的正面回应：用 mock 把失败注入进来。
//
// v0.4 契约调整：
//   - sentence 改为 store.generationResult.sentence 字符串（不再独立 invoke generate_sentence）
//   - art 用 art_png_path 渲染（<img>），不再用 canvas
//   - 模板断言：img.ft-art-canvas-el 可见 + 句子字符串渲染

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref } from 'vue'

// ---- mock 边界 ----

// downloadWavFromPath 的行为由测试逐例控制（成功 / 抛错）
const mockDownloadWav = vi.fn()
vi.mock('@/utils/download', () => ({
  downloadBlob: vi.fn(async () => '/mocked/saved.png'),
  downloadWavFromPath: (...args: unknown[]) => mockDownloadWav(...args),
}))

// plugin-fs.readFile：v0.4 onDownloadArt 改走 readFile(pngPath) → Blob
const mockReadFile = vi.fn(async (_path: string) => new Uint8Array([0x89, 0x50, 0x4e, 0x47]))
vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: (...args: unknown[]) => mockReadFile(...(args as [string])),
}))

// store：提供含 music_wav_path / art_png_path / sentence(v0.4 字符串) 的 generationResult
//   v0.4：art 不再有 width/height/pixels；music 不再有 notes；
//         新增 description/model。sentence 是字符串（不再 Sentence 对象）。
const mockStore = {
  generationResult: {
    art: { theme_word: 'x', mood: null, description: 'x', model: 'local' },
    music: {
      bpm: 0, duration_ms: 1000, amplitudes: [],
      mood: null, style: 'ambient', theme_word: 'x',
      description: 'x', model: 'local',
    },
    date: '2026-08-02',
    music_wav_path: '/appdata/downloads/2026-08-02/music.wav',
    art_png_path: '/appdata/downloads/2026-08-02/art.png',
    sentence: 'hello world',  // v0.4: 字符串
  } as Record<string, unknown> | null,
  timezoneOffsetMinutes: 480,
  downloadDir: '/mocked/dl',
}
vi.mock('@/stores/app', () => ({
  useAppStore: () => mockStore,
}))

// player：最小可用替身（load/play/stop + 进度 refs）
vi.mock('@/composables/useTonePlayback', () => ({
  useTonePlayback: () => ({
    load: vi.fn(async () => undefined),
    play: vi.fn(async () => undefined),
    stop: vi.fn(async () => undefined),
    isPlaying: ref(false),
    currentMs: ref(0),
    durationMs: ref(1000),
    _scheduledCount: () => 1,
    exportWav: vi.fn(async () => { throw new Error('deprecated') }),
  }),
}))

// Tauri core：convertFileSrc 把本地路径转 asset URL（v0.4 画作渲染需要）
//   v0.4: generate_sentence 不再独立 invoke；mock 仅返 null 即可（不会被调用）
vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: vi.fn((p: string) => `asset://${p}`),
  invoke: vi.fn(async () => 'null'),
}))

async function mountArtworks() {
  const { default: Artworks } = await import('@/views/Artworks.vue')
  const wrapper = mount(Artworks, {
    global: {
      stubs: {
        'n-empty': {
          props: ['description'],
          template: '<div class="n-empty-stub">{{ description }}</div>',
        },
      },
    },
  })
  await flushPromises()
  return wrapper
}

describe('Artworks.vue 下载失败可见性', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockStore.generationResult = {
      art: { theme_word: 'x', mood: null, description: 'x', model: 'local' },
      music: {
        bpm: 0, duration_ms: 1000, amplitudes: [],
        mood: null, style: 'ambient', theme_word: 'x',
        description: 'x', model: 'local',
      },
      date: '2026-08-02',
      music_wav_path: '/appdata/downloads/2026-08-02/music.wav',
      art_png_path: '/appdata/downloads/2026-08-02/art.png',
      sentence: 'hello world',
    }
  })

  it('下载抛错 → UI 显示失败提示（不只 console.warn）', async () => {
    mockDownloadWav.mockRejectedValueOnce(new Error('capability denied: fs:allow-read-file'))
    const wrapper = await mountArtworks()

    await wrapper.find('button[aria-label="下载音乐 wav"]').trigger('click')
    await flushPromises()

    // 关键断言：错误文案渲染到 DOM，用户看得见
    expect(wrapper.text()).toContain('下载失败')
    expect(wrapper.text()).toContain('capability denied')
  })

  it('缺 music_wav_path（旧数据）→ UI 提示先生成，不静默', async () => {
    mockStore.generationResult = {
      ...(mockStore.generationResult as object),
      music_wav_path: null,
    }
    const wrapper = await mountArtworks()

    await wrapper.find('button[aria-label="下载音乐 wav"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('尚未生成')
    expect(mockDownloadWav).not.toHaveBeenCalled()
  })

  it('下载成功 → 不显示任何错误', async () => {
    mockDownloadWav.mockResolvedValueOnce('/mocked/dl/chosen-music.wav')
    const wrapper = await mountArtworks()

    await wrapper.find('button[aria-label="下载音乐 wav"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).not.toContain('下载失败')
    expect(mockDownloadWav).toHaveBeenCalledWith(
      '/appdata/downloads/2026-08-02/music.wav',
      expect.stringContaining('music'),
    )
  })

  // v0.4: onDownloadArt 走 art_png_path → readFile → Blob → downloadBlob（fix P1）
  // 不再走 canvas.toBlob（drawCanvas 已删）。错误路径同 onDownloadMusic 模式。
  it('画作下载成功 → readFile 调一次、downloadBlob 收到 image/png', async () => {
    mockReadFile.mockResolvedValueOnce(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))
    const wrapper = await mountArtworks()

    await wrapper.find('button[aria-label="下载画作 png"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).not.toContain('下载失败')
    expect(mockReadFile).toHaveBeenCalledWith('/appdata/downloads/2026-08-02/art.png')
  })

  it('画作缺 art_png_path（旧数据）→ UI 提示先生成，不静默', async () => {
    mockStore.generationResult = {
      ...(mockStore.generationResult as object),
      art_png_path: null,
    }
    const wrapper = await mountArtworks()

    await wrapper.find('button[aria-label="下载画作 png"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('尚未生成')
    expect(mockReadFile).not.toHaveBeenCalled()
  })

  it('画作 readFile 抛错 → UI 显示失败提示（不只 console.warn）', async () => {
    mockReadFile.mockRejectedValueOnce(new Error('capability denied: fs:allow-read-file'))
    const wrapper = await mountArtworks()

    await wrapper.find('button[aria-label="下载画作 png"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('下载失败')
    expect(wrapper.text()).toContain('capability denied')
  })

  // ---- v0.4 新增：画作渲染 + 句子字符串 ----

  it('v0.4: 画作渲染为 <img>，src 来自 convertFileSrc(art_png_path)', async () => {
    const wrapper = await mountArtworks()

    const img = wrapper.find('img.ft-art-canvas-el')
    expect(img.exists()).toBe(true)
    expect(img.attributes('src')).toBe('asset:///appdata/downloads/2026-08-02/art.png')
    expect(img.attributes('alt')).toBe('今日画作')
  })

  it('v0.4: 句子面板渲染 generationResult.sentence 字符串（不是 Sentence 对象）', async () => {
    const wrapper = await mountArtworks()

    // 句子面板存在
    expect(wrapper.find('.ft-sentence-text').exists()).toBe(true)
    expect(wrapper.text()).toContain('hello world')
  })

  it('v0.4: 缺 art_png_path 时显示空态，不渲染 <img>', async () => {
    mockStore.generationResult = {
      ...(mockStore.generationResult as object),
      art_png_path: null,
    }
    const wrapper = await mountArtworks()

    expect(wrapper.find('img.ft-art-canvas-el').exists()).toBe(false)
    // 空态：n-empty stub 渲染 description
    expect(wrapper.text()).toContain('尚未生成今日画作')
  })

  it('v0.4: 缺 sentence 时句子面板不显示', async () => {
    mockStore.generationResult = {
      ...(mockStore.generationResult as object),
      sentence: null,
    }
    const wrapper = await mountArtworks()

    expect(wrapper.find('.ft-sentence-section').exists()).toBe(false)
  })
})
