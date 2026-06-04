use std::collections::HashSet;
use std::path::Path;

use super::models::{MemoryCategory, MemoryEntry, MemoryScope, DEFAULT_PIN_WEIGHT};
use super::store::{current_unix_millis, new_entry_id, MemoryStoreState};
use crate::knowledge_store::{self, KnowledgeType};
use crate::session::models::{ChatMessage, MessageRole};

const MEMORY_USER_PREFERENCE_PATH: &str = "user-preference.md";
const MEMORY_PROJECT_MISTAKE_NOTE_PATH: &str = "project-mistake-note.md";
const MEMORY_UNITY_PROJECT_UNDERSTANDING_DIR: &str = "unity-project-understanding";

pub fn default_scope_for_category(category: MemoryCategory) -> MemoryScope {
    match category {
        MemoryCategory::User => MemoryScope::User,
        MemoryCategory::Feedback | MemoryCategory::Topic | MemoryCategory::Reference => {
            MemoryScope::Project
        }
    }
}

pub fn linked_doc_path_for_category(category: MemoryCategory, slug: &str) -> Option<String> {
    match category {
        MemoryCategory::User => Some(MEMORY_USER_PREFERENCE_PATH.to_string()),
        MemoryCategory::Feedback => Some(MEMORY_PROJECT_MISTAKE_NOTE_PATH.to_string()),
        MemoryCategory::Topic => Some(format!("{}/{}.md", MEMORY_UNITY_PROJECT_UNDERSTANDING_DIR, slug)),
        MemoryCategory::Reference => Some(format!("reference/{}.md", slug)),
    }
}

pub fn slugify(value: &str) -> String {
    let mut slug = value
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    slug.trim_matches('-').chars().take(48).collect()
}

pub fn sync_entry_to_markdown(working_dir: &str, entry: &MemoryEntry) -> Result<Option<String>, String> {
    knowledge_store::ensure_memory_builtin_documents(working_dir)?;

    match entry.category {
        MemoryCategory::User => append_bullet_to_memory_doc(
            working_dir,
            MEMORY_USER_PREFERENCE_PATH,
            &entry.content,
        ),
        MemoryCategory::Feedback => append_bullet_to_memory_doc(
            working_dir,
            MEMORY_PROJECT_MISTAKE_NOTE_PATH,
            &entry.content,
        ),
        MemoryCategory::Topic => {
            let slug = slugify(&entry.content);
            let path = format!("{}/{}.md", MEMORY_UNITY_PROJECT_UNDERSTANDING_DIR, slug);
            write_topic_doc(working_dir, &path, &entry.content)
        }
        MemoryCategory::Reference => {
            let slug = slugify(&entry.content);
            let path = format!("reference/{}.md", slug);
            write_topic_doc(working_dir, &path, &entry.content)
        }
    }
}

fn append_bullet_to_memory_doc(
    working_dir: &str,
    rel_path: &str,
    content: &str,
) -> Result<Option<String>, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let bullet = format!("- {}", trimmed);
    let mut doc =
        knowledge_store::load_document_by_path(working_dir, KnowledgeType::Memory, rel_path)?;
    let body = doc.body.trim();
    if body.lines().any(|line| line.trim() == bullet) {
        return Ok(Some(rel_path.to_string()));
    }

    doc.body = if body.is_empty() {
        format!("{}\n", bullet)
    } else {
        format!("{}\n{}\n", body, bullet)
    };
    doc.updated_at = current_unix_millis();
    knowledge_store::save_document(working_dir, doc)?;
    Ok(Some(rel_path.to_string()))
}

fn write_topic_doc(working_dir: &str, rel_path: &str, content: &str) -> Result<Option<String>, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let now = current_unix_millis();
    match knowledge_store::load_document_by_path(working_dir, KnowledgeType::Memory, rel_path) {
        Ok(mut doc) => {
            if doc.body.trim() == trimmed {
                return Ok(Some(rel_path.to_string()));
            }
            doc.body = trimmed.to_string();
            doc.updated_at = now;
            knowledge_store::save_document(working_dir, doc)?;
        }
        Err(err) if err.contains("not found") => {
            if let Some(parent) = Path::new(rel_path).parent().and_then(|p| p.to_str()).filter(|p| !p.is_empty()) {
                let _ = knowledge_store::create_directory(working_dir, KnowledgeType::Memory, parent);
            }
            let title = Path::new(rel_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("memory")
                .replace('-', " ");
            let doc = knowledge_store::KnowledgeDocument {
                id: format!("kd_{}", uuid::Uuid::new_v4()),
                doc_type: KnowledgeType::Memory,
                path: rel_path.to_string(),
                title,
                inject_mode: knowledge_store::KnowledgeInjectMode::Path,
                inherit_inject_mode: true,
                inject_mode_source: Default::default(),
                summary_enabled: false,
                command_enabled: false,
                read_only: false,
                ai_maintained: true,
                storage_source: knowledge_store::KnowledgeStorageSource::Project,
                inherit_ai_config: true,
                ai_config_source: Default::default(),
                explicit_maintenance_rules: true,
                external_source: None,
                skill_enabled: None,
                skill_surface: None,
                command_trigger: None,
                argument_hint: None,
                tools: Vec::new(),
                summary: None,
                body: trimmed.to_string(),
                maintenance_rules: None,
                created_at: now,
                updated_at: now,
            };
            knowledge_store::save_document(working_dir, doc)?;
        }
        Err(err) => return Err(err),
    }

    Ok(Some(rel_path.to_string()))
}

pub fn apply_memory_entry(
    store: &MemoryStoreState,
    working_dir: &str,
    app_storage_dir: Option<&Path>,
    entry: MemoryEntry,
    embedding: Option<Vec<f32>>,
) -> Result<MemoryEntry, String> {
    let mut entry = entry;
    if entry.linked_doc_path.is_none() {
        entry.linked_doc_path = linked_doc_path_for_category(
            entry.category,
            &slugify(&entry.content),
        );
    }

    let saved = store.create(working_dir, app_storage_dir, entry, embedding)?;
    if saved.scope == MemoryScope::Project {
        match sync_entry_to_markdown(working_dir, &saved) {
            Ok(Some(linked)) => {
                let mut updated = saved.clone();
                updated.linked_doc_path = Some(linked);
                return Ok(updated);
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!(
                    "[Locus] memory saved to agentmemory but markdown sync failed: {}",
                    error
                );
            }
        }
    }
    Ok(saved)
}

pub fn build_memory_entry_from_proposal_item(
    item: &crate::session::models::MemoryProposalItem,
    source_session_id: Option<String>,
) -> MemoryEntry {
    let now = current_unix_millis();
    MemoryEntry {
        id: new_entry_id(),
        category: item.category,
        scope: item.scope,
        content: item.content.clone(),
        tags: item.tags.clone(),
        pinned: false,
        pin_weight: DEFAULT_PIN_WEIGHT,
        access_count: 0,
        last_accessed_at: 0,
        created_at: now,
        updated_at: now,
        source_session_id,
        linked_doc_path: None,
    }
}

pub fn evaluate_memory_proposal(
    user_message: &str,
    assistant_message: &str,
) -> Option<Vec<(MemoryCategory, String, Vec<String>, f32)>> {
    if is_trivial_exchange(user_message, assistant_message) {
        return None;
    }

    let mut candidates = extract_memory_candidates(user_message, assistant_message);
    if has_explicit_remember_intent(user_message) {
        let assistant = assistant_message.trim();
        if assistant.chars().count() >= 48 {
            candidates.push((
                MemoryCategory::Reference,
                clip_line(assistant, 280),
                vec!["explicit".to_string()],
                0.86,
            ));
        } else {
            candidates.push((
                MemoryCategory::User,
                clip_line(user_message, 240),
                vec!["remember".to_string()],
                0.84,
            ));
        }
    }

    let strong_candidates: Vec<_> = candidates
        .into_iter()
        .filter(|(_, content, _, confidence)| {
            !content.trim().is_empty() && *confidence >= MIN_ITEM_CONFIDENCE
        })
        .collect();

    if strong_candidates.is_empty() {
        return None;
    }

    if !is_worth_saving(&strong_candidates, user_message) {
        return None;
    }

    Some(strong_candidates)
}

pub fn evaluate_memory_proposal_from_session(
    messages: &[ChatMessage],
) -> Option<Vec<(MemoryCategory, String, Vec<String>, f32)>> {
    let pairs = collect_user_assistant_pairs(messages);
    if pairs.is_empty() {
        return None;
    }

    let mut raw_candidates =
        crate::memory::extract_session_correction_candidates(messages, &pairs);
    for (user, assistant) in &pairs {
        if crate::memory::is_correction_message(user) {
            continue;
        }
        if let Some(mut hits) = evaluate_memory_proposal(user, assistant) {
            raw_candidates.append(&mut hits);
        }
    }

    if raw_candidates.is_empty() {
        if let Some(outcome) = extract_session_outcome_summary(&pairs) {
            raw_candidates.push(outcome);
        } else {
            return None;
        }
    }

    let summarized = summarize_session_candidates(&raw_candidates, pairs.len());
    if summarized.is_empty() || !session_is_worth_saving(&summarized, pairs.len()) {
        return None;
    }

    Some(summarized)
}

pub fn extract_memory_candidates(
    user_message: &str,
    assistant_message: &str,
) -> Vec<(MemoryCategory, String, Vec<String>, f32)> {
    let mut candidates = Vec::new();
    let user = user_message.trim();
    let assistant = assistant_message.trim();
    if user.is_empty() && assistant.is_empty() {
        return candidates;
    }

    let preference_markers = PREFERENCE_MARKERS;
    let lower_user = user.to_lowercase();
    if preference_markers.iter().any(|marker| lower_user.contains(marker)) {
        candidates.push((
            MemoryCategory::User,
            clip_line(user, 240),
            vec!["preference".to_string()],
            0.82,
        ));
    }

    if crate::memory::is_correction_message(user) {
        if crate::memory::correction_likely_valid(user, assistant, None, &[]) {
            let draft = crate::memory::build_correction_draft(user, None, assistant, &[]);
            candidates.push((
                MemoryCategory::Feedback,
                crate::memory::format_correction_memory_content(&draft),
                vec![
                    "feedback".to_string(),
                    "correction".to_string(),
                    "avoidance".to_string(),
                    "user-validated".to_string(),
                ],
                0.9,
            ));
        }
    } else if FEEDBACK_MARKERS.iter().any(|marker| lower_user.contains(marker)) {
        candidates.push((
            MemoryCategory::Feedback,
            clip_line(user, 240),
            vec!["feedback".to_string()],
            0.78,
        ));
    }

    let topic_markers = [
        "architecture",
        "convention",
        "pattern",
        "we use",
        "this project",
        "in this codebase",
        "our project",
        "we decided",
        "the rule is",
        "本项目",
        "我们项目",
        "项目里",
        "架构",
        "规范",
        "约定",
        "技术栈",
        "设计是",
    ];
    if topic_markers.iter().any(|marker| lower_user.contains(marker)) {
        candidates.push((
            MemoryCategory::Topic,
            clip_line(user, 280),
            vec!["project".to_string()],
            0.76,
        ));
    }

    candidates
        .into_iter()
        .filter(|(_, content, _, confidence)| !content.trim().is_empty() && *confidence >= MIN_ITEM_CONFIDENCE)
        .collect()
}

const MIN_ITEM_CONFIDENCE: f32 = 0.72;
const MIN_PROPOSAL_CONFIDENCE: f32 = 0.74;

fn is_trivial_exchange(user_message: &str, assistant_message: &str) -> bool {
    let user = user_message.trim();
    if user.is_empty() {
        return assistant_message.trim().is_empty();
    }

    let lower_user = user.to_lowercase();
    const TRIVIAL_USER_REPLIES: &[&str] = &[
        "ok",
        "okay",
        "yes",
        "no",
        "yep",
        "nope",
        "thanks",
        "thank you",
        "thx",
        "got it",
        "continue",
        "go on",
        "好的",
        "好",
        "行",
        "嗯",
        "对的",
        "收到",
        "谢谢",
        "继续",
        "可以",
        "明白",
        "了解",
    ];
    if TRIVIAL_USER_REPLIES.iter().any(|marker| lower_user == *marker) {
        return true;
    }

    user.chars().count() < 12
        && !has_explicit_remember_intent(user)
        && !contains_any_marker(&lower_user, &PREFERENCE_MARKERS)
        && !contains_any_marker(&lower_user, &FEEDBACK_MARKERS)
}

fn has_explicit_remember_intent(user_message: &str) -> bool {
    let lower_user = user_message.trim().to_lowercase();
    const REMEMBER_MARKERS: &[&str] = &[
        "remember this",
        "save this",
        "keep in mind",
        "note that",
        "don't forget",
        "记住",
        "记下来",
        "保存一下",
        "请记住",
        "别忘了",
        "以后记得",
    ];
    contains_any_marker(&lower_user, REMEMBER_MARKERS)
}

fn is_worth_saving(
    candidates: &[(MemoryCategory, String, Vec<String>, f32)],
    user_message: &str,
) -> bool {
    if has_explicit_remember_intent(user_message) {
        return true;
    }

    if candidates.iter().any(|(category, _, _, confidence)| {
        matches!(
            category,
            MemoryCategory::User | MemoryCategory::Feedback | MemoryCategory::Topic
        ) && *confidence >= MIN_PROPOSAL_CONFIDENCE
    }) {
        return true;
    }

    let max_confidence = candidates
        .iter()
        .map(|(_, _, _, confidence)| *confidence)
        .fold(0.0_f32, f32::max);
    max_confidence >= 0.80
}

fn contains_any_marker(lower_text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| lower_text.contains(marker))
}

fn collect_user_assistant_pairs(messages: &[ChatMessage]) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut pending_user: Option<String> = None;

    for message in messages {
        if message.memory_proposal.is_some() || message.knowledge_proposal.is_some() {
            continue;
        }

        match message.role {
            MessageRole::User => {
                let text = message.content.trim();
                if !text.is_empty() {
                    pending_user = Some(text.to_string());
                }
            }
            MessageRole::Assistant => {
                let has_tool_calls = message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| !calls.is_empty());
                let assistant = message.content.trim();
                if has_tool_calls && assistant.is_empty() {
                    continue;
                }
                if let Some(user) = pending_user.take() {
                    if !is_trivial_exchange(&user, assistant) {
                        pairs.push((user, assistant.to_string()));
                    }
                }
            }
            MessageRole::Tool => {}
        }
    }

    pairs
}

fn dedupe_candidates(
    raw: &[(MemoryCategory, String, Vec<String>, f32)],
) -> Vec<(MemoryCategory, String, Vec<String>, f32)> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for (category, content, tags, confidence) in raw {
        let key = format!("{:?}:{}", category, normalize_dedupe_key(content));
        if seen.insert(key) {
            deduped.push((*category, content.clone(), tags.clone(), *confidence));
        }
    }
    deduped
}

fn normalize_dedupe_key(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .take(96)
        .collect()
}

fn summarize_session_candidates(
    raw: &[(MemoryCategory, String, Vec<String>, f32)],
    turn_count: usize,
) -> Vec<(MemoryCategory, String, Vec<String>, f32)> {
    let deduped = dedupe_candidates(raw);
    if deduped.is_empty() {
        return Vec::new();
    }

    if deduped.len() == 1 && turn_count <= 2 {
        return deduped;
    }

    let bullets: Vec<String> = deduped
        .iter()
        .map(|(category, content, _, _)| format!("• [{}] {}", category_summary_label(*category), content))
        .collect();
    let avg_confidence = deduped.iter().map(|(_, _, _, confidence)| *confidence).sum::<f32>()
        / deduped.len() as f32;
    let summary = format!(
        "会话总结（{} 轮交流）：\n{}",
        turn_count,
        bullets.join("\n")
    );
    vec![(
        MemoryCategory::Topic,
        clip_line(&summary, 500),
        vec!["session-summary".to_string()],
        avg_confidence.max(0.78),
    )]
}

fn category_summary_label(category: MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::User => "preference",
        MemoryCategory::Feedback => "feedback",
        MemoryCategory::Topic => "project",
        MemoryCategory::Reference => "reference",
    }
}

fn is_substantive_session(pairs: &[(String, String)]) -> bool {
    if pairs.len() < 2 {
        return false;
    }
    let total_user: usize = pairs.iter().map(|(user, _)| user.chars().count()).sum();
    let total_assistant: usize = pairs
        .iter()
        .map(|(_, assistant)| assistant.chars().count())
        .sum();
    total_user >= 80 && total_assistant >= 200
}

fn extract_session_outcome_summary(
    pairs: &[(String, String)],
) -> Option<(MemoryCategory, String, Vec<String>, f32)> {
    if !is_substantive_session(pairs) {
        return None;
    }
    let (_, last_assistant) = pairs.last()?;
    if last_assistant.chars().count() < 80 {
        return None;
    }
    let lower = last_assistant.to_lowercase();
    const OUTCOME_MARKERS: &[&str] = &[
        "fixed",
        "resolved",
        "completed",
        "summary",
        "root cause",
        "总结",
        "已完成",
        "修复",
        "原因是",
        "结论是",
        "经验",
    ];
    if !OUTCOME_MARKERS.iter().any(|marker| lower.contains(marker)) {
        return None;
    }
    let paragraph = first_meaningful_paragraph(last_assistant);
    Some((
        MemoryCategory::Topic,
        clip_line(&format!("会话结论：{}", paragraph), 320),
        vec!["session-outcome".to_string()],
        0.78,
    ))
}

fn first_meaningful_paragraph(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .find(|paragraph| paragraph.chars().count() >= 24)
        .or_else(|| text.lines().map(str::trim).find(|line| !line.is_empty()))
        .unwrap_or(text)
        .to_string()
}

fn session_is_worth_saving(
    items: &[(MemoryCategory, String, Vec<String>, f32)],
    turn_count: usize,
) -> bool {
    if items.iter().any(|(category, _, _, confidence)| {
        matches!(
            category,
            MemoryCategory::User | MemoryCategory::Feedback
        ) && *confidence >= MIN_PROPOSAL_CONFIDENCE
    }) {
        return true;
    }

    if turn_count >= 2
        && items
            .iter()
            .any(|(_, _, tags, confidence)| tags.iter().any(|tag| tag == "session-summary") && *confidence >= 0.76)
    {
        return true;
    }

    items
        .iter()
        .map(|(_, _, _, confidence)| *confidence)
        .fold(0.0_f32, f32::max)
        >= 0.80
}

const PREFERENCE_MARKERS: &[&str] = &[
    "i prefer",
    "i always",
    "please always",
    "don't ",
    "do not ",
    "never ",
    "我喜欢",
    "我偏好",
    "请始终",
    "不要",
    "以后都",
];

const FEEDBACK_MARKERS: &[&str] = &[
    "that's wrong",
    "that is wrong",
    "actually",
    "actually ",
    "instead",
    "instead ",
    "correction",
    "不对",
    "错了",
    "应该是",
    "其实是",
];

fn clip_line(value: &str, max_chars: usize) -> String {
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
    fn extract_user_preference_candidate() {
        let hits = extract_memory_candidates("Please always reply in Chinese", "");
        assert!(hits.iter().any(|(cat, _, _, _)| *cat == MemoryCategory::User));
    }

    #[test]
    fn evaluate_skips_trivial_exchange() {
        assert!(evaluate_memory_proposal("好的", "已完成修改。").is_none());
        assert!(evaluate_memory_proposal("thanks", "You're welcome.").is_none());
    }

    #[test]
    fn evaluate_skips_generic_conversation() {
        let user = "Can you help me refactor this function to be shorter?";
        let assistant = "Sure. I would extract the validation logic into a helper and keep the main flow linear.";
        assert!(evaluate_memory_proposal(user, assistant).is_none());
    }

    #[test]
    fn evaluate_accepts_preference_and_feedback() {
        assert!(evaluate_memory_proposal("Please always reply in Chinese", "").is_some());
    }

    #[test]
    fn evaluate_accepts_weak_feedback_without_validation() {
        assert!(evaluate_memory_proposal("actually ", "ok").is_some());
    }

    #[test]
    fn evaluate_structured_correction_when_user_is_right() {
        let hits = evaluate_memory_proposal(
            "不对，local TimerManager 会破坏全局 _G，应该保持全局赋值",
            "你说得对，我之前理解错了，会改回全局赋值。",
        )
        .expect("correction proposal");
        let feedback = hits
            .iter()
            .find(|(cat, _, _, _)| *cat == MemoryCategory::Feedback)
            .expect("feedback item");
        assert!(feedback.1.contains("【错误原因】"));
        assert!(feedback.1.contains("【如何避免】"));
        assert!(feedback.2.iter().any(|tag| tag == "user-validated"));
    }

    #[test]
    fn evaluate_accepts_explicit_remember_intent() {
        let assistant = "The project stores session state in SQLite and syncs memory docs to Markdown when applied.";
        assert!(evaluate_memory_proposal("请记住这个架构说明", assistant).is_some());
    }

    #[test]
    fn evaluate_accepts_project_topic_markers() {
        assert!(evaluate_memory_proposal(
            "In this codebase we use event sourcing for session history.",
            "",
        )
        .is_some());
    }

    fn user_message(content: &str) -> ChatMessage {
        ChatMessage {
            id: format!("u_{}", content.len()),
            role: MessageRole::User,
            content: content.to_string(),
            created_at: 1,
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

    fn assistant_message(content: &str) -> ChatMessage {
        ChatMessage {
            id: format!("a_{}", content.len()),
            role: MessageRole::Assistant,
            content: content.to_string(),
            created_at: 2,
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
    fn session_proposal_skips_short_generic_chat() {
        let messages = vec![
            user_message("help me rename this function"),
            assistant_message("Rename it to `loadSessionState` and update imports."),
        ];
        assert!(evaluate_memory_proposal_from_session(&messages).is_none());
    }

    #[test]
    fn session_proposal_summarizes_preferences() {
        let messages = vec![
            user_message("Please always reply in Chinese"),
            assistant_message("好的，后续我会使用简体中文回复。"),
            user_message("In this codebase we use SQLite for session storage"),
            assistant_message("Understood. I will keep SQLite as the source of truth for sessions."),
        ];
        let proposal = evaluate_memory_proposal_from_session(&messages).expect("session proposal");
        assert_eq!(proposal.len(), 1);
        assert_eq!(proposal[0].0, MemoryCategory::Topic);
        assert!(proposal[0].1.contains("会话总结"));
    }
}
