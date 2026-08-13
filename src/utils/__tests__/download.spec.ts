// download.ts 单测 —— 覆盖 v0.3 Stage 5 Task 5.7 E2E-C
//
// 验证意图（不是测"调用了 download"）：
// - 为什么需要 plugin-dialog + plugin-fs：浏览器 <a download> 无法控制保存路径，
//   用户的下载作品必须落到他在 Settings 选的目录里，而不是浏览器默认 Downloads。
// - 为什么 store.downloadDir 是 single source of truth：Settings 页改一次，
//   Artworks 页 / 任何后续下载入口都自动跟上。
// - 为什么 downloadBlob 在 !isTauri 时直接返回 null：vitest jsdom 没有 __TAURI_INTERNALS__，
//   单测默认走非 Tauri 分支（拼参数即可测，不需要写盘）。
//
// Tauri 路径 + 默认目录的完整 e2e（Save As → writeFile → 写盘）走 playwright-cli
// 真 NSIS 包装后手测；单测只验证参数拼装、cancelled 返回 null、ensureDefaultDir 创建目录。

import { describe, it, expect, vi, beforeEach } from 'vitest'

// ---- mock Tauri 插件 ----
//
// 注意：download.ts 用 `__TAURI_INTERNALS__?.transformCallback` 探测是否真在 Tauri 运行时。
// jsdom 不会注入这个对象，所以默认走「非 Tauri」分支 → 不会真调 save / writeFile。
// 想测 Tauri 分支必须先 mock __TAURI_INTERNALS__。

const mockSave = vi.fn(async (opts: { defaultPath?: string }): Promise<string | null> => {
  // 模拟用户在 Save As 对话框选了路径：默认文件名替换成 "chosen-{name}"
  const def = opts.defaultPath ?? 'unnamed'
  const parts = def.split(/[/\\]/)
  const filename = parts.pop() ?? 'unnamed'
  const dir = parts.join('/') || '/tmp'
  return `${dir}/chosen-${filename}`
})

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: (...args: unknown[]) => mockSave(...(args as [object])),
  open: vi.fn(async () => '/mocked/picked-dir'),
}))

const mockWriteFile = vi.fn(async (_path: string, _bytes: Uint8Array) => undefined)
const mockReadFile = vi.fn(async (_path: string): Promise<Uint8Array> => {
  // 默认返 4 字节 RIFF 头，模拟 wav 文件
  return new Uint8Array([0x52, 0x49, 0x46, 0x46])
})

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeFile: (...args: unknown[]) => mockWriteFile(...(args as [string, Uint8Array])),
  exists: vi.fn(async () => true),
  mkdir: vi.fn(async () => undefined),
  readFile: (...args: unknown[]) => mockReadFile(...(args as [string])),
}))

vi.mock('@tauri-apps/api/path', () => ({
  appDataDir: vi.fn(async () => '/mocked/appdata'),
  join: vi.fn(async (...parts: string[]) => parts.join('/')),
}))

// ---- mock store ----
//
// store 不需要真 pinia：download.ts 只读 downloadDir + 调 setDownloadDir。
// 用 plain object + vi.fn 占位就行。

const mockStore = {
  downloadDir: '/mocked/initial' as string,
  setDownloadDir: vi.fn((d: string) => {
    mockStore.downloadDir = d
  }),
  loadDownloadDir: vi.fn(),
}

vi.mock('@/stores/app', () => ({
  useAppStore: () => mockStore,
}))

// ---- mock Tauri runtime 探测 ----
//
// download.ts 通过 `window.__TAURI_INTERNALS__?.transformCallback` 判断是否 Tauri。
// 用 defineProperty 把 transformCallback 挂上去 → 模拟 Tauri 运行时。

function fakeTauriRuntime() {
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    value: { transformCallback: vi.fn(() => 1) },
  })
}

function clearTauriRuntime() {
  // 还原：直接把属性删掉，模拟 web / jsdom 环境
  try {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
  } catch {
    /* readonly window props 极少数情况会抛，忽略 */
  }
}

// jsdom Blob polyfill 缺 arrayBuffer()，downloadBlob 调用前补上。
// 验证意图：vitest jsdom 不能假设浏览器最新 Blob API；下载工具用了 arrayBuffer 就要补。
if (typeof Blob.prototype.arrayBuffer !== 'function') {
  Blob.prototype.arrayBuffer = function (): Promise<ArrayBuffer> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => resolve(reader.result as ArrayBuffer)
      reader.onerror = () => reject(reader.error)
      reader.readAsArrayBuffer(this)
    })
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  mockStore.downloadDir = '/mocked/initial'
  clearTauriRuntime()
})

// ---------------------------------------------------------------------------
// 1. 非 Tauri 环境（web / jsdom）：downloadBlob 直接返回 null，不调 save
// ---------------------------------------------------------------------------

describe('downloadBlob (web fallback)', () => {
  it('非 Tauri 运行时直接返回 null，不弹 Save As', async () => {
    // 此时 window 上没有 __TAURI_INTERNALS__ → isTauriEnv() === false
    const { downloadBlob } = await import('@/utils/download')
    const blob = new Blob([new Uint8Array([0x89, 0x50, 0x4e, 0x47])])
    const result = await downloadBlob('test.png', blob, 'png', 'image/png')
    expect(result).toBeNull()
    expect(mockSave).not.toHaveBeenCalled()
    expect(mockWriteFile).not.toHaveBeenCalled()
  })

  it('空 downloadDir + web 环境同样返回 null（不抛错）', async () => {
    mockStore.downloadDir = ''
    const { downloadBlob } = await import('@/utils/download')
    const result = await downloadBlob('test.wav', new Blob(), 'wav', 'audio/wav')
    expect(result).toBeNull()
  })
})

// ---------------------------------------------------------------------------
// 2. Tauri 环境：downloadBlob 拼 defaultPath、调 save、写文件
// ---------------------------------------------------------------------------

describe('downloadBlob (Tauri runtime)', () => {
  beforeEach(() => {
    fakeTauriRuntime()
  })

  it('用 store.downloadDir 拼 defaultPath 传给 save()', async () => {
    mockStore.downloadDir = '/mocked/initial'
    const { downloadBlob } = await import('@/utils/download')
    const blob = new Blob([new Uint8Array([0x89, 0x50, 0x4e, 0x47])])
    const filename = 'FingerTip-art-20260725-153000.png'
    const result = await downloadBlob(filename, blob, 'png', 'image/png')

    // 用户取消时返回 null；非取消返回 mockSave 给的路径
    expect(result).toMatch(/chosen-FingerTip-art-20260725-153000\.png$/)

    // save 收到的 defaultPath 必须含用户配置的目录 + 原始 filename
    expect(mockSave).toHaveBeenCalledTimes(1)
    expect(mockSave).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: `/mocked/initial/${filename}`,
        filters: expect.arrayContaining([
          expect.objectContaining({ extensions: expect.arrayContaining(['png']) }),
        ]),
      }),
    )
  })

  it('用户取消 Save As → 返回 null，不写盘', async () => {
    mockSave.mockResolvedValueOnce(null)
    const { downloadBlob } = await import('@/utils/download')
    const blob = new Blob([new Uint8Array([1, 2, 3, 4])])
    const result = await downloadBlob('cancel.wav', blob, 'wav', 'audio/wav')
    expect(result).toBeNull()
    expect(mockWriteFile).not.toHaveBeenCalled()
  })

  it('writeFile 收到 save 返回的路径 + Uint8Array bytes', async () => {
    mockSave.mockResolvedValueOnce('/mocked/initial/chosen-output.png')
    const { downloadBlob } = await import('@/utils/download')
    const payload = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a])
    const blob = new Blob([payload])
    await downloadBlob('output.png', blob, 'png', 'image/png')

    expect(mockWriteFile).toHaveBeenCalledTimes(1)
    const [calledPath, calledBytes] = mockWriteFile.mock.calls[0] as [string, Uint8Array]
    expect(calledPath).toBe('/mocked/initial/chosen-output.png')
    expect(calledBytes).toBeInstanceOf(Uint8Array)
    expect(Array.from(calledBytes)).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a])
  })

  it('store.downloadDir 为空时 defaultPath 只含 filename（不报路径错误）', async () => {
    mockStore.downloadDir = ''
    const { downloadBlob } = await import('@/utils/download')
    const blob = new Blob([new Uint8Array([1])])
    await downloadBlob('nopath.wav', blob, 'wav', 'audio/wav')
    expect(mockSave).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: 'nopath.wav' }),
    )
  })
})

// ---------------------------------------------------------------------------
// 3. ensureDefaultDir —— 启动时自动建 %APPDATA%/downloads
// ---------------------------------------------------------------------------

describe('ensureDefaultDir', () => {
  it('web 环境 + store 未配置 → 返回空串，不调 mkdir / appDataDir', async () => {
    // 默认：clearTauriRuntime + mockStore.downloadDir = '/mocked/initial'
    // 先清掉 → 走 "未配置" 分支 → 但 !isTauri → 直接返回 ''
    mockStore.downloadDir = ''
    const { ensureDefaultDir } = await import('@/utils/download')
    const result = await ensureDefaultDir()
    expect(result).toBe('')
  })

  it('Tauri 环境 + store 未配置 → 创建 %APPDATA%/downloads 并写入 store', async () => {
    fakeTauriRuntime()
    mockStore.downloadDir = ''
    const { ensureDefaultDir } = await import('@/utils/download')
    const result = await ensureDefaultDir()

    // appDataDir + join('downloads') 拼出目标路径
    expect(result).toBe('/mocked/appdata/downloads')

    // exists 返回 true → 不调 mkdir（已有目录就不重建）
    // 这里我们 mock exists → true，所以只验证 store.setDownloadDir 被调
    expect(mockStore.setDownloadDir).toHaveBeenCalledWith('/mocked/appdata/downloads')
  })

  it('Tauri 环境 + store 已配置 → 直接返回，不重建', async () => {
    fakeTauriRuntime()
    mockStore.downloadDir = '/already/configured'
    const { ensureDefaultDir } = await import('@/utils/download')
    const result = await ensureDefaultDir()
    expect(result).toBe('/already/configured')
    expect(mockStore.setDownloadDir).not.toHaveBeenCalled()
  })
})

// ---------------------------------------------------------------------------
// 4. downloadWavFromPath —— v0.3.7 R5 漏改 Artworks.vue 后的修复
// ---------------------------------------------------------------------------
//
// 验证意图：点 "wav ⬇" 按钮 → 读后端 music_wav_path 字节 → 走 Save As 落盘。
// 之前 Artworks.vue 调 player.exportWav()（v0.3.7 被 deprecated → 抛 Error），
// 必须改用 readFile(wavPath) → Blob → downloadBlob。
//
// mock 约定：mockReadFile 返 RIFF 头 4 字节；mockSave 返 chosen- 前缀路径；
// 验证 readFile 调了一次、save 收到的 defaultPath 含 store.downloadDir + filename。

describe('downloadWavFromPath', () => {
  beforeEach(() => {
    mockReadFile.mockClear()
    mockReadFile.mockResolvedValue(new Uint8Array([0x52, 0x49, 0x46, 0x46]))
  })

  it('Tauri 环境：读 wav_path 字节 → 拼 Blob → 调 downloadBlob 走 Save As', async () => {
    fakeTauriRuntime()
    mockStore.downloadDir = '/mocked/dl'
    const { downloadWavFromPath } = await import('@/utils/download')
    const wavPath = '/appdata/downloads/2026-07-31/music.wav'
    const filename = 'FingerTip-music-20260731-120000.wav'

    const result = await downloadWavFromPath(wavPath, filename)

    // readFile 被调一次，路径是 wav_path
    expect(mockReadFile).toHaveBeenCalledTimes(1)
    expect(mockReadFile).toHaveBeenCalledWith(wavPath)

    // save 收到的 defaultPath 含 store.downloadDir + filename
    expect(mockSave).toHaveBeenCalledTimes(1)
    expect(mockSave).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: `/mocked/dl/${filename}`,
        filters: expect.arrayContaining([
          expect.objectContaining({ extensions: expect.arrayContaining(['wav']) }),
        ]),
      }),
    )

    // 用户未取消 → 返回 mockSave 给的 chosen- 路径
    expect(result).toMatch(/chosen-FingerTip-music-20260731-120000\.wav$/)
  })

  it('非 Tauri 运行时直接返回 null，不调 readFile', async () => {
    // 不 fakeTauriRuntime() → isTauriEnv() === false
    const { downloadWavFromPath } = await import('@/utils/download')
    const result = await downloadWavFromPath('/x/y.wav', 'ignored.wav')
    expect(result).toBeNull()
    expect(mockReadFile).not.toHaveBeenCalled()
    expect(mockSave).not.toHaveBeenCalled()
  })

  it('用户取消 Save As → 返回 null，readFile 已发生但 writeFile 未发生', async () => {
    fakeTauriRuntime()
    mockSave.mockResolvedValueOnce(null)
    const { downloadWavFromPath } = await import('@/utils/download')
    const result = await downloadWavFromPath('/any.wav', 'y.wav')
    expect(result).toBeNull()
    expect(mockReadFile).toHaveBeenCalledOnce()
    expect(mockWriteFile).not.toHaveBeenCalled()
  })
})