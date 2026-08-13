//! KeyringVault：基于 OS Keyring 的生产实现。
//!
//! Windows: wincred 后端（Credential Manager）
//! macOS: Keychain
//! Linux: Secret Service（GNOME Keyring / KWallet）

use crate::privacy::vault::PrivacyVault;
use keyring::Entry;

const SERVICE: &str = "com.fingertip.app";

pub struct KeyringVault;

impl KeyringVault {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeyringVault {
    fn default() -> Self {
        Self::new()
    }
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

// 注意：KeyringVault 不在单元测试范围内，因为：
// 1. 它依赖真实 OS 凭据存储（Windows Credential Manager / macOS Keychain）
// 2. CI / dev 环境不一定有交互式凭据存储
// 3. 生产验证在 Phase 8 打包 + 真实运行后由用户手动验证
// InMemoryVault 已在 privacy::vault 测试中覆盖 trait 契约