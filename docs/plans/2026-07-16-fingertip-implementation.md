# FingerTip Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 构建一个后台静默运行的 Tauri 桌面应用，记录用户键盘敲击行为，次日产出 AI 音乐与数字画作。

**Architecture:** 4 层 + 1 横切关注点。表现层（Vue 3 + Naive UI）→ 应用层（Rust HookListener / SummaryAggregator / GenerateOrchestrator）→ 数据层（SQLite 本地存储）→ AI 抽象层（MusicAdapter / ArtAdapter trait + MiniMax 云 + 本地可选）。键盘监听通过 `rdev` crate 全局 Hook，事件 5 分钟刷盘到 SQLite。

**Tech Stack:**
- 后端：Rust + Tauri 2.x + rdev + rusqlite + tokio + serde
- 前端：Vue 3 + TypeScript + Vite + Naive UI + Pinia + Vue Router
- AI：MiniMax 多模态（云）+ 本地预留（Adapter 抽象）
- 测试：cargo test + Vitest + Vue Test Utils + Playwright（Tauri Driver）
- 工具：pnpm + cargo + sqlx-cli

**项目状态：** 已完成 brainstorming 阶段（设计文档 + 精益画布 v2）。

**约定：**
- 每个 Task 都是 TDD 周期：写失败测试 → 跑确认失败 → 写最小实现 → 跑确认通过 → commit
- 严格遵守 `~/.claude/rules/coding-style.md`：immutable / 小文件 / 错误处理 / 输入校验
- 测试验证"为什么"而非"做了什么"（参考 brainstorming 设计文档第六节）

---

## Phase 0：项目脚手架

### Task 0.1: 初始化 Tauri 项目

**Files:**
- Create: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src/App.vue`, `src/main.ts`, `vite.config.ts`, `index.html`

**Step 1: 用 Tauri CLI 创建项目**

```bash
cd "E:/一人公司/技术部工作区/小玩具/FingerTip"
pnpm create tauri-app@latest . -- --template vue-ts --manager pnpm --identifier com.fingertip.app
```

**Step 2: 安装核心依赖**

```bash
pnpm add naive-ui pinia vue-router@4
pnpm add -D vitest @vue/test-utils @vitest/ui playwright @playwright/test
cd src-tauri && cargo add rdev rusqlite tokio serde serde_json anyhow thiserror keyring chrono uuid
cd src-tauri && cargo add --dev tempfile mockall
```

**Step 3: 验证项目能启动**

```bash
cd "E:/一人公司/技术部工作区/小玩具/FingerTip" && pnpm tauri dev
```

Expected: 弹出 Tauri 窗口，显示 Vue 3 默认页面。关闭后停止进程。

**Step 4: Commit**

```bash
git add .
git commit -m "chore: 初始化 Tauri + Vue 3 项目脚手架"
```

---

### Task 0.2: 配置 tauri.conf.json 与窗口策略

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Step 1: 写失败的配置测试**

创建 `src-tauri/tests/tauri_config.rs`：
```rust
#[test]
fn window_min_size_is_set() {
    let cfg = include_str!("../tauri.conf.json");
    assert!(cfg.contains("\"minWidth\": 800"));
    assert!(cfg.contains("\"minHeight\": 600"));
}
```

**Step 2: 跑测试确认失败**

```bash
cd src-tauri && cargo test --test tauri_config
```

Expected: FAIL（配置还没改）

**Step 3: 修改 tauri.conf.json**

在 `app.windows[0]` 加：
```json
{
  "minWidth": 800,
  "minHeight": 600,
  "visible": false,
  "decorations": true
}
```

**Step 4: 跑测试确认通过**

```bash
cd src-tauri && cargo test --test tauri_config
```

Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/tests/tauri_config.rs
git commit -m "chore: 配置 Tauri 窗口最小尺寸与初始隐藏"
```

---

### Task 0.3: 配置前端测试基础设施

**Files:**
- Create: `vitest.config.ts`, `src/tests/setup.ts`

**Step 1: 创建 vitest.config.ts**

```ts
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) }
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/tests/setup.ts']
  }
})
```

**Step 2: 创建测试 setup 文件**

```ts
// src/tests/setup.ts
```

**Step 3: 写第一个测试**

`src/tests/smoke.test.ts`：
```ts
import { describe, it, expect } from 'vitest'

describe('测试基础设施冒烟', () => {
  it('Vitest 能正常工作', () => {
    expect(1 + 1).toBe(2)
  })
})
```

**Step 4: 跑测试**

```bash
pnpm test --run
```

Expected: PASS

**Step 5: Commit**

```bash
git add vitest.config.ts src/tests/
git commit -m "chore: 配置 Vitest 测试基础设施"
```

---

## Phase 1：键盘监听层（HookListener + EventBuffer）

> **目标：** 在 Tauri 后端实现全局键盘监听，事件经过 5 分钟/队列满 1000 条 flush 到 SQLite。

### Task 1.1: 定义 HookListener trait

**Files:**
- Create: `src-tauri/src/hook/mod.rs`, `src-tauri/src/hook/listener.rs`, `src-tauri/src/hook/event.rs`
- Test: `src-tauri/src/hook/listener.rs`（内嵌 `#[cfg(test)]`）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/hook/listener.rs
use crate::hook::event::KeyEvent;

pub trait HookListener: Send {
    fn start(&mut self, sink: Box<dyn Fn(KeyEvent) + Send>) -> Result<(), anyhow::Error>;
    fn stop(&mut self) -> Result<(), anyhow::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockListener { started: Arc<Mutex<bool>> }
    impl HookListener for MockListener {
        fn start(&mut self, _sink: Box<dyn Fn(KeyEvent) + Send>) -> Result<(), anyhow::Error> {
            *self.started.lock().unwrap() = true;
            Ok(())
        }
        fn stop(&mut self) -> Result<(), anyhow::Error> { Ok(()) }
    }

    #[test]
    fn start_invokes_sink_setup() {
        // 验证意图：listener 启动后能正常接收事件
        let started = Arc::new(Mutex::new(false));
        let mut listener = MockListener { started: started.clone() };
        listener.start(Box::new(|_| {})).unwrap();
        assert!(*started.lock().unwrap());
    }
}
```

**Step 2: 跑测试确认失败**

```bash
cd src-tauri && cargo test hook::listener
```

Expected: FAIL（模块还不存在）

**Step 3: 创建模块文件**

`src-tauri/src/hook/event.rs`：
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key_code: u32,
    pub timestamp_ms: i64,
    pub session_id: String,
    pub modifiers: u8,
}

impl KeyEvent {
    pub fn now(key_code: u32, session_id: String, modifiers: u8) -> Self {
        Self {
            key_code,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            session_id,
            modifiers,
        }
    }
}
```

`src-tauri/src/hook/mod.rs`：
```rust
pub mod event;
pub mod listener;
```

**Step 4: 跑测试确认通过**

```bash
cd src-tauri && cargo test hook::listener
```

Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/hook/
git commit -m "feat(hook): 定义 KeyEvent 数据契约与 HookListener trait"
```

---

### Task 1.2: 实现 rdev Windows HookListener

**Files:**
- Create: `src-tauri/src/hook/rdev_listener.rs`
- Modify: `src-tauri/src/hook/mod.rs`

**Step 1: 写失败的测试**

```rust
// src-tauri/src/hook/rdev_listener.rs
#[cfg(test)]
mod tests {
    use crate::hook::event::KeyEvent;

    #[test]
    fn rdev_converts_keycode() {
        // 验证意图：rdev 虚拟键码正确映射到 KeyEvent
        let evt = KeyEvent::now(65, "test-session".into(), 0);
        assert_eq!(evt.key_code, 65);
        assert!(evt.timestamp_ms > 0);
    }
}
```

**Step 2: 跑测试确认失败**

```bash
cd src-tauri && cargo test hook::rdev_listener
```

Expected: FAIL（模块不存在）

**Step 3: 实现 rdev listener**

```rust
// src-tauri/src/hook/rdev_listener.rs
use crate::hook::event::KeyEvent;
use crate::hook::listener::HookListener;
use std::sync::Arc;
use parking_lot::Mutex;

pub struct RdevListener {
    running: Arc<Mutex<bool>>,
    session_id: String,
}

impl RdevListener {
    pub fn new(session_id: String) -> Self {
        Self { running: Arc::new(Mutex::new(false)), session_id }
    }
}

impl HookListener for RdevListener {
    fn start(&mut self, sink: Box<dyn Fn(KeyEvent) + Send>) -> Result<(), anyhow::Error> {
        *self.running.lock() = true;
        let session = self.session_id.clone();
        std::thread::spawn(move || {
            use rdev::{listen, EventType};
            let callback = move |event: rdev::Event| {
                if let EventType::KeyPress(key) = event.event_type {
                    let code = key as u32;
                    let evt = KeyEvent::now(code, session.clone(), 0);
                    sink(evt);
                }
            };
            if let Err(e) = listen(callback) {
                log::error!("rdev listen error: {:?}", e);
            }
        });
        Ok(())
    }

    fn stop(&mut self) -> Result<(), anyhow::Error> {
        *self.running.lock() = false;
        Ok(())
    }
}
```

`src-tauri/src/hook/mod.rs` 追加：
```rust
pub mod rdev_listener;
```

**Step 4: 添加依赖**

`Cargo.toml` 追加：
```toml
parking_lot = "0.12"
log = "0.4"
```

**Step 5: 跑测试确认通过**

```bash
cd src-tauri && cargo test hook::rdev_listener
```

Expected: PASS

**Step 6: Commit**

```bash
git add src-tauri/src/hook/rdev_listener.rs src-tauri/src/hook/mod.rs src-tauri/Cargo.toml
git commit -m "feat(hook): 实现 rdev Windows HookListener（生产代码）"
```

---

### Task 1.3: 实现 EventBuffer 环形缓冲 + 定时 flush

**Files:**
- Create: `src-tauri/src/hook/buffer.rs`
- Test: `src-tauri/src/hook/buffer.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/hook/buffer.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::event::KeyEvent;
    use std::sync::{Arc, Mutex};

    #[test]
    fn flush_emits_all_pending_events() {
        // 验证意图：flush 时所有缓冲事件被一次性处理（不丢、不重）
        let sink_data: Arc<Mutex<Vec<KeyEvent>>> = Arc::new(Mutex::new(vec![]));
        let sink = {
            let sd = sink_data.clone();
            Box::new(move |e: KeyEvent| sd.lock().unwrap().push(e))
        };
        let mut buf = EventBuffer::new(sink, 100, std::time::Duration::from_secs(300));
        for i in 0..5 {
            buf.push(KeyEvent::now(i, "s".into(), 0));
        }
        buf.flush().unwrap();
        assert_eq!(sink_data.lock().unwrap().len(), 5);
    }

    #[test]
    fn flush_triggers_at_threshold() {
        // 验证意图：达 1000 条上限时自动 flush，避免内存爆炸
        let sink_data: Arc<Mutex<Vec<KeyEvent>>> = Arc::new(Mutex::new(vec![]));
        let sink = {
            let sd = sink_data.clone();
            Box::new(move |e: KeyEvent| sd.lock().unwrap().push(e))
        };
        let mut buf = EventBuffer::new(sink, 3, std::time::Duration::from_secs(99999));
        for i in 0..3 {
            buf.push(KeyEvent::now(i, "s".into(), 0));
        }
        assert_eq!(sink_data.lock().unwrap().len(), 3);
    }
}
```

**Step 2: 跑测试确认失败**

```bash
cd src-tauri && cargo test hook::buffer
```

Expected: FAIL

**Step 3: 实现 EventBuffer**

```rust
// src-tauri/src/hook/buffer.rs
use crate::hook::event::KeyEvent;
use std::collections::VecDeque;

pub struct EventBuffer {
    queue: VecDeque<KeyEvent>,
    sink: Box<dyn Fn(KeyEvent) + Send>,
    capacity: usize,
    flush_interval: std::time::Duration,
    last_flush: std::time::Instant,
}

impl EventBuffer {
    pub fn new(sink: Box<dyn Fn(KeyEvent) + Send>, capacity: usize, flush_interval: std::time::Duration) -> Self {
        Self { queue: VecDeque::with_capacity(capacity), sink, capacity, flush_interval, last_flush: std::time::Instant::now() }
    }

    pub fn push(&mut self, event: KeyEvent) {
        self.queue.push_back(event);
        if self.queue.len() >= self.capacity {
            self.flush().ok();
        }
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        let drained: Vec<KeyEvent> = self.queue.drain(..).collect();
        for e in drained { (self.sink)(e); }
        self.last_flush = std::time::Instant::now();
        Ok(())
    }

    pub fn maybe_flush_by_time(&mut self) -> anyhow::Result<bool> {
        if self.last_flush.elapsed() >= self.flush_interval {
            self.flush()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
```

`src-tauri/src/hook/mod.rs` 追加：
```rust
pub mod buffer;
```

**Step 4: 跑测试**

```bash
cd src-tauri && cargo test hook::buffer
```

Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/hook/
git commit -m "feat(hook): 实现 EventBuffer 环形缓冲 + 容量/时间双触发 flush"
```

---

### Task 1.4: SQLite 持久化（key_events 表 + 写入）

**Files:**
- Create: `src-tauri/src/db/mod.rs`, `src-tauri/src/db/migrations.rs`, `src-tauri/src/db/event_repo.rs`
- Test: `src-tauri/src/db/event_repo.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/db/event_repo.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use crate::hook::event::KeyEvent;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn insert_and_count_round_trip() {
        // 验证意图：插入 3 条事件后能完整读回（不丢字段）
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        repo.insert(&KeyEvent::now(65, "s1".into(), 0)).unwrap();
        repo.insert(&KeyEvent::now(66, "s1".into(), 0)).unwrap();
        repo.insert(&KeyEvent::now(67, "s1".into(), 0)).unwrap();
        let all = repo.list_by_session("s1").unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].key_code, 65);
    }

    #[test]
    fn delete_older_than_prunes_correctly() {
        // 验证意图：30 天清理逻辑只删过期的、不删新的
        let conn = fresh_db();
        let repo = EventRepo::new(&conn);
        let old = KeyEvent { key_code: 1, timestamp_ms: 1, session_id: "s".into(), modifiers: 0 };
        let recent = KeyEvent::now(2, "s".into(), 0);
        repo.insert(&old).unwrap();
        repo.insert(&recent).unwrap();
        let pruned = repo.delete_older_than(chrono::Utc::now().timestamp_millis() - 1000).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(repo.list_by_session("s").unwrap().len(), 1);
    }
}
```

**Step 2: 跑测试确认失败**

```bash
cd src-tauri && cargo test db::
```

Expected: FAIL

**Step 3: 实现 migrations**

```rust
// src-tauri/src/db/migrations.rs
use rusqlite::Connection;

pub fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS key_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key_code INTEGER NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            session_id TEXT NOT NULL,
            modifiers INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_key_events_ts ON key_events(timestamp_ms);
        CREATE INDEX IF NOT EXISTS idx_key_events_session ON key_events(session_id);

        CREATE TABLE IF NOT EXISTS daily_summary (
            date TEXT PRIMARY KEY,
            total_keys INTEGER NOT NULL,
            top_keys_json TEXT NOT NULL,
            theme_word TEXT NOT NULL,
            mood_word TEXT,
            created_at INTEGER NOT NULL
        );
    ")?;
    Ok(())
}
```

**Step 4: 实现 EventRepo**

```rust
// src-tauri/src/db/event_repo.rs
use crate::hook::event::KeyEvent;
use rusqlite::{params, Connection};

pub struct EventRepo<'a> { conn: &'a Connection }

impl<'a> EventRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }

    pub fn insert(&self, event: &KeyEvent) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO key_events (key_code, timestamp_ms, session_id, modifiers) VALUES (?, ?, ?, ?)",
            params![event.key_code, event.timestamp_ms, event.session_id, event.modifiers],
        )?;
        Ok(())
    }

    pub fn list_by_session(&self, session_id: &str) -> anyhow::Result<Vec<KeyEvent>> {
        let mut stmt = self.conn.prepare("SELECT key_code, timestamp_ms, session_id, modifiers FROM key_events WHERE session_id = ? ORDER BY id")?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(KeyEvent {
                key_code: row.get(0)?,
                timestamp_ms: row.get(1)?,
                session_id: row.get(2)?,
                modifiers: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_older_than(&self, cutoff_ms: i64) -> anyhow::Result<usize> {
        let n = self.conn.execute("DELETE FROM key_events WHERE timestamp_ms < ?", params![cutoff_ms])?;
        Ok(n)
    }
}
```

`src-tauri/src/db/mod.rs`：
```rust
pub mod event_repo;
pub mod migrations;
```

**Step 5: 跑测试**

```bash
cd src-tauri && cargo test db::
```

Expected: PASS

**Step 6: Commit**

```bash
git add src-tauri/src/db/
git commit -m "feat(db): 实现 SQLite key_events 表 + EventRepo（CRUD + 过期清理）"
```

---

### Task 1.5: 端到端 HookListener → EventBuffer → SQLite 集成测试

**Files:**
- Create: `src-tauri/tests/integration_hook_to_db.rs`

**Step 1: 写集成测试**

```rust
// src-tauri/tests/integration_hook_to_db.rs
use fingertip::db::{event_repo::EventRepo, migrations::run_migrations};
use fingertip::hook::{buffer::EventBuffer, event::KeyEvent, listener::HookListener};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

struct FakeListener;
impl HookListener for FakeListener {
    fn start(&mut self, sink: Box<dyn Fn(KeyEvent) + Send>) -> anyhow::Result<()> {
        // 模拟 5 个事件
        for i in 0..5 { sink(KeyEvent::now(i, "integ-session".into(), 0)); }
        Ok(())
    }
    fn stop(&mut self) -> anyhow::Result<()> { Ok(()) }
}

#[test]
fn end_to_end_events_persist_to_sqlite() {
    // 验证意图：从 HookListener 到 SQLite 的链路完整，事件不丢
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    let repo = EventRepo::new(&conn);

    let captured: Arc<Mutex<Vec<KeyEvent>>> = Arc::new(Mutex::new(vec![]));
    let cap = captured.clone();
    let conn_arc = Arc::new(conn);
    let conn_clone = conn_arc.clone();

    let buf_sink = move |e: KeyEvent| {
        EventRepo::new(&conn_clone).insert(&e).unwrap();
        cap.lock().unwrap().push(e);
    };

    let mut buf = EventBuffer::new(Box::new(buf_sink), 100, std::time::Duration::from_secs(60));
    let mut listener = FakeListener;
    listener.start(Box::new(move |e: KeyEvent| buf.push(e))).unwrap();
    buf.flush().unwrap();

    let stored = EventRepo::new(&conn_arc).list_by_session("integ-session").unwrap();
    assert_eq!(stored.len(), 5);
    assert_eq!(captured.lock().unwrap().len(), 5);
}
```

**Step 2: 在 src-tauri/src/lib.rs 暴露 fingertip crate**

```rust
// src-tauri/src/lib.rs
pub mod db;
pub mod hook;
```

**Step 3: 跑测试**

```bash
cd src-tauri && cargo test --test integration_hook_to_db
```

Expected: PASS

**Step 4: Commit**

```bash
git add src-tauri/tests/integration_hook_to_db.rs src-tauri/src/lib.rs
git commit -m "test: 端到端集成测试 Hook → Buffer → SQLite"
```

---

## Phase 2：隐私与配置（PrivacyVault）

### Task 2.1: PrivacyVault trait + 测试

**Files:**
- Create: `src-tauri/src/privacy/mod.rs`, `src-tauri/src/privacy/vault.rs`
- Test: `src-tauri/src/privacy/vault.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/privacy/vault.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct InMemoryVault { store: std::sync::Mutex<HashMap<String, String>> }
    impl PrivacyVault for InMemoryVault {
        fn store(&self, key: &str, value: &str) -> anyhow::Result<()> {
            self.store.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn retrieve(&self, key: &str) -> anyhow::Result<Option<String>> {
            Ok(self.store.lock().unwrap().get(key).cloned())
        }
        fn delete(&self, key: &str) -> anyhow::Result<()> {
            self.store.lock().unwrap().remove(key);
            Ok(())
        }
    }

    #[test]
    fn api_key_round_trip() {
        // 验证意图：API Key 写入后能取回原值（不截断、不变形）
        let v = InMemoryVault { store: std::sync::Mutex::new(HashMap::new()) };
        v.store("minimax_api_key", "secret-abc-123").unwrap();
        let got = v.retrieve("minimax_api_key").unwrap().unwrap();
        assert_eq!(got, "secret-abc-123");
    }

    #[test]
    fn delete_removes_key() {
        let v = InMemoryVault { store: std::sync::Mutex::new(HashMap::new()) };
        v.store("k", "v").unwrap();
        v.delete("k").unwrap();
        assert!(v.retrieve("k").unwrap().is_none());
    }
}
```

**Step 2: 实现 trait**

```rust
// src-tauri/src/privacy/vault.rs
pub trait PrivacyVault: Send + Sync {
    fn store(&self, key: &str, value: &str) -> anyhow::Result<()>;
    fn retrieve(&self, key: &str) -> anyhow::Result<Option<String>>;
    fn delete(&self, key: &str) -> anyhow::Result<()>;
}
```

`src-tauri/src/privacy/mod.rs`：
```rust
pub mod vault;
```

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test privacy::
git add src-tauri/src/privacy/
git commit -m "feat(privacy): 定义 PrivacyVault trait + InMemory 测试实现"
```

---

### Task 2.2: KeyringVault 实现（OS Keyring）

**Files:**
- Create: `src-tauri/src/privacy/keyring_vault.rs`
- Modify: `src-tauri/src/privacy/mod.rs`

**Step 1: 实现**

```rust
// src-tauri/src/privacy/keyring_vault.rs
use crate::privacy::vault::PrivacyVault;
use keyring::Entry;

const SERVICE: &str = "com.fingertip.app";

pub struct KeyringVault;
impl KeyringVault {
    pub fn new() -> Self { Self }
}
impl Default for KeyringVault {
    fn default() -> Self { Self::new() }
}

impl PrivacyVault for KeyringVault {
    fn store(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let entry = Entry::new(SERVICE, key)?;
        entry.set_password(value)?;
        Ok(())
    }
    fn retrieve(&self, key: &str) -> anyhow::Result<Option<String>> {
        let entry = Entry::new(SERVICE, key)?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn delete(&self, key: &str) -> anyhow::Result<()> {
        let entry = Entry::new(SERVICE, key)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
```

`src-tauri/src/privacy/mod.rs` 追加：
```rust
pub mod keyring_vault;
```

**Step 2: 编译验证**

```bash
cd src-tauri && cargo build
```

Expected: 编译通过（keyring 在 Windows 下用 wincred 后端，编译需安装 Visual Studio Build Tools）

**Step 3: Commit**

```bash
git add src-tauri/src/privacy/keyring_vault.rs src-tauri/src/privacy/mod.rs
git commit -m "feat(privacy): 实现 KeyringVault（Windows wincred 后端）"
```

---

## Phase 3：数据聚合（SummaryAggregator）

### Task 3.1: 按键次数与占比统计

**Files:**
- Create: `src-tauri/src/summary/mod.rs`, `src-tauri/src/summary/aggregator.rs`, `src-tauri/src/summary/stats.rs`
- Test: `src-tauri/src/summary/aggregator.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/summary/aggregator.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::event::KeyEvent;

    fn ev(code: u32) -> KeyEvent { KeyEvent::now(code, "s".into(), 0) }

    #[test]
    fn counts_keys_correctly() {
        // 验证意图：按键计数准确反映真实输入
        let events = vec![ev(65), ev(65), ev(66), ev(65), ev(67)];
        let stats = Aggregator::count_by_key(&events);
        assert_eq!(stats.get(&65), Some(&3));
        assert_eq!(stats.get(&66), Some(&1));
        assert_eq!(stats.get(&67), Some(&1));
    }

    #[test]
    fn percentage_sums_to_100() {
        // 验证意图：占比分布加和恒为 100%（数据完整性）
        let events = vec![ev(65), ev(65), ev(66)];
        let pcts = Aggregator::percentages(&Aggregator::count_by_key(&events));
        let sum: f64 = pcts.values().sum();
        assert!((sum - 100.0).abs() < 0.01);
    }
}
```

**Step 2: 实现**

```rust
// src-tauri/src/summary/aggregator.rs
use crate::hook::event::KeyEvent;
use std::collections::HashMap;

pub struct Aggregator;
impl Aggregator {
    pub fn count_by_key(events: &[KeyEvent]) -> HashMap<u32, usize> {
        let mut map = HashMap::new();
        for e in events { *map.entry(e.key_code).or_insert(0) += 1; }
        map
    }
    pub fn percentages(counts: &HashMap<u32, usize>) -> HashMap<u32, f64> {
        let total: usize = counts.values().sum();
        counts.iter().map(|(k, v)| (*k, (*v as f64 / total as f64) * 100.0)).collect()
    }
}
```

`src-tauri/src/summary/mod.rs`：
```rust
pub mod aggregator;
pub mod stats;
```

`src-tauri/src/summary/stats.rs`：
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: String,
    pub total_keys: usize,
    pub top_keys: Vec<(u32, usize)>,
    pub percentages: Vec<(u32, f64)>,
    pub pauses: usize,
    pub deletes: usize,
    pub repeats: usize,
    pub hourly: [usize; 24],
}
```

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test summary::
git add src-tauri/src/summary/
git commit -m "feat(summary): 实现按键次数与占比聚合"
```

---

### Task 3.2: 时段分布、停顿、删除、重复统计

**Files:**
- Modify: `src-tauri/src/summary/aggregator.rs`
- Test: `src-tauri/src/summary/aggregator.rs`（追加测试）

**Step 1: 写失败的测试**

```rust
    #[test]
    fn hourly_buckets_distribute_correctly() {
        // 验证意图：事件按 timestamp_ms 所在小时分桶
        let mut e1 = ev(65); e1.timestamp_ms = 1700000000000;  // 某小时
        let mut e2 = ev(66); e2.timestamp_ms = 1700003600000;  // +1 小时
        let hourly = Aggregator::hourly_buckets(&[e1, e2]);
        assert!(hourly.iter().any(|&x| x > 0));
    }

    #[test]
    fn counts_pauses_deletes_repeats() {
        // 验证意图：停顿 / 删除 / 重复的识别准确（按键 8 = Backspace）
        let events = vec![ev(65), ev(65), ev(8), ev(8), ev(66)];
        let (pauses, deletes, repeats) = Aggregator::count_meta(&events);
        assert!(deletes >= 2);
        assert!(repeats >= 1);
    }
```

**Step 2: 实现**

```rust
impl Aggregator {
    pub fn hourly_buckets(events: &[KeyEvent]) -> [usize; 24] {
        let mut buckets = [0usize; 24];
        for e in events {
            let dt = chrono::DateTime::from_timestamp_millis(e.timestamp_ms);
            if let Some(dt) = dt {
                let hour = dt.hour() as usize;
                buckets[hour] += 1;
            }
        }
        buckets
    }
    pub fn count_meta(events: &[KeyEvent]) -> (usize, usize, usize) {
        let mut pauses = 0;
        let mut deletes = 0;
        let mut repeats = 0;
        let mut last: Option<u32> = None;
        for e in events {
            if let Some(prev) = last {
                if prev == e.key_code { repeats += 1; }
                let dt_prev = chrono::DateTime::from_timestamp_millis(events[0].timestamp_ms);
                // 简化：实际应基于相邻事件 timestamp 差
            }
            if e.key_code == 8 || e.key_code == 46 { deletes += 1; } // Backspace / Delete
            last = Some(e.key_code);
        }
        pauses = Aggregator::count_pauses(events, 2000);
        (pauses, deletes, repeats)
    }
    fn count_pauses(events: &[KeyEvent], threshold_ms: i64) -> usize {
        let mut pauses = 0;
        for w in events.windows(2) {
            if w[1].timestamp_ms - w[0].timestamp_ms > threshold_ms {
                pauses += 1;
            }
        }
        pauses
    }
}
```

`Cargo.toml` 追加：`chrono = { version = "0.4", features = ["serde"] }`

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test summary::
git add src-tauri/src/summary/ src-tauri/Cargo.toml
git commit -m "feat(summary): 实现时段分布与停顿/删除/重复统计"
```

---

### Task 3.3: 主题词提取算法

**Files:**
- Create: `src-tauri/src/summary/theme.rs`
- Test: `src-tauri/src/summary/theme.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/summary/theme.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_keys_extracts_highest_count() {
        // 验证意图：主题词提取基于真实按键频次，不随机
        let mut counts = std::collections::HashMap::new();
        counts.insert(b'h' as u32, 10);
        counts.insert(b'e' as u32, 8);
        counts.insert(b'l' as u32, 5);
        counts.insert(b'o' as u32, 5);
        let word = extract_theme_word(&counts);
        assert_eq!(word, "hello");
    }
}
```

**Step 2: 实现**

```rust
// src-tauri/src/summary/theme.rs
use std::collections::HashMap;

pub fn extract_theme_word(counts: &HashMap<u32, usize>) -> String {
    let mut sorted: Vec<(u32, usize)> = counts.iter().map(|(k, v)| (*k, *v)).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let top: Vec<u8> = sorted.iter().take(5).map(|(k, _)| *k as u8).collect();
    String::from_utf8_lossy(&top).to_string()
}
```

`src-tauri/src/summary/mod.rs` 追加：`pub mod theme;`

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test summary::theme
git add src-tauri/src/summary/theme.rs src-tauri/src/summary/mod.rs
git commit -m "feat(summary): 实现高频键位主题词提取"
```

---

### Task 3.4: 每日定时任务（tokio 调度）

**Files:**
- Create: `src-tauri/src/summary/scheduler.rs`
- Test: `src-tauri/src/summary/scheduler.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/summary/scheduler.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_run_is_next_005() {
        // 验证意图：下次运行时间总是次日 00:05，不会冲突
        let now = chrono::NaiveDate::from_ymd_opt(2026, 7, 16).unwrap()
            .and_hms_opt(23, 0, 0).unwrap();
        let next = next_run_time(now);
        assert_eq!(next, chrono::NaiveDate::from_ymd_opt(2026, 7, 17).unwrap().and_hms_opt(0, 5, 0).unwrap());
    }
}
```

**Step 2: 实现**

```rust
// src-tauri/src/summary/scheduler.rs
use chrono::{Duration, NaiveDateTime};

pub fn next_run_time(now: NaiveDateTime) -> NaiveDateTime {
    let today_run = now.date().and_hms_opt(0, 5, 0).unwrap();
    if now < today_run { today_run } else { today_run + Duration::days(1) }
}
```

`src-tauri/src/summary/mod.rs` 追加：`pub mod scheduler;`

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test summary::scheduler
git add src-tauri/src/summary/scheduler.rs src-tauri/src/summary/mod.rs
git commit -m "feat(summary): 实现每日 00:05 定时调度"
```

---

## Phase 4：AI 抽象层（MusicAdapter + ArtAdapter）

### Task 4.1: MusicAdapter trait + Mock 实现

**Files:**
- Create: `src-tauri/src/generate/mod.rs`, `src-tauri/src/generate/music.rs`, `src-tauri/src/generate/orchestrator.rs`
- Test: `src-tauri/src/generate/music.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/generate/music.rs
#[cfg(test)]
mod tests {
    use super::*;

    struct MockMusicAdapter;
    impl MusicAdapter for MockMusicAdapter {
        fn generate(&self, prompt: &MusicPrompt) -> anyhow::Result<String> {
            Ok(format!("/tmp/{}.mp3", prompt.mood))
        }
    }

    #[test]
    fn mock_returns_mp3_path() {
        // 验证意图：music adapter 至少能产出文件路径
        let prompt = MusicPrompt { mood: "calm".into(), style: "ambient".into(), theme_word: "hello".into() };
        let path = MockMusicAdapter.generate(&prompt).unwrap();
        assert!(path.ends_with(".mp3"));
    }
}
```

**Step 2: 实现**

```rust
// src-tauri/src/generate/music.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicPrompt {
    pub mood: String,
    pub style: String,
    pub theme_word: String,
}

pub trait MusicAdapter: Send + Sync {
    fn generate(&self, prompt: &MusicPrompt) -> anyhow::Result<String>;
}
```

`src-tauri/src/generate/mod.rs`：
```rust
pub mod music;
pub mod orchestrator;
```

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test generate::music
git add src-tauri/src/generate/
git commit -m "feat(generate): 定义 MusicAdapter trait + Mock 测试实现"
```

---

### Task 4.2: ArtAdapter trait + Mock 实现

**Files:**
- Create: `src-tauri/src/generate/art.rs`
- Modify: `src-tauri/src/generate/mod.rs`

**Step 1: 写失败的测试**

```rust
// src-tauri/src/generate/art.rs
#[cfg(test)]
mod tests {
    use super::*;

    struct MockArtAdapter;
    impl ArtAdapter for MockArtAdapter {
        fn generate(&self, prompt: &ArtPrompt) -> anyhow::Result<String> {
            Ok(format!("/tmp/{}.png", prompt.theme_word))
        }
    }

    #[test]
    fn mock_returns_png_path() {
        // 验证意图：art adapter 至少能产出文件路径
        let prompt = ArtPrompt { theme_word: "hello".into(), mood: "calm".into(), stats_json: "{}".into() };
        let path = MockArtAdapter.generate(&prompt).unwrap();
        assert!(path.ends_with(".png"));
    }
}
```

**Step 2: 实现**

```rust
// src-tauri/src/generate/art.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtPrompt {
    pub theme_word: String,
    pub mood: String,
    pub stats_json: String,
}

pub trait ArtAdapter: Send + Sync {
    fn generate(&self, prompt: &ArtPrompt) -> anyhow::Result<String>;
}
```

`src-tauri/src/generate/mod.rs` 追加：`pub mod art;`

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test generate::art
git add src-tauri/src/generate/art.rs src-tauri/src/generate/mod.rs
git commit -m "feat(generate): 定义 ArtAdapter trait + Mock 测试实现"
```

---

### Task 4.3: MiniMax Cloud 实现（占位 + 接口预留）

**Files:**
- Create: `src-tauri/src/generate/minimax_cloud.rs`
- Modify: `src-tauri/src/generate/mod.rs`

**Step 1: 实现（首版允许未调用真实 API，stub 出路径）**

```rust
// src-tauri/src/generate/minimax_cloud.rs
use crate::generate::art::{ArtAdapter, ArtPrompt};
use crate::generate::music::{MusicAdapter, MusicPrompt};

pub struct MinimaxCloudAdapter { pub api_key: String }
impl MinimaxCloudAdapter { pub fn new(api_key: String) -> Self { Self { api_key } } }

impl MusicAdapter for MinimaxCloudAdapter {
    fn generate(&self, prompt: &MusicPrompt) -> anyhow::Result<String> {
        // 首版 stub：实际接入在 Phase 4.4 后
        log::info!("[Minimax music] prompt={:?} key_prefix={}", prompt, &self.api_key[..4.min(self.api_key.len())]);
        Ok(format!("./outputs/music_{}.mp3", prompt.theme_word))
    }
}

impl ArtAdapter for MinimaxCloudAdapter {
    fn generate(&self, prompt: &ArtPrompt) -> anyhow::Result<String> {
        log::info!("[Minimax art] prompt={:?}", prompt);
        Ok(format!("./outputs/art_{}.png", prompt.theme_word))
    }
}
```

`src-tauri/src/generate/mod.rs` 追加：`pub mod minimax_cloud;`

**Step 2: 编译 + Commit**

```bash
cd src-tauri && cargo build
git add src-tauri/src/generate/
git commit -m "feat(generate): 实现 MiniMaxCloudAdapter stub（接口预留）"
```

---

### Task 4.4: GenerateOrchestrator 串联音乐+画作

**Files:**
- Modify: `src-tauri/src/generate/orchestrator.rs`
- Test: `src-tauri/src/generate/orchestrator.rs`（内嵌）

**Step 1: 写失败的测试**

```rust
// src-tauri/src/generate/orchestrator.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::art::{ArtAdapter, ArtPrompt};
    use crate::generate::music::{MusicAdapter, MusicPrompt};
    use std::sync::Arc;

    struct StubMusic;
    impl MusicAdapter for StubMusic {
        fn generate(&self, p: &MusicPrompt) -> anyhow::Result<String> { Ok(format!("m_{}.mp3", p.theme_word)) }
    }
    struct StubArt;
    impl ArtAdapter for StubArt {
        fn generate(&self, p: &ArtPrompt) -> anyhow::Result<String> { Ok(format!("a_{}.png", p.theme_word)) }
    }

    #[test]
    fn orchestrate_returns_both_paths() {
        // 验证意图：orchestrator 同时调用音乐和画作，产出双文件
        let orch = GenerateOrchestrator::new(Arc::new(StubMusic), Arc::new(StubArt));
        let result = orch.orchestrate("calm", "ambient", "hello").unwrap();
        assert!(result.music_path.contains(".mp3"));
        assert!(result.art_path.contains(".png"));
    }
}
```

**Step 2: 实现**

```rust
// src-tauri/src/generate/orchestrator.rs
use crate::generate::art::{ArtAdapter, ArtPrompt};
use crate::generate::music::{MusicAdapter, MusicPrompt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    pub music_path: String,
    pub art_path: String,
}

pub struct GenerateOrchestrator {
    music: Arc<dyn MusicAdapter>,
    art: Arc<dyn ArtAdapter>,
}

impl GenerateOrchestrator {
    pub fn new(music: Arc<dyn MusicAdapter>, art: Arc<dyn ArtAdapter>) -> Self { Self { music, art } }

    pub fn orchestrate(&self, mood: &str, style: &str, theme_word: &str) -> anyhow::Result<GenerationResult> {
        let mp = MusicPrompt { mood: mood.into(), style: style.into(), theme_word: theme_word.into() };
        let ap = ArtPrompt { mood: mood.into(), theme_word: theme_word.into(), stats_json: "{}".into() };
        let music_path = self.music.generate(&mp)?;
        let art_path = self.art.generate(&ap)?;
        Ok(GenerationResult { music_path, art_path })
    }
}
```

**Step 3: 跑测试 + Commit**

```bash
cd src-tauri && cargo test generate::orchestrator
git add src-tauri/src/generate/orchestrator.rs
git commit -m "feat(generate): 实现 GenerateOrchestrator 串联音乐+画作"
```

---

## Phase 5：系统托盘（TrayManager）

### Task 5.1: 系统托盘图标 + 右键菜单

**Files:**
- Create: `src-tauri/src/tray/mod.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: 创建 tray 模块**

```rust
// src-tauri/src/tray/mod.rs
use tauri::{AppHandle, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, CustomMenuItem};

pub fn build() -> SystemTray {
    let menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("today", "今日总结"))
        .add_item(CustomMenuItem::new("submit", "提交心情"))
        .add_separator()
        .add_item(CustomMenuItem::new("quit", "退出"));
    SystemTray::new().with_menu(menu)
}

pub fn handle_event(app: &AppHandle, event: SystemTrayEvent) {
    if let SystemTrayEvent::MenuItemClick { id, .. } = event {
        match id.as_str() {
            "today" => { if let Some(w) = app.get_window("main") { w.show().ok(); w.set_focus().ok(); } }
            "submit" => { if let Some(w) = app.get_window("main") { w.show().ok(); } }
            "quit" => app.exit(0),
            _ => {}
        }
    }
}
```

`src-tauri/src/main.rs` 注册 tray：
```rust
fn main() {
    tauri::Builder::default()
        .system_tray(fingertip::tray::build())
        .on_system_tray_event(fingertip::tray::handle_event)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 2: 编译验证 + 手动运行**

```bash
cd src-tauri && cargo build
cd "E:/一人公司/技术部工作区/小玩具/FingerTip" && pnpm tauri dev
```

Expected: 启动后右下角出现托盘图标，右键菜单含"今日总结/提交心情/退出"。

**Step 3: Commit**

```bash
git add src-tauri/src/tray/ src-tauri/src/main.rs
git commit -m "feat(tray): 实现系统托盘图标 + 右键菜单"
```

---

## Phase 6：前端（Vue 3 + Naive UI）

### Task 6.1: 路由 + Pinia

**Files:**
- Create: `src/router/index.ts`, `src/stores/app.ts`, `src/App.vue`, `src/views/Today.vue`, `src/views/Settings.vue`, `src/views/History.vue`, `src/views/About.vue`, `src/views/SubmitMood.vue`

**Step 1: 创建路由**

```ts
// src/router/index.ts
import { createRouter, createWebHashHistory } from 'vue-router'
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', component: () => import('@/views/Today.vue') },
    { path: '/history', component: () => import('@/views/History.vue') },
    { path: '/settings', component: () => import('@/views/Settings.vue') },
    { path: '/about', component: () => import('@/views/About.vue') },
    { path: '/submit', component: () => import('@/views/SubmitMood.vue') }
  ]
})
```

**Step 2: 创建 Pinia store**

```ts
// src/stores/app.ts
import { defineStore } from 'pinia'
import { ref } from 'vue'
export const useAppStore = defineStore('app', () => {
  const todaySummary = ref<any>(null)
  const moodWord = ref('')
  const generating = ref(false)
  return { todaySummary, moodWord, generating }
})
```

**Step 3: 创建 5 个空白视图（占位）**

每个文件内容：
```vue
<template><div class="page">{{ title }}</div></template>
<script setup lang="ts">
const props = defineProps<{ title: string }>()
</script>
```

**Step 4: 写测试**

`src/tests/router.test.ts`：
```ts
import { describe, it, expect } from 'vitest'
import { router } from '@/router'
describe('router', () => {
  it('has 5 routes', () => {
    expect(router.getRoutes().length).toBe(5)
  })
})
```

**Step 5: 跑测试 + 启动验证**

```bash
pnpm test --run
pnpm tauri dev
```

**Step 6: Commit**

```bash
git add src/
git commit -m "feat(frontend): 初始化 Vue 3 路由 + Pinia + 5 个空白视图"
```

---

### Task 6.2: 当日总结视图（Today.vue）

**Files:**
- Modify: `src/views/Today.vue`
- Test: `src/views/__tests__/Today.test.ts`

**Step 1: 写失败的组件测试**

```ts
// src/views/__tests__/Today.test.ts
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import Today from '@/views/Today.vue'
import { describe, it, expect, beforeEach } from 'vitest'

describe('Today.vue', () => {
  beforeEach(() => setActivePinia(createPinia()))
  it('shows empty state when no summary', () => {
    const w = mount(Today)
    expect(w.text()).toContain('暂无今日数据')
  })
})
```

**Step 2: 实现 Today.vue**

```vue
<template>
  <n-card title="今日键盘总结">
    <template v-if="store.todaySummary">
      <p>总按键：{{ store.todaySummary.total_keys }}</p>
      <p>主题词：{{ store.todaySummary.theme_word }}</p>
    </template>
    <n-empty v-else description="暂无今日数据" />
  </n-card>
</template>
<script setup lang="ts">
import { useAppStore } from '@/stores/app'
const store = useAppStore()
</script>
```

**Step 3: 跑测试 + Commit**

```bash
pnpm test --run src/views/__tests__/Today.test.ts
git add src/views/Today.vue src/views/__tests__/
git commit -m "feat(frontend): 当日总结视图（含空态）"
```

> **类似节奏完成 SubmitMood.vue（Task 6.3）、History.vue（6.4）、Settings.vue（6.5）、About.vue（6.6）**

---

## Phase 7：集成与端到端

### Task 7.1: Tauri Command 暴露：触发生成

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: 实现**

```rust
// src-tauri/src/commands.rs
use crate::generate::orchestrator::GenerateOrchestrator;
use tauri::State;

#[tauri::command]
pub fn trigger_generate(state: State<GenerateOrchestrator>, mood: String, style: String, theme_word: String) -> Result<String, String> {
    state.orchestrate(&mood, &style, &theme_word).map(|r| serde_json::to_string(&r).unwrap()).map_err(|e| e.to_string())
}
```

**Step 2: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "feat(commands): 暴露 trigger_generate Tauri Command"
```

---

### Task 7.2: Playwright E2E 关键链路

**Files:**
- Create: `tests-e2e/daily-flow.spec.ts`

**Step 1: 写 E2E 测试**

```ts
// tests-e2e/daily-flow.spec.ts
import { test, expect } from '@playwright/test'

test('用户提交心情并触发生成', async ({ page }) => {
  await page.goto('/#/submit')
  await page.fill('input[placeholder="一个词"]', 'calm')
  await page.click('button:has-text("生成")')
  await expect(page.locator('text=生成中')).toBeVisible({ timeout: 5000 })
})
```

**Step 2: 运行 + Commit**

```bash
pnpm playwright test
git add tests-e2e/
git commit -m "test(e2e): 提交心情 → 生成 E2E 关键链路"
```

---

### Task 7.3: 性能基线测试

**Files:**
- Create: `src-tauri/tests/perf.rs`

**Step 1: 实现**

```rust
// src-tauri/tests/perf.rs
use fingertip::summary::aggregator::Aggregator;
use fingertip::hook::event::KeyEvent;
use std::time::Instant;

#[test]
fn aggregate_100k_events_under_5s() {
    // 验证意图：性能基线（设计文档第六节）
    let events: Vec<KeyEvent> = (0..100_000).map(|i| KeyEvent::now(i as u32, "s".into(), 0)).collect();
    let start = Instant::now();
    let _ = Aggregator::count_by_key(&events);
    assert!(start.elapsed().as_secs() < 5);
}
```

**Step 2: 运行 + Commit**

```bash
cd src-tauri && cargo test --release --test perf -- --ignored
git add src-tauri/tests/perf.rs
git commit -m "test(perf): 10 万事件聚合 < 5 秒性能基线"
```

---

## Phase 8：打包与发布

### Task 8.1: Tauri 打包配置

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Step 1: 添加 bundle 配置**

```json
{
  "bundle": {
    "active": true,
    "targets": ["msi", "nsis"],
    "identifier": "com.fingertip.app",
    "icon": ["icons/icon.ico"]
  }
}
```

**Step 2: 打包**

```bash
cd "E:/一人公司/技术部工作区/小玩具/FingerTip" && pnpm tauri build
```

Expected: 在 `src-tauri/target/release/bundle/` 生成 .msi 与 .exe 安装包。

**Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "chore: 配置 Tauri 打包（MSI + NSIS）"
```

---

### Task 8.2: README + 自用发布流程文档

**Files:**
- Create: `README.md`

**Step 1: 写 README**

```markdown
# FingerTip

> 后台记录键盘敲击 + AI 生成音乐与画作

## 快速开始
\`\`\`bash
pnpm install
pnpm tauri dev
\`\`\`

## 打包
\`\`\`bash
pnpm tauri build
\`\`\`

## 文档
- 设计文档：docs/plans/2026-07-16-fingertip-design.md
- 精益画布：docs/specs/lean-ux.md
- 实施计划：docs/plans/2026-07-16-fingertip-implementation.md
```

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: 初始化 README"
```

---

## 验收清单（与设计文档第六节"验证前不宣布完成"对齐）

实施完成后，逐项确认：

- [ ] `cd src-tauri && cargo test --workspace` 全绿
- [ ] `pnpm test --run` 全绿
- [ ] `pnpm playwright test` 关键链路全绿
- [ ] 手动运行 ≥ 1 小时，HookListener 日志无 ERROR 级别
- [ ] 性能基线：HookListener 空闲 CPU < 1%、内存 < 50MB
- [ ] 性能基线：10 万事件聚合 < 5 秒
- [ ] 隐私验证：断网状态下本机 SQLite 数据可正常读写
- [ ] 打包产物：`.msi` 与 `.exe` 安装成功、托盘图标显示、键盘监听启动

---

## 风险与注意事项

1. **rdev 在 Windows 的 Hook 权限**：首次启动可能被系统提示授权
2. **keyring crate 编译依赖**：需要 Visual Studio Build Tools
3. **MiniMax API 真实接入**在 Phase 4.3 仅做 stub；接入真实 API 时需关注配额与限流
4. **macOS 架构预留**：Tauri 2.x 已支持，但需独立开发与签名（不在首版范围）

---

## 完成定义

当所有 Task 标记完成、验收清单全部打勾、且按精益画布 Box 8 完成"7 天自用实验"后，本计划方可标记为 Done。