# FingerTip v0.2.4

## 🐛 Bug 修复 hotfix

### 时区切换首页不变
- 后端 `get_today_hourly` / `get_today_key_count` 命令接受 `offset_minutes` 参数
- 新增 `tz_today_range_ms(offset_minutes)` 纯函数，按用户时区算"今日 0:00"边界
- 前端 `invoke` 调用带 `offsetMinutes: store.timezoneOffsetMinutes`
- 前端 `watch(() => store.timezoneOffsetMinutes, refresh)` —— 切时区立刻重拉数据

### 主题词总是 I,N,A
- 旧实现把 top 5 ASCII 字符直接拼接成"主题词"（"INA" 这种无意义字符串）
- 中文输入时 IME 把拼音首字母作为 key_events，导致高频就是 I/N/A
- 改为输出**可读摘要**：`{最高频字母} · {总按键数}`（如 `I · 303`）
- 加 6 个新单元测试覆盖 INA 高频、单字符、纯修饰键、同 count 排序等边界

### 开机自启动提示
- 注册表项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\FingerTip` 实际存在
- 用户感受不到是因为窗口 `visible: false` + 托盘图标可能被 Win11 折叠
- Settings.vue 加 `isTauri` runtime 检测 + warning 提示
- web 模式下 toggle disabled，避免误操作

## 🧪 验证

| 维度 | 状态 |
|---|---|
| Rust `cargo test --lib` | **111 passed**（105 + 6 theme 新测）|
| 前端 `npm run test` | **68/68** 全过 |
| `vue-tsc --noEmit` | **0 error** |

## 📜 Commit 链

```
6a9a00e fix: v0.2.4 — 时区 / 自启提示 / 主题词 3 项 hotfix
27af82c fix(artworks): v0.2.3 hotfix — 作品页 UI 间距
7217b05 v0.2.2: 时区功能 + 节奏指纹修复 + 音乐 UI 间距
8a67b20 chore: 版本同步 v0.2.0 → v0.2.1
0bb3d30 v0.2.1: Artworks 并排紧凑 + HookWriter 异步落库
dfc922f feat: v0.2 升级 — 实时记录 + 真作品/历史 + 1420
5fdd4a9 feat: 开机自启动 + 静默后台模式
```

## 📂 改动文件（4 source + 5 同步）

```
src-tauri/src/commands.rs        # offset_minutes 参数 + tz_today_range_ms
src-tauri/src/summary/theme.rs   # 输出可读摘要（替代字母拼接）
src/views/Settings.vue           # Tauri 环境检测 + UI 提示
src/views/Today.vue              # watch 触发 refresh + invoke 带 offset
package.json / Cargo.toml / tauri.conf.json / About.vue / CHANGELOG.md
```