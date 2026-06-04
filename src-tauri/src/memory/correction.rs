use std::collections::HashSet;

use crate::memory::models::MemoryCategory;
use crate::session::models::{ChatMessage, MessageRole};

/// User pushed back on agent work; extract structured lesson for memory proposals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectionMemoryDraft {
    pub user_correction: String,
    pub error_reason: String,
    pub avoidance: String,
    pub context: Option<String>,
}

const STRONG_CORRECTION_MARKERS: &[&str] = &[
    "that's wrong",
    "that is wrong",
    "you're wrong",
    "you are wrong",
    "not correct",
    "incorrect",
    "actually ",
    "instead ",
    "should be",
    "should have",
    "must be",
    "must not",
    "don't ",
    "do not ",
    "不对",
    "错了",
    "不是这样",
    "不是这",
    "不应该",
    "不该",
    "有问题",
    "质疑",
    "纠正",
    "搞错",
    "弄错",
    "误解",
    "应该是",
    "其实是",
    "正确做法",
    "为什么不能",
];

const ASSISTANT_ACK_MARKERS: &[&str] = &[
    "you're right",
    "you are right",
    "i was wrong",
    "my mistake",
    "good catch",
    "understood",
    "i see the issue",
    "i'll fix",
    "i will fix",
    "let me fix",
    "抱歉",
    "对不起",
    "你说得对",
    "确实",
    "我理解",
    "我搞错",
    "我弄错",
    "之前错了",
    "先前理解有误",
    "重新修改",
    "马上改",
];

const IMPLEMENTATION_MARKERS: &[&str] = &[
    "implemented",
    "applied",
    "updated",
    "modified",
    "refactor",
    "fixed",
    "已完成",
    "已实现",
    "已修改",
    "已更新",
    "改动",
    "修复",
];

const REASON_HINT_MARKERS: &[&str] = &[
    "because",
    "reason",
    "cause",
    "问题在于",
    "原因是",
    "根源",
    "由于",
    "导致",
];

const AVOIDANCE_HINT_MARKERS: &[&str] = &[
    "should",
    "must",
    "never",
    "always",
    "avoid",
    "instead",
    "以后",
    "下次",
    "不要",
    "别再",
    "应当",
    "需要",
    "确保",
];

pub fn is_correction_message(user_message: &str) -> bool {
    let trimmed = user_message.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    contains_any(&lower, STRONG_CORRECTION_MARKERS)
}

pub fn collect_tool_rejection_feedbacks(messages: &[ChatMessage]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for message in messages {
        if message.role != MessageRole::Tool {
            continue;
        }
        let Some(feedback) = parse_tool_rejection_feedback(&message.content) else {
            continue;
        };
        let key = normalize_key(&feedback);
        if seen.insert(key) {
            out.push(feedback);
        }
    }
    out
}

fn parse_tool_rejection_feedback(content: &str) -> Option<String> {
    const PREFIX: &str = "User feedback:";
    let idx = content.find(PREFIX)?;
    let feedback = content[idx + PREFIX.len()..].trim();
    if feedback.is_empty() {
        return None;
    }
    Some(clip(feedback, 400))
}

pub fn correction_likely_valid(
    user_correction: &str,
    assistant_after: &str,
    prior_assistant: Option<&str>,
    tool_rejections: &[String],
) -> bool {
    if !is_correction_message(user_correction) {
        return false;
    }
    if !tool_rejections.is_empty() {
        return true;
    }
    let after = assistant_after.trim();
    if !after.is_empty() && contains_any(&after.to_lowercase(), ASSISTANT_ACK_MARKERS) {
        return true;
    }
    if let Some(prior) = prior_assistant {
        let prior = prior.trim();
        if prior.chars().count() >= 60
            && contains_any(&prior.to_lowercase(), IMPLEMENTATION_MARKERS)
        {
            return true;
        }
    }
    false
}

pub fn build_correction_draft(
    user_correction: &str,
    mistaken_assistant: Option<&str>,
    assistant_after: &str,
    tool_rejections: &[String],
) -> CorrectionMemoryDraft {
    let user_correction = clip(user_correction.trim(), 480);
    let error_reason = infer_error_reason(
        &user_correction,
        mistaken_assistant,
        tool_rejections,
    );
    let avoidance = infer_avoidance(&user_correction, assistant_after, mistaken_assistant);
    let context = build_context_snippet(mistaken_assistant, tool_rejections);

    CorrectionMemoryDraft {
        user_correction,
        error_reason,
        avoidance,
        context,
    }
}

pub fn format_correction_memory_content(draft: &CorrectionMemoryDraft) -> String {
    let mut parts = vec![
        format!("【错误原因】{}", draft.error_reason),
        format!("【如何避免】{}", draft.avoidance),
        format!("【用户纠正】{}", draft.user_correction),
    ];
    if let Some(context) = draft.context.as_ref().filter(|v| !v.trim().is_empty()) {
        parts.push(format!("【相关上下文】{}", context));
    }
    clip(&parts.join("\n"), 900)
}

pub fn extract_session_correction_candidates(
    messages: &[ChatMessage],
    pairs: &[(String, String)],
) -> Vec<(MemoryCategory, String, Vec<String>, f32)> {
    if pairs.is_empty() {
        return Vec::new();
    }
    let tool_rejections = collect_tool_rejection_feedbacks(messages);
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for (index, (user, assistant_after)) in pairs.iter().enumerate() {
        if !is_correction_message(user) {
            continue;
        }
        let prior_assistant = index
            .checked_sub(1)
            .and_then(|i| pairs.get(i))
            .map(|(_, assistant)| assistant.as_str());
        if !correction_likely_valid(user, assistant_after, prior_assistant, &tool_rejections) {
            continue;
        }
        let draft = build_correction_draft(
            user,
            prior_assistant,
            assistant_after,
            &tool_rejections,
        );
        let content = format_correction_memory_content(&draft);
        let key = normalize_key(&content);
        if !seen.insert(key) {
            continue;
        }
        out.push((
            MemoryCategory::Feedback,
            content,
            vec![
                "feedback".to_string(),
                "correction".to_string(),
                "avoidance".to_string(),
                "user-validated".to_string(),
            ],
            0.9,
        ));
    }

    out
}

fn infer_error_reason(
    user_correction: &str,
    mistaken_assistant: Option<&str>,
    tool_rejections: &[String],
) -> String {
    if let Some(sentence) = extract_sentence_with_markers(user_correction, REASON_HINT_MARKERS) {
        return clip(&sentence, 360);
    }
    if let Some(first) = tool_rejections.first() {
        return format!("用户拒绝工具执行并说明：{}", clip(first, 320));
    }
    if let Some(prior) = mistaken_assistant.filter(|v| !v.trim().is_empty()) {
        return format!(
            "先前实现/回复与预期不符。摘要：{}",
            clip(&first_meaningful_chunk(prior), 320)
        );
    }
    clip(user_correction, 360)
}

fn infer_avoidance(
    user_correction: &str,
    assistant_after: &str,
    mistaken_assistant: Option<&str>,
) -> String {
    if let Some(sentence) = extract_sentence_with_markers(user_correction, AVOIDANCE_HINT_MARKERS) {
        return clip(&sentence, 360);
    }
    let after = assistant_after.trim();
    if !after.is_empty() && contains_any(&after.to_lowercase(), ASSISTANT_ACK_MARKERS) {
        return clip(
            &format!("按后续修正执行：{}", first_meaningful_chunk(after)),
            360,
        );
    }
    if let Some(prior) = mistaken_assistant {
        return format!(
            "避免重复「{}」这类做法；以用户本次纠正为准。",
            clip(&first_meaningful_chunk(prior), 120)
        );
    }
    "后续遇到类似场景时，先核对用户约束再改代码或调用工具。".to_string()
}

fn build_context_snippet(
    mistaken_assistant: Option<&str>,
    tool_rejections: &[String],
) -> Option<String> {
    let mut chunks = Vec::new();
    if let Some(prior) = mistaken_assistant.filter(|v| v.trim().len() >= 40) {
        chunks.push(format!(
            "先前回复片段：{}",
            clip(&first_meaningful_chunk(prior), 220)
        ));
    }
    for (index, rejection) in tool_rejections.iter().take(2).enumerate() {
        chunks.push(format!("工具拒绝反馈{}：{}", index + 1, clip(rejection, 180)));
    }
    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join(" "))
    }
}

fn extract_sentence_with_markers(text: &str, markers: &[&str]) -> Option<String> {
    let lower = text.to_lowercase();
    for sentence in split_sentences(text) {
        let sentence_lower = sentence.to_lowercase();
        if markers.iter().any(|m| sentence_lower.contains(m)) {
            return Some(sentence);
        }
    }
    if markers.iter().any(|m| lower.contains(m)) {
        return Some(clip(text, 280));
    }
    None
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '。' | '！' | '？' | '.' | '!' | '?') && current.trim().len() > 4 {
            sentences.push(current.trim().to_string());
            current.clear();
        }
    }
    if current.trim().len() > 4 {
        sentences.push(current.trim().to_string());
    }
    if sentences.is_empty() && !text.trim().is_empty() {
        sentences.push(text.trim().to_string());
    }
    sentences
}

fn first_meaningful_chunk(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .find(|p| p.chars().count() >= 16)
        .or_else(|| text.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or(text)
        .to_string()
}

fn contains_any(lower_text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| lower_text.contains(marker))
}

fn normalize_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .take(120)
        .collect()
}

fn clip(value: &str, max_chars: usize) -> String {
    let merged = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if merged.chars().count() <= max_chars {
        merged
    } else {
        merged.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_strong_correction_in_chinese() {
        assert!(is_correction_message("这个改法不对，应该是 local 而不是全局赋值"));
    }

    #[test]
    fn valid_when_assistant_acknowledges() {
        assert!(correction_likely_valid(
            "不对，这样会破坏 _G.TimerManager",
            "你说得对，我之前理解错了，我会改回全局赋值。",
            Some("已完成 Application.lua 的优化并实现。"),
            &[],
        ));
    }

    #[test]
    fn formats_structured_memory_content() {
        let draft = build_correction_draft(
            "不对，local Log 会遮住全局 Log",
            Some("已在顶部加入 local Log = Log"),
            "抱歉，我会改为只在本文件缓存。",
            &[],
        );
        let text = format_correction_memory_content(&draft);
        assert!(text.contains("【错误原因】"));
        assert!(text.contains("【如何避免】"));
        assert!(text.contains("【用户纠正】"));
    }

    #[test]
    fn parses_tool_rejection_feedback() {
        let body = "Tool 'bash' was rejected by user feedback. Revise the proposal before trying again.\nUser feedback: 目录不要用 /tmp";
        assert_eq!(
            parse_tool_rejection_feedback(body).as_deref(),
            Some("目录不要用 /tmp")
        );
    }
}
