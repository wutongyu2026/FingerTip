// Phase 0 占位入口：仅启动 Tauri builder，加载 lib.rs 中的 run() 函数
// 后续 Phase 会：
//   - 注册系统托盘（Task 0.3）
//   - 注册键盘 Hook 命令（Phase 1, HookListener）
//   - 暴露 SQLite IPC（Phase 2）
//   - 暴露 AI Adapter IPC（Phase 3）

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    fingertip_lib::run();
}