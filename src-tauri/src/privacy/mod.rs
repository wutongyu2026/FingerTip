// 隐私模块：API Key 加密存储抽象层
//
// v0.3.1 状态：完全未集成（v0.3 删了 MinimaxCloudAdapter 后没有任何代码需要 API Key）。
//
// v0.4 计划：真实云端 AI 接入（musicgen / suno 等）时引入 KeyringVault。
//           - 在 Settings.vue 加"AI Key"输入框 → store → keyring_vault.store()
//           - CloudMusicAdapter / CloudArtAdapter 拿 key → 拼 Authorization header
//           - 单元测试已覆盖 trait 契约（InMemoryVault），生产实现不测（OS 依赖）
//
// 保留理由：
//   1. trait 抽象 + InMemoryVault 测试已成熟，删了下次重写浪费时间
//   2. v0.4 接云是 roadmap 明确项，删了再恢复要重新设计
//   3. 编译进二进制（dead code 不影响 release 体积 —— LTO 会清掉）

pub mod keyring_vault;
pub mod vault;