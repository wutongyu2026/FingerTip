# FingerTip v0.2.5

## 🔥 修复：开机自启完全失效（panic 退出无报错）

### 根因

`src-tauri/src/lib.rs` 在 `run()` 顶层用 `std::env::var("FINGERTIP_DATA_DIR").unwrap_or(".")` 解析 DB 路径。回退到 `"."` 当前工作目录——但 **Windows HKCU Run 键启动的进程 cwd 永远是 `C:\Windows\System32\`**（普通用户无写权限）。

结果：
1. 进程启动 → 解析 cwd = `C:\Windows\System32\`
2. 尝试 `init_at("C:\Windows\System32\fingertip.db")` → OS 拒绝
3. `expect("init DB")` **panic**
4. 进程闪退，`EventLog` 因为 `windows_subsystem="windows"` 没 console 没 GUI 也不留 trace
5. 看起来"完全没启动"，但注册表项是好的

**bug 跟 build 类型（debug/release/安装包）无关**——只要是 HKCU Run 方式启动，就会触发。

### 修复

将 DB 初始化整段下沉到 `setup()` 闭包内，用 Tauri 推荐的 `app.path().app_data_dir()` 解析路径：

```rust
.setup({
    let orchestrator = orchestrator.clone();
    move |app| {
        let app_data_dir = app.path().app_data_dir()
            .expect("failed to resolve app_data_dir");
        std::fs::create_dir_all(&app_data_dir).ok();
        let db_path = app_data_dir.join("fingertip.db");
        log::info!("FingerTip DB: {}", db_path.display());
        let conn = db::init_at(&db_path).expect("init DB");
        let conn = Arc::new(Mutex::new(conn));
        let state = AppState { orchestrator, conn: conn.clone() };
        app.manage(state);
        // ... existing tray / scheduler / hook setup
    }
})
```

DB 落点：
- Windows: `%APPDATA%\com.fingertip.app\fingertip.db`
- macOS: `~/Library/Application Support/com.fingertip.app/fingertip.db`
- Linux: `~/.local/share/com.fingertip.app/fingertip.db`

### 📦 安装包发布（首次）

```
src-tauri/target/release/bundle/nsis/FingerTip_0.2.5_x64-setup.exe       7.4 MB
src-tauri/target/release/bundle/msi/FingerTip_0.2.5_x64_en-US.msi       13.0 MB
```

用户安装流程：
1. 双击 `FingerTip_0.2.5_x64-setup.exe`
2. NSIS 引导：选安装路径 → 勾选「开机自启动」（建议勾）
3. 安装完成 → Start Menu 出现「FingerTip」
4. 重启电脑 → 托盘出现 FingerTip 图标 → 后台静默记录 ✅

### 🧪 验证

| 维度 | 状态 |
|---|---|
| Rust `cargo test --lib` | **111 passed**（修复未引入回归）|
| `pnpm tauri build` | ✅ 完整产物 2 份 bundle |
| 路径解析 | `app.path().app_data_dir()` 来自 Tauri 2.11.5 `desktop.rs:247` |
| panic 路径 | 不再可能（`expect("...")` 在 setup 内，失败也只是 Tauri 启动失败，至少有日志）|

### 📜 Commit 链

```
[v0.2.5] fix: 自启动 panic（DB 路径下沉）+ 首次正式出包
6a9a00e fix: v0.2.4 — 时区 / 自启提示 / 主题词 3 项 hotfix
27af82c fix(artworks): v0.2.3 hotfix — 作品页 UI 间距
```

### 📂 改动文件

```
src-tauri/src/lib.rs          # DB init 下沉到 setup 闭包 + app.path().app_data_dir()
package.json                 # 0.2.4 → 0.2.5
src-tauri/Cargo.toml         # version 0.2.4 → 0.2.5
src-tauri/tauri.conf.json    # version 0.2.4 → 0.2.5
src/views/About.vue          # FingerTip v0.2.4 → v0.2.5
CHANGELOG.md                 # 加 [0.2.5] 段
```

### ⚠️ 已知：bundle identifier 警告

NSIS 打包提示 `com.fingertip.app` 以 `.app` 结尾与 macOS application bundle 冲突。**non-fatal warning，build 成功**。下次发版前建议改为 `com.fingertip.desktop` 之类的安全 identifier。

### 🚀 如何验证自启动确实修好了

1. 安装 `FingerTip_0.2.5_x64-setup.exe`
2. 启动一次，确认托盘出现 FingerTip 图标
3. 右键托盘 → 退出（先验证可退出，否则死锁）
4. 重启电脑
5. 登录后看托盘区是否出现 FingerTip 图标 ← 现在应该出现
6. 按几下键盘、打开 About 页，确认有按键记录
