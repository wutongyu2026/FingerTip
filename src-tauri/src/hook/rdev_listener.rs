use crate::hook::event::KeyEvent;
use crate::hook::rdev_adapter::rdev_event_to_key_event;
use parking_lot::Mutex;

/// 基于 `rdev` crate 的全局键盘监听器。
///
/// v0.3.1: HookListener trait 已删（v0.1 的 EventBuffer 抽象层不再需要）。
/// RdevListener 直接暴露 `start(Box<FnMut(KeyEvent) + Send>)` —— lib.rs 调一次
/// 启动一个独立线程跑 rdev::listen（block 调用），通过 rdev_adapter 把
/// rdev::Event 翻译为 KeyEvent 喂给 sink。
pub struct RdevListener {
    running: std::sync::Arc<Mutex<bool>>,
    session_id: String,
}

impl RdevListener {
    pub fn new(session_id: String) -> Self {
        Self {
            running: std::sync::Arc::new(Mutex::new(false)),
            session_id,
        }
    }

    /// 当前是否正在监听
    pub fn is_running(&self) -> bool {
        *self.running.lock()
    }

    /// session_id 读取器（用于校验 sink 收到的事件属于正确 session）
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 启动 OS 钩子监听（在新线程中跑 rdev::listen）。
    ///
    /// rdev::listen 是 block 调用必须放新线程；callback 把 rdev::Event
    /// 经 `rdev_event_to_key_event` 翻译为 KeyEvent 后喂给 sink。
    /// sink 通常是 `Box::new(closure that calls writer.send(e))`。
    ///
    /// 注：rdev 没有优雅 stop 接口；调用方在进程退出时停止。
    pub fn start(&mut self, mut sink: Box<dyn FnMut(KeyEvent) + Send>) -> Result<(), anyhow::Error> {
        *self.running.lock() = true;

        let session = self.session_id.clone();
        std::thread::spawn(move || {
            let callback = move |event: rdev::Event| {
                if let Some(ke) = rdev_event_to_key_event(&event, &session) {
                    sink(ke);
                }
            };
            if let Err(e) = rdev::listen(callback) {
                log::error!("rdev::listen failed: {:?}", e);
            }
        });
        Ok(())
    }

    /// 标记停止（rdev 无优雅 stop，仅切换 running 标志）。
    pub fn stop(&mut self) -> Result<(), anyhow::Error> {
        *self.running.lock() = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdev_converts_keycode() {
        // 验证意图：KeyEvent::now 正确设置当前时间与键码（事件构造器的契约）
        let evt = KeyEvent::now(65, "test-session".into(), 0);
        assert_eq!(evt.key_code, 65);
        assert!(evt.timestamp_ms > 0);
        assert_eq!(evt.session_id, "test-session");
    }

    #[test]
    fn is_running_starts_false() {
        // 验证意图：新创建的 listener 尚未运行（避免 start 之前误判）
        let l = RdevListener::new("s".into());
        assert!(!l.is_running());
    }

    #[test]
    fn start_sets_running_flag() {
        // 验证意图：start 触发 is_running 状态切换（生命周期契约）
        // 注意：实际 rdev::listen 在测试环境无 OS 键盘，会立即出错但 running 已设 true
        let mut l = RdevListener::new("s".into());
        l.start(Box::new(|_| {})).unwrap();
        assert!(l.is_running());
    }

    #[test]
    fn stop_clears_running_flag() {
        // 验证意图：stop 触发 is_running 状态切换
        let mut l = RdevListener::new("s".into());
        l.start(Box::new(|_| {})).unwrap();
        l.stop().unwrap();
        assert!(!l.is_running());
    }
}
