// v0.3 Stage 5 Task 5.2 — 下载基础设施
//
// 设计目标：
// 1. 单一入口（downloadBlob）封装 "Save As 对话框 + 写文件" 的全流程
// 2. 默认目录由 store.downloadDir 控制（用户可在 Settings 改）
// 3. 首次启动自动在 %APPDATA%\com.fingertip.app\downloads\ 创建 + 写 store
// 4. web 模式（无 Tauri runtime）安全 fallback —— 不抛错，仅返回 null
//
// 验证意图：
// - 为什么要 plugin-dialog + plugin-fs：浏览器 <a download> 无法指定路径 + 无法保证用户选定的目标路径
// - 为什么要 appDataDir 下的 downloads/：与 DB 路径同目录 → 用户卸载 App 一次性清理
// - 为什么 web 模式也支持：vitest jsdom 环境不能假装 Tauri，但单元测试只测参数拼装

import { save } from '@tauri-apps/plugin-dialog'
import { useAppStore } from '@/stores/app'

/**
 * 是否在 Tauri 运行时内（不在则为 web / 单元测试环境）。
 * 排除 jsdom 下的 `__TAURI_INTERNALS__` 假对象。
 */
function isTauriEnv(): boolean {
  return (
    typeof window !== 'undefined' &&
    '__TAURI_INTERNALS__' in window &&
    // jsdom + Tauri mock 时 `__TAURI_INTERNALS__` 可能是空对象，必须确认有真实 transformer
    typeof (window as any).__TAURI_INTERNALS__?.transformCallback === 'function'
  )
}

/**
 * 把 blob 写到用户选定的路径。
 * 默认目录 + 默认文件名由 caller 拼好传入。
 *
 * @param defaultFilename 建议文件名（含扩展名）
 * @param blob 要写入的数据
 * @param ext 默认扩展名（用于 dialog filter），例如 "png"
 * @param mime MIME type 给 dialog filter 用，例如 "image/png"
 * @returns 写入的绝对路径；用户取消返回 null；web 模式也返回 null
 */
export async function downloadBlob(
  defaultFilename: string,
  blob: Blob,
  ext: string,
  mime: string,
): Promise<string | null> {
  // web 模式：直接返回 null + 警告，避免污染 e2e 单测
  if (!isTauriEnv()) {
    console.warn('[download] downloadBlob called in non-Tauri env, skipped')
    return null
  }

  const store = useAppStore()
  const defaultDir = store.downloadDir || ''
  const defaultPath = defaultDir
    ? `${defaultDir}/${defaultFilename}`
    : defaultFilename

  // 1. 弹 Save As 对话框（filter name 显式带 mime hint，方便用户识别格式）
  const path = await save({
    defaultPath,
    filters: [{ name: `${ext.toUpperCase()} (${mime})`, extensions: [ext] }],
  })

  if (!path) return null // user cancelled

  // 2. 写文件（动态 import 避免 web 模式加载 plugin-fs 报错）
  const { writeFile } = await import('@tauri-apps/plugin-fs')
  const bytes = new Uint8Array(await blob.arrayBuffer())
  await writeFile(path, bytes)
  return path
}

/**
 * v0.3.7 R5 修复：读后端生成的本地 WAV（mood_music.wasm 等）→ 拼 Blob → 走 Save As。
 *
 * 之前的 Artworks.vue.onDownloadMusic 调 player.exportWav()，但 v0.3.7 把
 *   exportWav 标记 deprecated 直接抛 Error —— 修这个 bug 的方法就是
 *   从 Artworks.vue 抽出 wav_path → bytes → Blob → downloadBlob 路径，与 Art 同享。
 *
 * 验证意图：
 * - 为什么要 readFile + Blob：浏览器 <a download> 不能直接吃 wav_path（是 Tauri asset protocol 路径），
 *   必须读到 Uint8Array → Blob 才能走 downloadBlob 的 Save As dialog。
 * - 为什么 web 模式返 null：vitest jsdom 没 __TAURI_INTERNALS__ → 与 downloadBlob 行为对齐。
 *
 * @param wavPath 后端产物绝对路径（来自 `store.generationResult.music_wav_path`）
 * @param filename 建议文件名（与 downloadBlob 第 1 参数语义一致）
 * @returns 写入的绝对路径；用户取消 / web 模式返 null
 */
export async function downloadWavFromPath(
  wavPath: string,
  filename: string,
): Promise<string | null> {
  if (!isTauriEnv()) {
    console.warn('[download] downloadWavFromPath called in non-Tauri env, skipped')
    return null
  }

  // 1. 读 wav 字节（动态 import 避免 web 模式加载 plugin-fs 报错）
  const { readFile } = await import('@tauri-apps/plugin-fs')
  const bytes = await readFile(wavPath)
  const blob = new Blob([bytes], { type: 'audio/wav' })

  // 2. 走已有 downloadBlob：弹 Save As → 写盘
  return downloadBlob(filename, blob, 'wav', 'audio/wav')
}

/**
 * 首次启动调用：如果用户还没配置 downloadDir，自动创建
 * `%APPDATA%\com.fingertip.app\downloads\` 并写入 store。
 *
 * 顺序敏感：调用方必须先 `store.loadDownloadDir()`，再 `await ensureDefaultDir()`。
 * 否则 localStorage 中已存但还没回填 store 的目录会被误判为「未配置」。
 *
 * @returns 已确定的下载目录（用户已配置 / 自动创建 / web 模式空字符串）
 */
export async function ensureDefaultDir(): Promise<string> {
  const store = useAppStore()

  // 1. 用户已配置 → 直接返回（不论是上次的还是本次改的）
  if (store.downloadDir) return store.downloadDir

  // 2. web 模式：plugin 不可用，静默返回空串
  if (!isTauriEnv()) return ''

  // 3. Tauri 桌面：解析 %APPDATA%\com.fingertip.app\ + 'downloads'，创建目录，写 store
  const { appDataDir, join } = await import('@tauri-apps/api/path')
  const { mkdir, exists } = await import('@tauri-apps/plugin-fs')

  const base = await appDataDir()
  const dir = await join(base, 'downloads')

  if (!(await exists(dir))) {
    await mkdir(dir, { recursive: true })
  }

  store.setDownloadDir(dir)
  return dir
}
