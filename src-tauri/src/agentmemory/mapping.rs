use chrono::{DateTime, Utc};

use crate::memory::models::{MemoryCategory, MemoryEntry, MemoryScope, DEFAULT_PIN_WEIGHT};

const SCOPE_PREFIX: &str = "locus-scope:";
const CATEGORY_PREFIX: &str = "locus-category:";
const PINNED_TAG: &str = "locus:pinned";

/// Hook labels / titles from observations — not user-facing memory content.
const OBSERVATION_HOOK_MARKERS: &[&str] = &[
    "UserPromptSubmit",
    "prompt_submit",
    "PostToolUse",
    "post_tool_use",
    "PostToolUseFailure",
    "post_tool_failure",
    "PreToolUse",
    "pretooluse",
    "pre_tool_use",
    "SessionStart",
    "session_start",
    "SessionEnd",
    "session_end",
    "Stop",
    "stop",
    "SubagentStop",
    "subagent_stop",
    "Notification",
    "notification",
    "PermissionRequest",
    "permission_request",
];

/// LLM-compressed hook lifecycle narratives — not user-facing memory content.
const OBSERVATION_HOOK_NARRATIVE_MARKERS: &[&str] = &[
    "pre-tool-use hook",
    "pre_tool_use hook",
    "pretooluse hook",
    "hook fired before tool execution with no payload",
    "about to execute a tool. however, the observation contains no details",
    "post-tool-use failure hook",
    "post_tool_failure hook",
    "posttoolusefailure hook",
    "post tool use failure hook",
    "hook fired with no tool",
    "hook triggered with no",
    "hook triggered",
    "hook fired",
    "with no details",
    "no details provided",
    "no tool call content or payload",
    "hook lifecycle event",
    "hook notification",
    "hook event lifecycle",
    "hook event with no",
    "minimal hook notification",
    "no actionable content",
    "no associated tool",
    "no tool details",
    "standalone hook firing",
    "bare hook trigger",
    "observation payload",
    "pre-compact hook",
    "pre_compact hook",
    "recurring error:",
];

const OBSERVE_TOOL_OUTPUT_MAX_CHARS: usize = 8_000;

pub fn enrich_targets_for_tool(
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> (Vec<String>, Vec<String>) {
    let normalized = tool_name.trim().to_ascii_lowercase();
    let enrichable = matches!(
        normalized.as_str(),
        "read" | "write" | "edit" | "grep" | "list" | "glob"
    );
    if !enrichable {
        return (Vec::new(), Vec::new());
    }

    let mut files = Vec::new();
    let mut terms = Vec::new();
    let file_keys = if normalized == "grep" {
        &["path", "file"][..]
    } else {
        &["filePath", "file_path", "path", "file", "pattern"][..]
    };
    for key in file_keys {
        if let Some(value) = tool_input.get(*key).and_then(|v| v.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                files.push(trimmed.to_string());
            }
        }
    }
    if normalized == "grep" || normalized == "glob" {
        if let Some(pattern) = tool_input.get("pattern").and_then(|v| v.as_str()) {
            let trimmed = pattern.trim();
            if !trimmed.is_empty() {
                terms.push(trimmed.to_string());
            }
        }
    }
    (files, terms)
}

pub fn extract_smart_search_result_content(result: &serde_json::Value) -> Option<String> {
    if let Some(content) = extract_search_result_content(result) {
        return Some(content);
    }
    let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
    if should_include_memory_content(title) {
        return Some(title.to_string());
    }
    None
}

pub fn observe_tool_payload(
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_output: &str,
    is_error: bool,
) -> (&'static str, serde_json::Value) {
    let tool_input = tool_input.clone();
    let output = truncate_observe_text(tool_output, OBSERVE_TOOL_OUTPUT_MAX_CHARS);
    if is_error {
        (
            "post_tool_failure",
            serde_json::json!({
                "tool_name": tool_name,
                "tool_input": tool_input,
                "error": output,
            }),
        )
    } else {
        (
            "post_tool_use",
            serde_json::json!({
                "tool_name": tool_name,
                "tool_input": tool_input,
                "tool_output": output,
            }),
        )
    }
}

fn truncate_observe_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let truncated: String = value.chars().take(max_chars).collect();
    format!("{truncated}\n[...truncated]")
}

pub fn is_observation_hook_noise(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    if OBSERVATION_HOOK_MARKERS
        .iter()
        .any(|marker| trimmed.eq_ignore_ascii_case(marker))
    {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    OBSERVATION_HOOK_NARRATIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

pub fn should_include_memory_content(content: &str) -> bool {
    !is_observation_hook_noise(content)
}

/// Pattern API / cross-session summaries that describe hook lifecycle noise, not project facts.
pub fn is_memory_pattern_noise(value: &str) -> bool {
    is_observation_hook_noise(value)
}

/// Locus records tool activity via `post_tool_use` and session-tree replay; pre_tool_use only creates hook-noise cards.
pub fn should_observe_pre_tool_use(_tool_name: &str, _tool_input: &serde_json::Value) -> bool {
    false
}

pub fn should_observe_tool_failure(tool_output: &str) -> bool {
    let trimmed = tool_output.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed == crate::session::history::INTERRUPTED_TOOL_RESULT {
        return false;
    }
    if is_observation_hook_noise(trimmed) {
        return false;
    }
    // Ignore generic one-liners with no diagnostic value.
    let lower = trimmed.to_ascii_lowercase();
    if lower == "error" || lower == "failed" || lower == "tool error" {
        return false;
    }
    true
}

pub fn filter_patterns_response(mut body: serde_json::Value) -> serde_json::Value {
    if let Some(patterns) = body.get_mut("patterns").and_then(|v| v.as_array_mut()) {
        patterns.retain(|item| {
            pattern_item_text(item)
                .map(|text| !is_memory_pattern_noise(&text))
                .unwrap_or(true)
        });
    }
    body
}

fn pattern_item_text(item: &serde_json::Value) -> Option<String> {
    if let Some(text) = item.as_str() {
        return Some(text.to_string());
    }
    let object = item.as_object()?;
    for key in ["description", "title", "pattern", "narrative", "summary", "text"] {
        if let Some(text) = object.get(key).and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn action_project_matches_workspace(
    action_project: Option<&str>,
    normalized_workspace: &str,
) -> bool {
    let action_project = action_project.map(str::trim).filter(|value| !value.is_empty());
    let Some(action_project) = action_project else {
        return true;
    };
    if normalized_workspace.is_empty() {
        return true;
    }
    if action_project == normalized_workspace {
        return true;
    }
    let action_name = std::path::Path::new(action_project)
        .file_name()
        .and_then(|name| name.to_str());
    let workspace_name = std::path::Path::new(normalized_workspace)
        .file_name()
        .and_then(|name| name.to_str());
    match (action_name, workspace_name) {
        (Some(a), Some(w)) => a.eq_ignore_ascii_case(w),
        _ => false,
    }
}

pub fn extract_search_result_content(result: &serde_json::Value) -> Option<String> {
    let observation = result.get("observation");
    let candidates = [
        result.get("narrative").and_then(|v| v.as_str()),
        result.get("title").and_then(|v| v.as_str()),
        observation
            .and_then(|o| o.get("narrative"))
            .and_then(|v| v.as_str()),
        observation
            .and_then(|o| o.get("title"))
            .and_then(|v| v.as_str()),
        observation
            .and_then(|o| o.get("data"))
            .and_then(|d| d.get("prompt"))
            .and_then(|v| v.as_str()),
    ];
    for candidate in candidates {
        if let Some(text) = candidate {
            let trimmed = text.trim();
            if should_include_memory_content(trimmed) {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(facts) = observation
        .and_then(|o| o.get("facts"))
        .and_then(|v| v.as_array())
    {
        let joined: Vec<&str> = facts
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| should_include_memory_content(s))
            .collect();
        if !joined.is_empty() {
            return Some(joined.join("; "));
        }
    }
    None
}

pub fn search_result_category(result: &serde_json::Value) -> MemoryCategory {
    let obs_type = result
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| {
            result
                .get("observation")
                .and_then(|o| o.get("type"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("fact");
    agent_type_to_category(obs_type)
}

/// Stable workspace path for agentmemory HTTP APIs (no `\\?\` extended-length prefix).
pub fn normalize_project_path(working_dir: &str) -> String {
    let trimmed = working_dir.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let path = dunce::canonicalize(trimmed)
        .unwrap_or_else(|_| std::path::PathBuf::from(trimmed));
    crate::process_util::windows_command_path(&path)
}

pub fn category_to_agent_type(category: MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::User => "preference",
        MemoryCategory::Feedback => "bug",
        MemoryCategory::Topic => "architecture",
        MemoryCategory::Reference => "fact",
    }
}

pub fn agent_type_to_category(value: &str) -> MemoryCategory {
    match value.trim().to_lowercase().as_str() {
        "preference" => MemoryCategory::User,
        "bug" => MemoryCategory::Feedback,
        "architecture" | "workflow" | "pattern" => MemoryCategory::Topic,
        _ => MemoryCategory::Reference,
    }
}

pub fn scope_from_concepts(concepts: &[String]) -> Option<MemoryScope> {
    for concept in concepts {
        if let Some(raw) = concept.strip_prefix(SCOPE_PREFIX) {
            return MemoryScope::from_str(raw);
        }
    }
    None
}

pub fn category_from_concepts(concepts: &[String]) -> Option<MemoryCategory> {
    for concept in concepts {
        if let Some(raw) = concept.strip_prefix(CATEGORY_PREFIX) {
            return MemoryCategory::from_str(raw);
        }
    }
    None
}

pub fn is_pinned_concepts(concepts: &[String], strength: i32) -> bool {
    concepts.iter().any(|c| c == PINNED_TAG) || strength >= 9
}

pub fn build_concepts(
    category: MemoryCategory,
    scope: MemoryScope,
    pinned: bool,
    tags: &[String],
) -> Vec<String> {
    let mut concepts = vec![
        format!("{}{}", SCOPE_PREFIX, scope.as_str()),
        format!("{}{}", CATEGORY_PREFIX, category.as_str()),
    ];
    if pinned {
        concepts.push(PINNED_TAG.to_string());
    }
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !concepts.iter().any(|existing| existing == trimmed) {
            concepts.push(trimmed.to_string());
        }
    }
    concepts
}

pub fn user_tags_from_concepts(concepts: &[String]) -> Vec<String> {
    concepts
        .iter()
        .filter(|concept| {
            !concept.starts_with(SCOPE_PREFIX)
                && !concept.starts_with(CATEGORY_PREFIX)
                && concept.as_str() != PINNED_TAG
        })
        .cloned()
        .collect()
}

fn parse_iso_ms(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|_| {
            value
                .parse::<i64>()
                .unwrap_or_else(|_| Utc::now().timestamp_millis())
        })
}

pub fn remote_memory_to_entry(
    remote: &serde_json::Value,
    fallback_scope: MemoryScope,
    fallback_project: &str,
) -> Option<MemoryEntry> {
    let id = remote.get("id")?.as_str()?.to_string();
    let content = remote.get("content")?.as_str()?.to_string();
    if !should_include_memory_content(&content) {
        return None;
    }
    let concepts = remote
        .get("concepts")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let remote_type = remote
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("fact");
    let strength = remote.get("strength").and_then(|v| v.as_i64()).unwrap_or(7) as i32;
    let project = remote
        .get("project")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let scope = scope_from_concepts(&concepts).unwrap_or_else(|| {
        if project.is_empty() {
            MemoryScope::User
        } else if fallback_project.is_empty() || project == fallback_project {
            fallback_scope
        } else {
            MemoryScope::Project
        }
    });
    let category =
        category_from_concepts(&concepts).unwrap_or_else(|| agent_type_to_category(remote_type));
    let pinned = is_pinned_concepts(&concepts, strength);
    let created_at = remote
        .get("createdAt")
        .and_then(|v| v.as_str())
        .map(parse_iso_ms)
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    let updated_at = remote
        .get("updatedAt")
        .and_then(|v| v.as_str())
        .map(parse_iso_ms)
        .unwrap_or(created_at);
    let source_session_id = remote
        .get("sessionIds")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.as_str())
        .map(str::to_string);

    Some(MemoryEntry {
        id,
        category,
        scope,
        content,
        tags: user_tags_from_concepts(&concepts),
        pinned,
        pin_weight: if pinned {
            DEFAULT_PIN_WEIGHT
        } else {
            (strength * 10).clamp(1, DEFAULT_PIN_WEIGHT)
        },
        access_count: 0,
        last_accessed_at: 0,
        created_at,
        updated_at,
        source_session_id,
        linked_doc_path: None,
    })
}

/// Map a successful `/agentmemory/remember` payload, falling back to the local entry when
/// agentmemory returns hook-noise or sparse content that `remote_memory_to_entry` skips.
pub fn entry_from_remember_response(
    entry: MemoryEntry,
    body: &serde_json::Value,
    fallback_scope: MemoryScope,
    fallback_project: &str,
) -> Result<MemoryEntry, String> {
    let memory = body.get("memory").unwrap_or(body);
    if let Some(mapped) = remote_memory_to_entry(memory, fallback_scope, fallback_project) {
        return Ok(mapped);
    }
    let id = memory
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "agentmemory remember succeeded but response had no memory id".to_string()
        })?;
    let created_at = memory
        .get("createdAt")
        .and_then(|v| v.as_str())
        .map(parse_iso_ms)
        .unwrap_or(entry.created_at);
    let updated_at = memory
        .get("updatedAt")
        .and_then(|v| v.as_str())
        .map(parse_iso_ms)
        .unwrap_or(created_at);
    Ok(MemoryEntry {
        id: id.to_string(),
        created_at,
        updated_at,
        ..entry
    })
}

pub fn entry_matches_filter(entry: &MemoryEntry, filter: &crate::memory::models::MemoryListFilter) -> bool {
    if let Some(category) = filter.category {
        if entry.category != category {
            return false;
        }
    }
    if let Some(scope) = filter.scope {
        if entry.scope != scope {
            return false;
        }
    }
    if let Some(tags) = &filter.tags {
        if !tags.is_empty() && !tags.iter().all(|tag| entry.tags.iter().any(|t| t == tag)) {
            return false;
        }
    }
    if let Some(query) = &filter.query {
        let q = query.trim().to_lowercase();
        if !q.is_empty() {
            let haystack = format!(
                "{} {}",
                entry.content.to_lowercase(),
                entry.tags.join(" ").to_lowercase()
            );
            if !haystack.contains(&q) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_names_are_filtered_as_noise() {
        assert!(is_observation_hook_noise("UserPromptSubmit"));
        assert!(is_observation_hook_noise("  PostToolUse  "));
        assert!(is_observation_hook_noise("post_tool_use"));
        assert!(is_observation_hook_noise(
            "Pre-tool-use hook triggered. Hook fired with no tool call details provided."
        ));
        assert!(is_observation_hook_noise(
            "A pre_tool_use hook event was triggered. No associated tool invocation details."
        ));
        assert!(is_observation_hook_noise(
            "Hook notification for incoming tool execution. No tool details or payload available."
        ));
        assert!(is_observation_hook_noise(
            "Recurring error: posttoolusefailure hook triggered with no details"
        ));
        assert!(!is_observation_hook_noise("所有会话使用简体中文"));
    }

    #[test]
    fn pre_tool_use_never_observed_from_locus() {
        assert!(!should_observe_pre_tool_use("", &serde_json::json!({})));
        assert!(!should_observe_pre_tool_use("   ", &serde_json::json!({"path": "a"})));
        assert!(!should_observe_pre_tool_use("pre_tool_use", &serde_json::json!({})));
        assert!(!should_observe_pre_tool_use("PreToolUse", &serde_json::json!({})));
        assert!(!should_observe_pre_tool_use("notification", &serde_json::json!({})));
        assert!(!should_observe_pre_tool_use("read", &serde_json::json!({})));
        assert!(!should_observe_pre_tool_use(
            "read",
            &serde_json::json!({"path": "README.md"}),
        ));
    }

    #[test]
    fn tool_failure_observation_skips_empty_and_hook_noise() {
        assert!(!should_observe_tool_failure(""));
        assert!(!should_observe_tool_failure("   "));
        assert!(!should_observe_tool_failure(
            "PostToolUseFailure hook triggered with no details"
        ));
        assert!(should_observe_tool_failure(
            "unity_execute failed: NullReferenceException at PhoneLoginComponent.cs:42"
        ));
    }

    #[test]
    fn pattern_response_filters_hook_noise() {
        let body = serde_json::json!({
            "patterns": [
                "H:/exas game/Assets.Lua/Halmodels/oginNiewui/Phonel ogincomponent.lua frequently modified",
                "Recurring error: posttoolusefailure hook triggered",
                { "description": "Recurring error: posttoolusefailure hook triggered with no details" }
            ]
        });
        let filtered = filter_patterns_response(body);
        let patterns = filtered["patterns"].as_array().unwrap();
        assert_eq!(patterns.len(), 1);
    }

    #[test]
    fn action_project_matches_workspace_by_full_path_or_basename() {
        assert!(action_project_matches_workspace(
            Some(r"G:\AI\Locus"),
            r"G:\AI\Locus",
        ));
        assert!(action_project_matches_workspace(
            Some("Locus"),
            r"G:\AI\Locus",
        ));
        assert!(!action_project_matches_workspace(
            Some(r"G:\AI\Other"),
            r"G:\AI\Locus",
        ));
        assert!(action_project_matches_workspace(None, r"G:\AI\Locus"));
    }

    #[test]
    fn observe_tool_payload_uses_agentmemory_hook_types() {
        let (hook, data) = observe_tool_payload(
            "bash",
            &serde_json::json!({ "command": "git status" }),
            "Exit code: 0",
            false,
        );
        assert_eq!(hook, "post_tool_use");
        assert_eq!(data["tool_name"], "bash");
        assert_eq!(data["tool_output"], "Exit code: 0");

        let (hook, data) = observe_tool_payload("read", &serde_json::json!({}), "not found", true);
        assert_eq!(hook, "post_tool_failure");
        assert_eq!(data["tool_name"], "read");
        assert_eq!(data["error"], "not found");
        assert!(data.get("tool_output").is_none());
    }

    #[test]
    fn enrich_targets_extract_read_and_grep_paths() {
        let (files, terms) = enrich_targets_for_tool(
            "read",
            &serde_json::json!({ "filePath": "src/main.rs" }),
        );
        assert_eq!(files, vec!["src/main.rs"]);
        assert!(terms.is_empty());

        let (files, terms) = enrich_targets_for_tool(
            "grep",
            &serde_json::json!({ "path": "src", "pattern": "AgentMemory" }),
        );
        assert_eq!(files, vec!["src"]);
        assert_eq!(terms, vec!["AgentMemory"]);
    }

    #[test]
    fn normalize_project_path_strips_extended_prefix_on_windows() {
        let temp = tempfile::tempdir().expect("tempdir");
        let normalized = normalize_project_path(temp.path().to_str().expect("utf8 path"));
        assert!(!normalized.is_empty());
        assert!(!normalized.starts_with(r"\\?\"));
        assert!(!normalized.starts_with("//?/"));
    }
}

pub fn entry_belongs_to_workspace(
    entry: &MemoryEntry,
    memory_project: Option<&str>,
    normalized_project: &str,
) -> bool {
    match entry.scope {
        MemoryScope::User => true,
        MemoryScope::Project => {
            if normalized_project.is_empty() {
                return false;
            }
            match memory_project.map(str::trim).filter(|value| !value.is_empty()) {
                Some(project) => project == normalized_project,
                None => true,
            }
        }
    }
}
