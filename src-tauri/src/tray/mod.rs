//! 系统托盘：让 FingerTip 后台常驻时不打扰用户。
//!
//! 验证意图：托盘图标 + 菜单 + 窗口显隐 + 退出 + 跳路由，最小可用。
//!
//! v0.3.3: 菜单「今日总结 / 提交心情」点击后：
//!   1. show + focus 窗口（同 v0.3.0 行为）
//!   2. emit('navigate', path) 让前端 App.vue 监听 + router.push(path)
//! 这样从系统托盘可直接跳到对应页面（之前两者行为完全一样）

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

// 编译时嵌入托盘图标（PNG 格式，跨平台）
// 选择 32x32.png 是因为托盘在 Windows/macOS/Linux 上自动缩放到 16x16/24x24
// 修复原 bug：之前没显式设置图标导致空方块（截图所示）
const TRAY_ICON_BYTES: &[u8] = include_bytes!("../../icons/32x32.png");

const MENU_TODAY: &str = "today";
const MENU_SUBMIT: &str = "submit";
const MENU_QUIT: &str = "quit";

/// 跨前后端的导航事件名（前端 App.vue 监听 + 跳路由）
const NAVIGATE_EVENT: &str = "navigate";

/// 构建托盘（菜单 + 图标）
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let today = MenuItem::with_id(app, MENU_TODAY, "今日总结", true, None::<&str>)?;
    let submit = MenuItem::with_id(app, MENU_SUBMIT, "提交心情", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&today, &submit, &quit])?;

    // 编译期嵌入的 owned Icon（生命周期 'static）
    // Tauri TrayIconBuilder::icon() 要求 Image<'_>，'static 来源最稳
    let tray_icon = Image::from_bytes(TRAY_ICON_BYTES)?;

    TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("FingerTip — 记录键盘节奏")
        .icon(tray_icon)
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            // 左键单击 = 切换主窗口显隐（Windows 习惯）
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        MENU_TODAY => {
            show_and_focus(app);
            navigate(app, "/");
        }
        MENU_SUBMIT => {
            show_and_focus(app);
            navigate(app, "/submit");
        }
        MENU_QUIT => {
            app.exit(0);
        }
        _ => {}
    }
}

/// 显示 + 聚焦主窗口（v0.3.0 行为）
fn show_and_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// emit 'navigate' 事件给前端，前端 App.vue 监听后 router.push(path)
///
/// emit 失败（窗口未创建 / 权限）只 log，不影响主流程。
fn navigate(app: &AppHandle, path: &str) {
    if let Err(e) = app.emit(NAVIGATE_EVENT, path) {
        log::warn!("tray emit navigate({}) failed: {:?}", path, e);
    }
}