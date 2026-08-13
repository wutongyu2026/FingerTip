//! 隐私存储抽象：管理敏感字符串（API Key、Token 等）。
//!
//! 验证意图：业务逻辑不直接依赖 OS Keyring，
//! 可在测试中用 InMemory 实现，生产用 keyring crate。

use std::collections::HashMap;
use std::sync::Mutex;

pub trait PrivacyVault: Send + Sync {
    fn store(&self, key: &str, value: &str) -> anyhow::Result<()>;
    fn retrieve(&self, key: &str) -> anyhow::Result<Option<String>>;
    fn delete(&self, key: &str) -> anyhow::Result<()>;
}

/// 测试用 InMemory 实现：仅用于单元/集成测试，绝不用于生产
pub struct InMemoryVault {
    store: Mutex<HashMap<String, String>>,
}

impl InMemoryVault {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryVault {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyVault for InMemoryVault {
    fn store(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(key.to_string(), value.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_round_trip() {
        // 验证意图：API Key 写入后能取回原值（不截断、不变形）
        let v = InMemoryVault::new();
        v.store("minimax_api_key", "secret-abc-123").unwrap();
        let got = v.retrieve("minimax_api_key").unwrap().unwrap();
        assert_eq!(got, "secret-abc-123");
    }

    #[test]
    fn delete_removes_key() {
        // 验证意图：delete 后 retrieve 应返回 None
        let v = InMemoryVault::new();
        v.store("k", "v").unwrap();
        v.delete("k").unwrap();
        assert!(v.retrieve("k").unwrap().is_none());
    }

    #[test]
    fn retrieve_missing_key_returns_none() {
        // 验证意图：未存储的 key 应返回 None（不是 panic / 不是空字符串）
        let v = InMemoryVault::new();
        assert!(v.retrieve("never_stored").unwrap().is_none());
    }

    #[test]
    fn overwriting_key_works() {
        // 验证意图：同 key 二次写入应覆盖（不是新增 / 不是报错）
        let v = InMemoryVault::new();
        v.store("k", "v1").unwrap();
        v.store("k", "v2").unwrap();
        assert_eq!(v.retrieve("k").unwrap().unwrap(), "v2");
    }
}