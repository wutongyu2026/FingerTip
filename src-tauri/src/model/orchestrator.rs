//! 编排器（Orchestrator）：组装 daily context，拼 prompt，调一次 LLM，解析 JSON。
//!
//! v0.4「编排器 LLM → 专有模型」改造的核心。`generate_now` 调
//! `run_orchestrator(&dyn JsonChat, ctx)` 拿回三条产物（音乐描述 / 图像描述 / 句子），
//! 然后分发到 Step-Audio / MiniMax / SD1.5 等专有模型。LLM 侧具体走本地引擎还是
//! 云端 MiniMax 由路由决策（T2）决定，本模块不关心。
//!
//! 失败处理：编排器拿 `serde_json::Value`（chat_json 已做首层解析），再按契约校验
//! 三字段均存在且非空字符串。失败时**重试 1 次**（网络抖动 / 偶发空响应），仍失败
//! → bail，错误信息含「编排器」与「重试」便于上层 UI 归因。

use crate::model::JsonChat;

/// 编排器输入：当日键盘活动聚合并浓缩的「当日信号」。
///
/// 字段意义见 `DailySummary`（generate 模块）。本结构是「编排器侧」视图，
/// 只挑出 prompt 与统计需要的字段，不耦合 DB 行结构。
#[derive(Debug, Clone)]
pub struct OrchestrationContext {
    /// 当日主题词（来自 `DailySummary.theme_word`）。
    pub theme_word: String,
    /// 当日情绪（`DailySummary.mood`，可能为 None）。
    pub mood: Option<String>,
    /// 风格标签（`DailySummary.style`，如 "ambient"）。
    pub style: String,
    /// 当日活动强度（按键数）。
    pub intensity: f64,
    /// 节奏稳定性（0..1）。
    pub steadiness: f64,
    /// 输入流畅度（0..1）。
    pub fluency: f64,
    /// 当日有活动的小时数（0..24）。
    pub activity_hours: i32,
    /// Top 键盘按键 + 计数（截前 5 项展示给 LLM）。
    pub top_keys: Vec<(u32, usize)>,
    /// 24 小时活动计数（用于生成「活跃时段」摘要）。
    pub hourly: [usize; 24],
    /// 当日首次活动时间（epoch ms），进 prompt 给 LLM 作为「当下时刻」背景。
    /// `0` 表示 sentinel「尚未首次活动」，prompt 渲染为「尚未首次活动」。
    pub first_active_ms: i64,
    /// v0.6: 特殊键计数（编排器 prompt 给 LLM 看主题词触发规则）。
    pub backspace_count: usize,
    pub delete_count: usize,
    pub enter_count: usize,
    pub space_count: usize,
    pub wasd_count: usize,
    pub total_events: usize,
}

/// 编排器输出：六条产物。LLM 一次性产出，下游适配器各自消费。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorResult {
    /// 音乐描述：风格/情绪/主题词三维（一段话，兼容 Step-Audio 与 MiniMax 纯器乐 prompt）。
    pub music_description: String,
    /// 图像描述：画面构图与色彩倾向（给 SD1.5 / 文生图 API 用）。
    pub image_description: String,
    /// 当日结语：一句 ≤ 60 个汉字。
    pub sentence: String,
    /// v0.6.0: 英文结语 — ≤ 120 英文字符，与 sentence 语义一致。
    pub english_sentence: String,
    /// v0.6.0: 主题词解释 — 8-20 汉字，一句话说明主题词由来。
    pub theme_explanation: String,
    /// v0.6.0: AI 键盘诊断 — 2 句话、40-80 字的搞笑总结。
    pub funny_summary: String,
}

/// 编排器系统提示（角色 + 输出 JSON 契约）。
///
/// 拼接规则：固定角色设定 + 六条产物的字段名/语义/字数约束 + 「必须输出合法 JSON」。
/// 与 user prompt 分开发送 —— 部分模型对 system 角色有更稳的指令遵循。
/// v0.6: 升级到 6 字段（加 english_sentence + theme_explanation），对齐同学项目完整 v0.7 契约。
const ORCHESTRATOR_SYSTEM: &str = "你是 FingerTip 的创作编排器。基于键盘活动信号，输出一个 JSON 对象，只包含以下六个字段，不要 markdown、不要解释：\
\n\n【输出字段】\n\
- `music_description`：音乐描述。一段话，覆盖「风格/情绪/主题词」三维度，兼容 Step-Audio 与 MiniMax 纯器乐 prompt，不要歌词。\
\n- `image_description`：图像描述。一段话，描述画面构图与色彩倾向，给 SD1.5 / 文生图 API 用，避免人物面孔以免肖像问题。\
\n- `sentence`：当日结语。**一句话 ≤ 60 个汉字**，呼应主题词与情绪，可诗意、可克制，避免说教。\
\n- `funny_summary`：AI 键盘诊断。**恰好 2 句话、共 40-80 个汉字**（不要写成 3 句及以上的长段），以吐槽/调侃风格总结用户今天的键盘行为特点（如「你的回车键按得比咖啡师还勤，是个急于表达的人」「凌晨三点的键盘党，建议换个养生枕头」）。可挖苦但不能冒犯，避免性别/年龄/职业歧视。\
\n\n【硬约束】\n\
1. 必须输出合法 JSON 对象，**只包含以上四个字段**，不要 markdown 代码块、不要多余字段、不要解释。\
\n2. 任一字段值**不能为空字符串**——空内容等同于未产出。";

/// 拼编排器 user prompt（当日信号 + 产物说明）。
///
/// 结构：主题词 / 情绪 / 风格 / 四指标 / Top 5 按键 / 活跃小时数 / hourly 摘要 /
/// 首次活动时间。Top 截前 5 项避免 prompt 过长；hourly 摘要统计活跃小时，避免
/// 打印 24 个数字。
pub fn orchestrator_prompt(ctx: &OrchestrationContext) -> String {
    let top_keys_str = render_top_keys(&ctx.top_keys);
    let hourly_summary = render_hourly_summary(&ctx.hourly);
    let first_active_line = render_first_active(ctx.first_active_ms);
    let mood_line = ctx
        .mood
        .as_deref()
        .map(|m| format!("情绪：{}\n", m))
        .unwrap_or_default();

    format!(
        "【当日信号】\n\
         主题词：{}\n\
         {}\
         风格：{}\n\
         首次活动时间：{}\n\
         \n\
         【四指标】\n\
         - intensity（强度，按键数）：{}\n\
         - steadiness（节奏稳定性，0..1）：{}\n\
         - fluency（流畅度，0..1）：{}\n\
         - activity_hours（活跃小时数）：{}\n\
         \n\
         【特殊键统计】\n\
         - 退格（Backspace）：{}\n\
         - 删除（Delete）：{}\n\
         - 回车（Enter）：{}\n\
         - 空格（Space）：{}\n\
         - WASD（游戏键）：{}\n\
         - 总按键：{}\n\
         \n\
         【Top 按键】\n\
         {}\n\
         \n\
         【活跃时段】\n\
         {}\n\
         \n\
         【产物契约】请按以下六个字段输出 JSON：\n\
         - music_description（音乐描述，纯器乐）\n\
         - image_description（图像描述，海报画面）\n\
         - sentence（中文当日结语，≤ 60 汉字）\n\
         - english_sentence（英文结语，≤ 120 英文字符）\n\
         - theme_explanation（主题词解释，8-20 汉字）\n\
         - funny_summary（搞笑键盘吐槽，2 句话，40-80 字）\n\
         只输出 JSON 对象，不要 markdown 代码块或多余字段。",
        ctx.theme_word,
        mood_line,
        ctx.style,
        first_active_line,
        ctx.intensity,
        ctx.steadiness,
        ctx.fluency,
        ctx.activity_hours,
        ctx.backspace_count,
        ctx.delete_count,
        ctx.enter_count,
        ctx.space_count,
        ctx.wasd_count,
        ctx.total_events,
        top_keys_str,
        hourly_summary,
    )
}

/// 渲染首次活动时间。`0` 是 sentinel「尚未首次活动」，避免把 epoch 0
/// （1970-01-01）当成真实数据塞给 LLM。
fn render_first_active(first_active_ms: i64) -> String {
    if first_active_ms == 0 {
        return "尚未首次活动".to_string();
    }
    format!("epoch_ms={}", first_active_ms)
}

/// 渲染 Top 按键：截前 5 项，格式 `key_code(N): count`。
fn render_top_keys(keys: &[(u32, usize)]) -> String {
    if keys.is_empty() {
        return "（无）".to_string();
    }
    keys.iter()
        .take(5)
        .map(|(code, count)| format!("key_code({}): {}", code, count))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 渲染 hourly 摘要：标记活跃小时（count > 0），避免打印 24 个数字。
///
/// 输出形如「活跃小时: 9, 10, 11, 14, 15」；全 0 时输出「（无）」。
fn render_hourly_summary(hourly: &[usize; 24]) -> String {
    let active: Vec<usize> = (0..24).filter(|h| hourly[*h] > 0).collect();
    if active.is_empty() {
        return "（无）".to_string();
    }
    format!("活跃小时: {}", active.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(", "))
}

/// 从 JSON value 抽取 6 字段（必填 3 + 可选 3）。
///
/// 必填：music_description / image_description / sentence —— 任一缺失/空 → None。
/// 可选：english_sentence / theme_explanation / funny_summary —— 缺失默认空字符串。
///
/// 供 `parse_orchestrator_json` 在「顶层 6 字段」与「MiniMax 包一层 schema name」两种
/// 形态下复用。空字符串入库后前端 v-if 不渲染，向后兼容 v0.5/v0.6 旧 3/4 字段契约。
fn extract_six_fields(v: &serde_json::Value) -> Option<(String, String, String, String, String, String)> {
    let obj = v.as_object()?;
    let m = obj.get("music_description")?.as_str()?;
    let i = obj.get("image_description")?.as_str()?;
    let s = obj.get("sentence")?.as_str()?;
    if m.trim().is_empty() || i.trim().is_empty() || s.trim().is_empty() {
        return None;
    }
    let e = obj
        .get("english_sentence")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let t = obj
        .get("theme_explanation")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let f = obj
        .get("funny_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    Some((m.to_string(), i.to_string(), s.to_string(), e, t, f))
}

/// 解析 LLM 返回的 JSON 为 `OrchestratorResult`。
///
/// 严格校验：
///   - 必须是 JSON object
///   - 三字段 `music_description` / `image_description` / `sentence` 必须存在
///   - 三字段必须是字符串且**非空**（编排器没产出内容视为失败）
///   - 三可选字段 `english_sentence` / `theme_explanation` / `funny_summary` 缺失默认空字符串
///   - 兼容两种形态：顶层直接 6 字段（OpenAI json_object），或顶层只包一层 schema
///     name、内层含 6 字段（MiniMax json_schema，如 `{"orchestrator_output":{六字段}}`）
///
/// 失败 → bail，错误信息含「编排器」便于上层 UI 归因。
pub fn parse_orchestrator_json(text: &str) -> anyhow::Result<OrchestratorResult> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| anyhow::anyhow!("编排器输出非合法 JSON: {}", e))?;

    let build = |m: String, i: String, s: String, e: String, t: String, f: String| OrchestratorResult {
        music_description: m,
        image_description: i,
        sentence: s,
        english_sentence: e,
        theme_explanation: t,
        funny_summary: f,
    };

    // 1. 顶层 6 字段（OpenAI json_object 直出）
    if let Some((m, i, s, e, t, f)) = extract_six_fields(&v) {
        return Ok(build(m, i, s, e, t, f));
    }

    // 2. 兼容 MiniMax json_schema：顶层只有 1 个 key 且其 value 是 object（包了一层
    //    schema name）→ unwrap 那层再按 6 字段解析。
    if let Some(inner) = v.as_object().and_then(|o| {
        if o.len() == 1 {
            o.values().next()
        } else {
            None
        }
    }) {
        if let Some((m, i, s, e, t, f)) = extract_six_fields(inner) {
            return Ok(build(m, i, s, e, t, f));
        }
    }

    if !v.is_object() {
        anyhow::bail!("编排器输出不是 JSON 对象");
    }
    anyhow::bail!("编排器输出缺 music_description/image_description/sentence 必填三字段")
}

/// 跑编排器：拼 prompt → 调一次 LLM → 解析 → 失败重试 1 次 → 还失败 bail。
///
/// 设计取舍：只重试 1 次而非无限重试 —— LLM 持续输出非法 JSON 几乎一定是 prompt
/// 设计问题，无限重试只会浪费 token。1 次重试覆盖偶发抖动（网络 / 临时空响应），
/// 2 次仍失败则视为编排契约失败，让上层 UI 报告用户（重新生成/检查 LLM 配置）。
pub async fn run_orchestrator(
    chat: &dyn JsonChat,
    ctx: &OrchestrationContext,
) -> anyhow::Result<OrchestratorResult> {
    let prompt = orchestrator_prompt(ctx);

    // 第一次尝试
    match try_once(chat, &prompt).await {
        Ok(r) => Ok(r),
        Err(e1) => {
            // 重试 1 次
            match try_once(chat, &prompt).await {
                Ok(r) => Ok(r),
                Err(e2) => Err(anyhow::anyhow!(
                    "编排器重试后仍失败（首次: {}; 重试: {}）",
                    e1,
                    e2
                )),
            }
        }
    }
}

/// 单次尝试：调 LLM 并解析。
async fn try_once(chat: &dyn JsonChat, prompt: &str) -> anyhow::Result<OrchestratorResult> {
    let v = chat.chat_json(ORCHESTRATOR_SYSTEM, prompt).await?;
    // chat_json 已返回 serde_json::Value，但保险起见再做一次解析——LLM 偶尔会
    // 把 JSON 字符串当字符串返回（即使请求了 response_format.json_object）。
    let text = v
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string());
    parse_orchestrator_json(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ctx() -> OrchestrationContext {
        OrchestrationContext {
            theme_word: "rain".into(),
            mood: Some("calm".into()),
            style: "ambient".into(),
            intensity: 120.0,
            steadiness: 0.5,
            fluency: 0.05,
            activity_hours: 6,
            top_keys: vec![(65, 50), (66, 30)], // A=50, B=30
            hourly: [0; 24],
            first_active_ms: 1_700_000_000_000,
            backspace_count: 3,
            delete_count: 1,
            enter_count: 12,
            space_count: 40,
            wasd_count: 0,
            total_events: 80,
        }
    }

    #[test]
    fn orchestrator_prompt_contains_daily_signals() {
        let p = orchestrator_prompt(&sample_ctx());
        assert!(p.contains("rain"), "prompt 必须含主题词");
        assert!(p.contains("calm"), "prompt 必须含 mood");
        assert!(p.contains("ambient"), "prompt 必须含 style");
        assert!(p.contains("intensity"), "prompt 必须含四指标名");
        assert!(p.contains("120"), "intensity 数值进 prompt");
    }

    #[test]
    fn parse_orchestrator_json_valid() {
        let j = r#"{"music_description":"calm piano","image_description":"orange abstract","sentence":"A quiet day of focus"}"#;
        let r = parse_orchestrator_json(j).unwrap();
        assert_eq!(r.sentence, "A quiet day of focus");
        assert_eq!(r.music_description, "calm piano");
        // v0.6.0: 缺 funny_summary 时默认为空字符串（前端 v-if 不渲染）
        assert_eq!(r.funny_summary, "");
    }

    #[test]
    fn parse_orchestrator_json_extracts_funny_summary() {
        let j = r#"{"music_description":"m","image_description":"i","sentence":"s","funny_summary":"凌晨三点还在敲键盘"}"#;
        let r = parse_orchestrator_json(j).unwrap();
        assert_eq!(r.funny_summary, "凌晨三点还在敲键盘");
    }

    #[test]
    fn parse_orchestrator_json_unwraps_single_wrapper_key_with_funny() {
        // v0.6.0: MiniMax json_schema 形态下 funny_summary 也必须透传
        let j = r#"{"orchestrator_output":{"music_description":"m","image_description":"i","sentence":"s","funny_summary":"你的回车键按得比咖啡师还勤"}}"#;
        let r = parse_orchestrator_json(j).unwrap();
        assert_eq!(r.funny_summary, "你的回车键按得比咖啡师还勤");
    }

    #[test]
    fn parse_orchestrator_json_unwraps_single_wrapper_key() {
        // MiniMax 用 json_schema 会把三字段包一层 name（orchestrator_output）：
        // 顶层只有 1 个 key 且 value 是 object → unwrap 内层再按三字段解析。
        let j = r#"{"orchestrator_output":{"music_description":"m","image_description":"i","sentence":"s"}}"#;
        let r = parse_orchestrator_json(j).unwrap();
        assert_eq!(r.sentence, "s");
        assert_eq!(r.music_description, "m");
    }

    #[test]
    fn parse_orchestrator_json_rejects_missing_fields() {
        assert!(parse_orchestrator_json(r#"{"music_description":"x"}"#).is_err());
        assert!(parse_orchestrator_json(r#"{}"#).is_err());
        // r#""# 是裸字符串 → 不是合法 JSON 对象（解析时已抛错）
        assert!(parse_orchestrator_json(r#""#).is_err());
    }

    #[test]
    fn parse_orchestrator_json_rejects_non_object() {
        // 数组 / 字符串 / 数字 / null —— 解析能过但不是 object → 必须拒
        assert!(parse_orchestrator_json("[1,2,3]").is_err());
        assert!(parse_orchestrator_json("\"string\"").is_err());
        assert!(parse_orchestrator_json("42").is_err());
        assert!(parse_orchestrator_json("null").is_err());
        let err = parse_orchestrator_json("[1,2,3]").unwrap_err().to_string();
        assert!(err.contains("对象"), "非 object 错误信息应含「对象」");
    }

    #[test]
    fn parse_orchestrator_json_rejects_whitespace_only_fields() {
        // 纯空格绕过 .is_empty() 必须拒（trim 防御）
        assert!(
            parse_orchestrator_json(
                r#"{"music_description":"  ","image_description":"i","sentence":"s"}"#
            )
            .is_err()
        );
        assert!(
            parse_orchestrator_json(
                r#"{"music_description":"x","image_description":"\t","sentence":"s"}"#
            )
            .is_err()
        );
        assert!(
            parse_orchestrator_json(
                r#"{"music_description":"x","image_description":"i","sentence":"\n"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn orchestrator_prompt_includes_first_active_time() {
        // 正常值：进 prompt
        let p = orchestrator_prompt(&sample_ctx());
        assert!(
            p.contains("1700000000000"),
            "first_active_ms=1700000000000 应进 prompt"
        );
        assert!(p.contains("首次活动时间"));
        // sentinel 0：渲染为「尚未首次活动」
        let mut ctx = sample_ctx();
        ctx.first_active_ms = 0;
        let p = orchestrator_prompt(&ctx);
        assert!(p.contains("尚未首次活动"));
        assert!(!p.contains("epoch_ms=0"));
    }

    #[tokio::test]
    async fn run_orchestrator_retries_once_on_invalid_json() {
        use crate::model::JsonChat;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        struct MockChat(Arc<AtomicUsize>);
        #[async_trait::async_trait]
        impl JsonChat for MockChat {
            async fn chat_json(
                &self,
                _system: &str,
                _user: &str,
            ) -> anyhow::Result<serde_json::Value> {
                let n = self.0.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Ok(serde_json::json!({"bad": true})) // 第一次：非合法 JSON 结构
                } else {
                    Ok(serde_json::json!({"music_description":"m","image_description":"i","sentence":"s"}))
                }
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let r = run_orchestrator(&MockChat(calls.clone()), &sample_ctx()).await.unwrap();
        assert_eq!(r.sentence, "s");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "应调用 2 次：1 次失败 + 1 次成功"
        );
    }

    #[tokio::test]
    async fn run_orchestrator_fails_after_two_invalid_attempts() {
        use crate::model::JsonChat;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        struct MockChat(Arc<AtomicUsize>);
        #[async_trait::async_trait]
        impl JsonChat for MockChat {
            async fn chat_json(
                &self,
                _system: &str,
                _user: &str,
            ) -> anyhow::Result<serde_json::Value> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::json!({"still_bad": true}))
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let res = run_orchestrator(&MockChat(calls.clone()), &sample_ctx()).await;
        assert!(res.is_err());
        let err = format!("{:#}", res.unwrap_err());
        assert!(
            err.contains("编排器") && err.contains("重试"),
            "错误信息应说明编排器重试后仍失败"
        );
        // 收紧：必须调 2 次（首次 + 1 次重试），不多不少。
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "应调用 2 次：1 次失败 + 1 次重试"
        );
    }

    #[test]
    fn prompt_describes_music_image_sentence_separately() {
        // 音乐描述要对 Step-Audio 与 MiniMax 都通用（风格/情绪/主题词三维）
        let p = orchestrator_prompt(&sample_ctx());
        assert!(p.contains("music_description") || p.contains("音乐描述") || p.contains("音乐"));
        assert!(p.contains("image_description") || p.contains("图像描述") || p.contains("图像"));
        assert!(p.contains("sentence") || p.contains("句子"));
    }
}