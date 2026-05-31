//! Detect and enforce response language consistency between parent and subagent sessions.

use crate::session::models::{ChatMessage, MessageRole};
use crate::session::store::SessionStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLanguage {
    Zh,
    En,
}

impl SessionLanguage {
    pub fn from_tag(tag: &str) -> Option<Self> {
        let normalized = tag.trim().to_ascii_lowercase().replace('_', "-");
        if normalized == "zh" || normalized.starts_with("zh-") {
            Some(Self::Zh)
        } else if normalized == "en" || normalized.starts_with("en-") {
            Some(Self::En)
        } else {
            None
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }

    pub fn instruction(self) -> &'static str {
        match self {
            Self::Zh => {
                "<system-reminder>\nLanguage: Simplified Chinese (简体中文) is mandatory for this session.\n\n- ALL assistant replies MUST be in Simplified Chinese.\n- ALL internal reasoning / thinking / extended-thinking blocks MUST be written in Simplified Chinese. Do NOT think in English.\n- Tool narration, summaries, review verdicts, and error explanations MUST also use Simplified Chinese.\n</system-reminder>"
            }
            Self::En => {
                "<system-reminder>\nRespond in English. Match the parent session language for all explanations, summaries, and review verdicts.\n</system-reminder>"
            }
        }
    }

    /// Short per-turn reminder appended to user messages — models weight this more than distant system text.
    pub fn user_turn_reminder(self) -> Option<&'static str> {
        match self {
            Self::Zh => Some(
                "<system-reminder>\nReply and think in Simplified Chinese (简体中文). Thinking blocks must NOT be in English.\n</system-reminder>",
            ),
            Self::En => None,
        }
    }

    /// Prepended via `prompt_prefix` (not persisted in visible message content) when Chinese is forced.
    pub fn user_message_prefix(self) -> Option<&'static str> {
        match self {
            Self::Zh => Some(ZH_USER_MESSAGE_PREFIX),
            Self::En => None,
        }
    }
}

pub const ZH_USER_MESSAGE_PREFIX: &str =
    "【语言要求：全程使用简体中文，包括 thinking / 推理过程，禁止英文思考】\n\n";

pub fn strip_user_message_display_prefix(content: &str) -> &str {
    content.strip_prefix(ZH_USER_MESSAGE_PREFIX).unwrap_or(content)
}

pub fn is_explicit_chinese_locale(explicit_locale: Option<&str>) -> bool {
    matches!(
        explicit_locale.and_then(SessionLanguage::from_tag),
        Some(SessionLanguage::Zh)
    )
}

/// Drop replayed assistant thinking so prior English reasoning does not anchor new turns.
pub fn strip_assistant_thinking_for_prompt(messages: &mut [ChatMessage]) {
    for message in messages.iter_mut() {
        if message.role != MessageRole::Assistant {
            continue;
        }
        message.thinking_content = None;
        message.thinking_duration = None;
        message.thinking_signature = None;
    }
}

pub fn detect_session_language(
    store: &SessionStore,
    session_id: &str,
    working_dir: &str,
    explicit_locale: Option<&str>,
) -> SessionLanguage {
    if let Some(tag) = explicit_locale.and_then(SessionLanguage::from_tag) {
        return tag;
    }

    if !working_dir.trim().is_empty() {
        if let Ok(config) = crate::workspace::read_workspace_config(working_dir) {
            if config.force_zh {
                return SessionLanguage::Zh;
            }
        }
    }

    if let Ok(messages) = store.get_messages(session_id) {
        if let Some(lang) = infer_language_from_messages(&messages) {
            return lang;
        }
    }

    if sys_locale::get_locale()
        .map(|locale| locale.to_ascii_lowercase().starts_with("zh"))
        .unwrap_or(false)
    {
        return SessionLanguage::Zh;
    }

    SessionLanguage::En
}

pub fn wrap_subagent_prompt(prompt: &str, language: SessionLanguage) -> String {
    format!("{}\n\n{prompt}", language.instruction())
}

fn infer_language_from_messages(messages: &[ChatMessage]) -> Option<SessionLanguage> {
    let mut sample = String::new();
    for message in messages.iter().rev().take(32) {
        if !matches!(message.role, MessageRole::User | MessageRole::Assistant) {
            continue;
        }
        sample.push_str(&message.content);
        sample.push('\n');
        if sample.len() >= 12_000 {
            break;
        }
    }

    let trimmed = sample.trim();
    if trimmed.is_empty() {
        return None;
    }

    let cjk = trimmed.chars().filter(|ch| is_cjk(*ch)).count();
    let latin = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();

    if cjk == 0 && latin > 0 {
        return Some(SessionLanguage::En);
    }
    if latin == 0 && cjk > 0 {
        return Some(SessionLanguage::Zh);
    }
    if cjk == 0 && latin == 0 {
        return None;
    }

    let ratio = cjk as f64 / (cjk + latin) as f64;
    if ratio >= 0.2 || (cjk >= 6 && ratio >= 0.08) {
        Some(SessionLanguage::Zh)
    } else {
        Some(SessionLanguage::En)
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{3000}'..='\u{303F}'
            | '\u{FF00}'..='\u{FFEF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_message(content: &str) -> ChatMessage {
        ChatMessage {
            id: "1".to_string(),
            role: MessageRole::User,
            content: content.to_string(),
            created_at: 0,
            prompt_prefix: None,
            prompt_suffix: None,
            response_id: None,
            content_order: None,
            thinking_order: None,
            tool_calls: None,
            tool_call_id: None,
            images: None,
            asset_refs: None,
            thinking_content: None,
            thinking_duration: None,
            thinking_signature: None,
            knowledge_proposal: None,
            memory_proposal: None,
            render_parts: None,
        }
    }

    #[test]
    fn infers_chinese_from_user_text() {
        let messages = vec![sample_message(
            "请帮我修改 Seat.lua，添加座位信息显示",
        )];
        assert_eq!(
            infer_language_from_messages(&messages),
            Some(SessionLanguage::Zh)
        );
    }

    #[test]
    fn infers_english_from_user_text() {
        let messages = vec![sample_message(
            "Please refactor the seat controller and add tests.",
        )];
        assert_eq!(
            infer_language_from_messages(&messages),
            Some(SessionLanguage::En)
        );
    }

    #[test]
    fn wrap_subagent_prompt_includes_instruction() {
        let wrapped = wrap_subagent_prompt("Do the task.", SessionLanguage::Zh);
        assert!(wrapped.contains("简体中文"));
        assert!(wrapped.contains("thinking"));
        assert!(wrapped.contains("Do the task."));
    }

    #[test]
    fn strip_user_message_display_prefix_removes_implicit_language_requirement() {
        assert_eq!(
            strip_user_message_display_prefix(&format!("{ZH_USER_MESSAGE_PREFIX}创建文件")),
            "创建文件"
        );
        assert_eq!(strip_user_message_display_prefix("创建文件"), "创建文件");
    }

    #[test]
    fn strips_assistant_thinking_from_prompt_messages() {
        let mut messages = vec![ChatMessage {
            id: "a1".to_string(),
            role: MessageRole::Assistant,
            content: "done".to_string(),
            created_at: 0,
            prompt_prefix: None,
            prompt_suffix: None,
            response_id: None,
            content_order: None,
            thinking_order: Some(1),
            tool_calls: None,
            tool_call_id: None,
            images: None,
            asset_refs: None,
            thinking_content: Some("Let me think in English".to_string()),
            thinking_duration: Some(3),
            thinking_signature: Some("sig".to_string()),
            knowledge_proposal: None,
            memory_proposal: None,
            render_parts: None,
        }];

        strip_assistant_thinking_for_prompt(&mut messages);

        assert!(messages[0].thinking_content.is_none());
        assert!(messages[0].thinking_duration.is_none());
        assert!(messages[0].thinking_signature.is_none());
    }

    #[test]
    fn explicit_locale_overrides_english_history() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SessionStore::new(dir.path()).expect("store");
        let session_id = store
            .create_session("test", None, None, "chat", None)
            .expect("session");
        store
            .add_message(
                &session_id,
                MessageRole::User,
                "Please refactor the seat controller.",
            )
            .expect("message");

        let lang = detect_session_language(&store, &session_id, "", Some("zh"));
        assert_eq!(lang, SessionLanguage::Zh);
        assert!(lang.user_turn_reminder().is_some());
        assert!(lang.instruction().contains("Do NOT think in English"));
    }
}
