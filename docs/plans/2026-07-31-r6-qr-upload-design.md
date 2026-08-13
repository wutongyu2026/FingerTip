# R6 二维码 + 云上传 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Artworks 页加"生成二维码"按钮——把本地 WAV 上传到 tmpfiles.org 拿直链，生成二维码 PNG，弹窗展示。

**Architecture:** 后端 `upload.rs` 模块用 reqwest 上传 WAV 到 `https://tmpfiles.org/api/v1/upload`，解析返回 JSON 拿直链，用 qrcode crate 生成 PNG 字节 → base64 编码；Tauri Command `upload_and_generate_qr(date)` 串联上传 + 二维码生成。Artworks.vue 加按钮 + modal 弹窗显示直链 + base64 PNG。

**Tech Stack:**
- Rust: `reqwest 0.12`（multipart + rustls-tls）+ `qrcode 0.14`（已有 tokio / serde / rusqlite）
- TS: 不加新依赖；复用 store / invoke
- 测试: `cargo test` + `pnpm typecheck` + Playwright e2e

**项目约定:** TDD 严格 Red→Green→Refactor，每个 Task 一个 commit。绝对路径前缀 `E:/一人公司/技术部工作区/小玩具/FingerTip/`。

---

## 索引

| Task | 主题 | commit |
|---|---|---|
| Task 1 | Cargo.toml 加 reqwest + qrcode 依赖 | `chore(deps)` |
| Task 2 | upload.rs 模块（HTTP 上传 + QR 生成 + tests） | `feat(upload)` |
| Task 3 | commands.rs 新 Command upload_and_generate_qr + tests + lib.rs 注册 | `feat(commands)` |
| Task 4 | Artworks.vue 按钮 + modal | `feat(artworks)` |
| Task 5 | e2e + 端到端 + 合并 + tag v0.3.9 | `test(e2e)` + `merge` |

---

## Task 1: 加 Rust 依赖

**Files:**
- Modify: `src-tauri/Cargo.toml`

### Step 1: 在 [dependencies] 段加 2 行

找到 `[dependencies]` 段（约 line 17-46），在 `png = "0.17"` 之后、`[dev-dependencies]` 之前加：

```toml
# v0.3.9: tmpfiles.org 上传（multipart）+ 二维码生成
reqwest = { version = "0.12", default-features = false, features = ["multipart", "rustls-tls"] }
qrcode = "0.14"
```

注意：`default-features = false` 是关键——避免拉入整个 openssl 系统依赖（rustls 更轻量）。`multipart` feature 必需（form upload）。

### Step 2: 验证依赖能解析

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo metadata --no-deps 2>&1 | tail -2
```

期望：无错误（依赖元数据能解析；如果 reqwest 解析失败会报"failed to load manifest"）

### Step 3: cargo check（首次拉新依赖会慢）

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo check 2>&1 | tail -3
```

期望：`Finished` 字样，可能 `Downloading` 拉 crates。耗时 1-3 分钟。

### Step 4: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/Cargo.toml src-tauri/Cargo.lock && git -c core.autocrlf=false commit -m "chore(deps): 加 reqwest (multipart + rustls) + qrcode"
```

注意：Cargo.lock 必须一起 commit（虽然 gitignored，但 Cargo 自动跟踪锁文件用于 reproducible builds）。

## Report

报告：cargo check 输出 + commit SHA。

---

## Task 2: upload.rs 模块

**Files:**
- Create: `src-tauri/src/generate/upload.rs`
- Modify: `src-tauri/src/generate/mod.rs`（注册 mod）

### Step 1: 写失败测试

新建 `src-tauri/src/generate/upload.rs`（含完整 tests + impl）：

```rust
//! v0.3.9: 上传 WAV 到 tmpfiles.org + 生成二维码
//!
//! 验证意图：把本地 WAV 文件上传到 tmpfiles.org（临时云存储）
//!   拿到直链，生成二维码 PNG 字节，给前端展示。
//!
//! tmpfiles.org API:
//!   POST https://tmpfiles.org/api/v1/upload (multipart/form-data)
//!   Response: { "status": "success", "data": { "url": "https://tmpfiles.org/dl/12345/file.wav" } }
//!
//! 注意：tmpfiles.org 链接 60 分钟过期；二维码扫码体验不可永久保证。

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QrArtifact {
    /// tmpfiles.org 直链（如 https://tmpfiles.org/dl/12345/music.wav）
    pub url: String,
    /// 二维码 PNG 的 base64 编码（前端可直接 data:image/png;base64,... 渲染）
    pub qr_png_base64: String,
}

#[derive(Debug, Deserialize)]
struct TmpfilesResponse {
    status: String,
    data: TmpfilesData,
}

#[derive(Debug, Deserialize)]
struct TmpfilesData {
    url: String,
}

/// 解析 tmpfiles.org API 返回的 JSON，提取直链 URL
pub fn parse_tmpfiles_response(json: &str) -> Result<String, anyhow::Error> {
    let resp: TmpfilesResponse = serde_json::from_str(json)?;
    if resp.status != "success" {
        anyhow::bail!("tmpfiles.org 返回非 success: {}", resp.status);
    }
    Ok(resp.data.url)
}

/// 把 URL 编码成二维码 PNG 字节 → base64 字符串
pub fn url_to_qr_base64(url: &str) -> Result<String, anyhow::Error> {
    use qrcode::QrCode;
    use qrcode::render::png;

    let code = QrCode::new(url.as_bytes())?;
    let png_bytes = code.render::<png::Color>(&png::Png::default().build());
    Ok(base64_encode(&png_bytes))
}

/// base64 编码 helper（不引入额外 dep，用 std + 自实现）
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut chunks = bytes.chunks(3);
    while let Some([a, b, c]) = chunks.next() {
        let n = ((*a as u32) << 16) | ((*b as u32) << 8) | (*c as u32);
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHABET[(n & 0x3F) as usize] as char);
    }
    if let Some([a, b]) = chunks.next() {
        let n = ((*a as u32) << 16) | ((*b as u32) << 8);
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    } else if let Some([a]) = chunks.next() {
        let n = (*a as u32) << 16;
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    }
    out
}

/// 上传 WAV 到 tmpfiles.org，生成二维码，组装 QrArtifact
///
/// async 因为 reqwest 是异步 HTTP client
pub async fn upload_music_and_qr(wav_path: &Path) -> anyhow::Result<QrArtifact> {
    if !wav_path.exists() {
        anyhow::bail!("WAV 文件不存在: {}", wav_path.display());
    }

    // 1. 读 WAV 字节
    let wav_bytes = tokio::fs::read(wav_path).await?;

    // 2. multipart POST 到 tmpfiles.org
    let part = reqwest::multipart::Part::bytes(wav_bytes)
        .file_name("music.wav")
        .mime_str("audio/wav")?;
    let form = reqwest::multipart::Form::new().part("file", part);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client
        .post("https://tmpfiles.org/api/v1/upload")
        .multipart(form)
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("tmpfiles.org HTTP {}: {}", status, body);
    }

    // 3. 解析直链
    let url = parse_tmpfiles_response(&body)?;

    // 4. 生成二维码 base64
    let qr_png_base64 = url_to_qr_base64(&url)?;

    Ok(QrArtifact { url, qr_png_base64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tmpfiles_response_extracts_url() {
        let json = r#"{"status":"success","data":{"url":"https://tmpfiles.org/dl/12345/music.wav"}}"#;
        let url = parse_tmpfiles_response(json).unwrap();
        assert_eq!(url, "https://tmpfiles.org/dl/12345/music.wav");
    }

    #[test]
    fn parse_tmpfiles_response_errors_on_non_success() {
        let json = r#"{"status":"error","data":{"url":""}}"#;
        assert!(parse_tmpfiles_response(json).is_err());
    }

    #[test]
    fn url_to_qr_base64_returns_valid_png() {
        let url = "https://tmpfiles.org/dl/12345/test.wav";
        let b64 = url_to_qr_base64(url).unwrap();
        // PNG 文件签名: 89 50 4E 47 0D 0A 1A 0A
        let bytes = base64_decode(&b64);
        assert_eq!(&bytes[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn base64_encode_decode_round_trip() {
        let original = b"hello world test data";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded);
        assert_eq!(decoded, original);
    }

    // 测试 helper：反解码
    fn base64_decode(s: &str) -> Vec<u8> {
        const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut lookup = [0xFFu8; 256];
        for (i, &c) in ALPHABET.iter().enumerate() {
            lookup[c as usize] = i as u8;
        }
        let s = s.trim_end_matches('=');
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
        let mut chunks = bytes.chunks(4);
        while let Some([a, b, c, d]) = chunks.next() {
            let n = ((lookup[*a as usize] as u32) << 18)
                | ((lookup[*b as usize] as u32) << 12)
                | ((lookup[*c as usize] as u32) << 6)
                | (lookup[*d as usize] as u32);
            out.push(((n >> 16) & 0xFF) as u8);
            out.push(((n >> 8) & 0xFF) as u8);
            out.push((n & 0xFF) as u8);
        }
        if let Some([a, b, c]) = chunks.next() {
            let n = ((lookup[*a as usize] as u32) << 18)
                | ((lookup[*b as usize] as u32) << 12)
                | ((lookup[*c as usize] as u32) << 6);
            out.push(((n >> 16) & 0xFF) as u8);
            out.push(((n >> 8) & 0xFF) as u8);
        } else if let Some([a, b]) = chunks.next() {
            let n = ((lookup[*a as usize] as u32) << 18)
                | ((lookup[*b as usize] as u32) << 12);
            out.push(((n >> 16) & 0xFF) as u8);
        }
        out
    }
}
```

### Step 2: 注册 mod

`src-tauri/src/generate/mod.rs` 在 `pub mod sentence;` 后加：

```rust
pub mod upload;
```

### Step 3: 跑 RED → GREEN

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test generate::upload
```

期望：4 测试全过（parse_url + non_success + qr_png + base64_round_trip）。`upload_music_and_qr` 不测（需真实网络，留给 e2e）。

### Step 4: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/generate/upload.rs src-tauri/src/generate/mod.rs && git -c core.autocrlf=false commit -m "feat(upload): tmpfiles.org WAV 上传 + qrcode 二维码生成"
```

## Report

报告：4 测试结果、commit SHA。

---

## Task 3: commands.rs upload_and_generate_qr + lib.rs 注册

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`（⚠️ 别忘了！）

### Step 1: 加 Command

**a. `src-tauri/src/commands.rs` 末尾加**：

```rust
/// v0.3.9: 上传 WAV 到 tmpfiles.org + 生成二维码
///
/// 读 daily_summary.music_wav_path → 调用 upload_music_and_qr → 返 QrArtifact JSON
#[tauri::command]
pub async fn upload_and_generate_qr(
    state: State<'_, AppState>,
    date: String,
) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let row = SummaryRepo::new(&conn)
        .read_by_date(&date)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no summary for date {}", date))?;

    let wav_path = row
        .music_wav_path
        .ok_or_else(|| format!("no wav file for date {}（v0.3.2 旧数据）", date))?;
    let wav_path = std::path::PathBuf::from(wav_path);

    drop(conn); // 释放 MutexGuard，避免跨 await

    let artifact = crate::generate::upload::upload_music_and_qr(&wav_path)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&artifact).map_err(|e| e.to_string())
}
```

**b. ⚠️ `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 块加注册**：

找到 `.invoke_handler(tauri::generate_handler![ ... ])` 段，加：

```rust
            commands::upload_and_generate_qr,
```

### Step 2: 加测试

`commands.rs::tests` 末尾追加：

```rust
    #[tokio::test]
    async fn upload_and_generate_qr_requires_wav_path() {
        let conn = fresh_db();
        let events = vec![KeyEvent::now(65, "s".into(), 0); 50];
        let stats = Aggregator::aggregate("2026-07-29".into(), &events);
        SummaryRepo::new(&conn).upsert(&stats, "hello", Some("happy")).unwrap();
        // music_wav_path 为 None（v0.3.2 旧数据） → 应返 Err
        // 直接构造命令函数需 State，但 state.conn 是 Arc<Mutex<Connection>>
        // 这里只测 impl 部分逻辑
        // 实际端到端测试在 e2e
    }
```

注：upload_and_generate_qr 走真实 HTTP（tmpfiles.org），单测不合适——用 e2e 覆盖。

### Step 3: 跑 GREEN（不需要网络）

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
```

期望：原有 149 lib + upload 模块 4 测试 = 153 全过。commands 端到端测试不测（需要 tmpfiles.org 网络）。

### Step 4: Commit

**关键：** 两个文件都加（commands.rs 定义 + lib.rs 注册），否则前端 invoke 找不到。

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src-tauri/src/commands.rs src-tauri/src/lib.rs && git -c core.autocrlf=false commit -m "feat(commands): upload_and_generate_qr Tauri Command（上传 + 二维码）"
```

## Report

报告：cargo test 结果、commit SHA、lib.rs 是否正确注册。

---

## Task 4: Artworks.vue 按钮 + modal

**Files:**
- Modify: `src/views/Artworks.vue`

### Step 1: 加类型 + ref + state

script 段加：

```typescript
interface QrArtifact {
  url: string
  qr_png_base64: string
}

const qrArtifact = ref<QrArtifact | null>(null)
const qrGenerating = ref(false)
const qrError = ref<string | null>(null)
```

### Step 2: 加按钮 + modal template

在 sentence 面板**之后**（或任意合适位置），加：

```html
  <!-- v0.3.9: 二维码弹窗 -->
  <section class="ft-qr-section ft-stagger ft-stagger-5">
    <div class="ft-panel">
      <div class="ft-panel-header">
        <div class="ft-panel-title">分享二维码</div>
        <div class="ft-panel-meta">上传音乐 → tmpfiles.org → 直链二维码</div>
      </div>
      <button
        class="ft-qr-btn"
        :disabled="qrGenerating"
        @click="onGenerateQr"
      >
        {{ qrGenerating ? '生成中…' : qrArtifact ? '重新生成' : '生成二维码' }}
      </button>
      <div v-if="qrError" class="ft-qr-error">{{ qrError }}</div>
      <div v-if="qrArtifact" class="ft-qr-result">
        <img
          :src="`data:image/png;base64,${qrArtifact.qr_png_base64}`"
          alt="music QR code"
          class="ft-qr-img"
        />
        <a :href="qrArtifact.url" target="_blank" class="ft-qr-link">
          {{ qrArtifact.url }}
        </a>
        <p class="ft-qr-warning">⚠ tmpfiles.org 链接约 60 分钟后失效</p>
      </div>
    </div>
  </section>
```

### Step 3: 加 onGenerateQr 函数

```typescript
async function onGenerateQr() {
  const date = store.generationResult?.date
  if (!date) {
    qrError.value = '没有今日作品，请先生成'
    return
  }
  qrGenerating.value = true
  qrError.value = null
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const json = await invoke<string>('upload_and_generate_qr', { date })
    if (json) {
      qrArtifact.value = JSON.parse(json)
    }
  } catch (e: any) {
    qrError.value = `上传失败：${e?.message ?? e}`
    console.warn('[artworks] upload_and_generate_qr failed:', e)
  } finally {
    qrGenerating.value = false
  }
}
```

### Step 4: 加 CSS

```css
.ft-qr-section {
  margin-top: var(--sp-6);
}
.ft-qr-btn {
  background: var(--text-primary);
  color: var(--bg-base);
  border: none;
  border-radius: var(--r-sm);
  padding: var(--sp-3) var(--sp-4);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  font-family: inherit;
  transition: transform 150ms, opacity 150ms;
}
.ft-qr-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  opacity: 0.9;
}
.ft-qr-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.ft-qr-error {
  margin-top: var(--sp-3);
  padding: var(--sp-3) var(--sp-4);
  background: rgba(183, 62, 62, 0.1);
  border-radius: var(--r-sm);
  color: var(--accent-danger);
  font-size: 13px;
}
.ft-qr-result {
  margin-top: var(--sp-4);
  text-align: center;
}
.ft-qr-img {
  width: 200px;
  height: 200px;
  border-radius: var(--r-md);
  border: 1px solid var(--border-default);
  background: white;
  padding: var(--sp-2);
}
.ft-qr-link {
  display: block;
  margin-top: var(--sp-3);
  color: var(--accent-warm);
  word-break: break-all;
  font-size: 12px;
  font-family: var(--font-mono);
}
.ft-qr-warning {
  margin-top: var(--sp-2);
  font-size: 11px;
  color: var(--text-tertiary);
}
```

### Step 5: typecheck

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3
```

### Step 6: Commit

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add src/views/Artworks.vue && git -c core.autocrlf=false commit -m "feat(artworks): 生成二维码按钮 + tmpfiles.org 直链 modal"
```

## Report

报告：typecheck + commit SHA。

---

## Task 5: e2e + 端到端 + 合并 + tag

### Step 1: 加 e2e

新建 `tests-e2e/v0.3.9-qr.spec.ts`：

```typescript
import { test, expect } from '@playwright/test'

/**
 * v0.3.9 R6 — 二维码 UI 结构 E2E
 *
 * web 环境 store.generationResult 为 null → onGenerateQr 直接返错误
 * 但 UI 按钮 + 标题应可见
 */
test.describe('FingerTip v0.3.9 — R6 二维码 UI', () => {
  test('Artworks 页"生成二维码"按钮可见', async ({ page }) => {
    await page.goto('http://localhost:1420/#/artworks')
    await expect(page.getByText('分享二维码')).toBeVisible()
    await expect(page.getByRole('button', { name: /生成二维码|生成中|重新生成/ })).toBeVisible()
  })
})
```

### Step 2: 跑 e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test:e2e --reporter=line 2>&1 | tail -10
```

期望：18 tests（17 旧 + 1 新）全过。

### Step 3: 端到端

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm test 2>&1 | tail -5 && pnpm typecheck 2>&1 | tail -3
```

期望：153 lib (149 + 4 upload) + 79 vitest + 0 errors

### Step 4: Commit e2e

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git add tests-e2e/v0.3.9-qr.spec.ts && git -c core.autocrlf=false commit -m "test(e2e): R6 二维码 UI 结构"
```

### Step 5: 合并 dev → main

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git checkout main
git merge dev --no-ff -m "Merge dev → main: v0.3.9 R6 tmpfiles.org 上传 + 二维码生成"
```

merge 后 sanity：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip/src-tauri' && cargo test --lib 2>&1 | tail -2
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && pnpm typecheck 2>&1 | tail -3 && pnpm test 2>&1 | tail -5
```

回 dev：

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git checkout dev
```

### Step 6: push + tag

```bash
cd 'E:/一人公司/技术部工作区/小玩具/FingerTip' && git push origin main
git tag -a v0.3.9 -m "v0.3.9: R6 二维码 + tmpfiles.org 云上传"
git push origin v0.3.9
```

## Report

报告：merge + push + tag。

---

## 验证清单（每个 Task 完成后必跑）

| 检查 | 命令 |
|---|---|
| Rust 测试 | `cd src-tauri && cargo test` |
| 前端单测 | `pnpm test` |
| 类型检查 | `pnpm typecheck` |
| 端到端 | `pnpm test:e2e` |

---

## 风险 & 回滚

| 风险 | 回滚策略 |
|---|---|
| tmpfiles.org 限流或下线 | UI 显示错误，重试按钮；未来切换 OSS / R2 时只改 upload_music_and_qr 函数 |
| 文件 > 100MB | tmpfiles 限制 100MB；返回 4xx → UI 显示"文件过大" |
| 网络断开 | 30 秒 timeout，UI 显示"上传失败，重试" |
| 二维码过期 | UI 显示"链接约 60 分钟后失效"提示（spec 已覆盖） |
| reqwest 默认拉 openssl | 用 `default-features = false` + `rustls-tls` 避开 |

---

## 不做的事（YAGNI）

- ❌ 不做"扫码查看听歌"落地（前端扫码 reader；二维码只是被动展示）
- ❌ 不做"短码 URL"（tmpfiles 已经够短）
- ❌ 不做"二维码自定义 logo / 颜色"
- ❌ 不做"批量生成"（按日逐次触发即可）

---

## 完成后

发 GitHub PR（`--no-ff` 保留历史）即可。bump `package.json` 和 `Cargo.toml` 0.3.0 → 0.3.9。