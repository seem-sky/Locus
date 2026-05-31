use chrono::{DateTime, Utc};

use crate::memory::models::{MemoryCategory, MemoryEntry, MemoryScope, DEFAULT_PIN_WEIGHT};

const SCOPE_PREFIX: &str = "locus-scope:";
const CATEGORY_PREFIX: &str = "locus-category:";
const PINNED_TAG: &str = "locus:pinned";

pub fn normalize_project_path(working_dir: &str) -> String {
    let trimmed = working_dir.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    std::path::Path::new(trimmed)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| trimmed.replace('\\', "/"))
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
