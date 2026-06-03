use chrono::{DateTime, Utc};

use crate::memory::models::{MemoryCategory, MemoryEntry, MemoryScope, DEFAULT_PIN_WEIGHT};

const SCOPE_PREFIX: &str = "locus-scope:";
const CATEGORY_PREFIX: &str = "locus-category:";
const PINNED_TAG: &str = "locus:pinned";

/// Claude Code / agentmemory hook names — not user-facing memory content.
const OBSERVATION_HOOK_MARKERS: &[&str] = &[
    "UserPromptSubmit",
    "PostToolUse",
    "PostToolUseFailure",
    "PreToolUse",
    "SessionStart",
    "SessionEnd",
    "Stop",
    "SubagentStop",
    "Notification",
    "PermissionRequest",
];

pub fn is_observation_hook_noise(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    OBSERVATION_HOOK_MARKERS
        .iter()
        .any(|marker| trimmed.eq_ignore_ascii_case(marker))
}

pub fn should_include_memory_content(content: &str) -> bool {
    !is_observation_hook_noise(content)
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
        assert!(!is_observation_hook_noise("所有会话使用简体中文"));
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
