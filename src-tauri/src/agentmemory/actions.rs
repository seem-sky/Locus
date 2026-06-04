use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::memory::models::{MemoryCategory, MemoryScope};
use crate::session::models::MemoryProposalItem;

use super::mapping::{normalize_project_path, should_include_memory_content};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryAction {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateAgentMemoryActionRequest {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub project: Option<String>,
    pub created_by: Option<String>,
    pub tags: Vec<String>,
    pub parent_id: Option<String>,
    pub requires: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateAgentMemoryActionRequest {
    pub action_id: String,
    pub status: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub result: Option<String>,
}

pub fn parse_action(value: &Value) -> Option<AgentMemoryAction> {
    let id = value.get("id").and_then(|v| v.as_str())?.to_string();
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if title.is_empty() {
        return None;
    }
    Some(AgentMemoryAction {
        id,
        title,
        description: value
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        status: value
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending")
            .to_string(),
        priority: value.get("priority").and_then(|v| v.as_i64()).map(|v| v as i32),
        project: value
            .get("project")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        created_by: value
            .get("createdBy")
            .or_else(|| value.get("created_by"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        tags: value
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        created_at: value
            .get("createdAt")
            .or_else(|| value.get("created_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        updated_at: value
            .get("updatedAt")
            .or_else(|| value.get("updated_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

pub fn parse_action_list(body: &Value) -> Vec<AgentMemoryAction> {
    body.get("actions")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_action).collect())
        .unwrap_or_default()
}

pub fn action_title_from_content(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return "Follow-up".to_string();
    }
    let first_line = trimmed.lines().next().unwrap_or(trimmed).trim();
    let title: String = first_line.chars().take(120).collect();
    if first_line.chars().count() > 120 {
        format!("{title}…")
    } else {
        title
    }
}

pub fn tags_for_proposal_item(item: &MemoryProposalItem) -> Vec<String> {
    let mut tags = item.tags.clone();
    tags.push(format!("locus-category:{}", category_label(item.category)));
    tags.push(format!(
        "locus-scope:{}",
        match item.scope {
            MemoryScope::User => "user",
            MemoryScope::Project => "project",
        }
    ));
    tags.push("locus:memory-proposal".to_string());
    tags
}

fn category_label(category: MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::User => "user",
        MemoryCategory::Topic => "topic",
        MemoryCategory::Feedback => "feedback",
        MemoryCategory::Reference => "reference",
    }
}

/// Parsed session summary → action create requests (parent + optional decision children).
pub struct SummaryActionBatch {
    pub parent: CreateAgentMemoryActionRequest,
    pub children: Vec<CreateAgentMemoryActionRequest>,
}

pub fn summary_action_batch_from_response(
    body: &Value,
    session_id: &str,
    working_dir: &str,
) -> Option<SummaryActionBatch> {
    if body.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return None;
    }
    let summary = body.get("summary")?;
    let title = summary
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if !should_include_memory_content(title) {
        return None;
    }

    let narrative = summary
        .get("narrative")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let key_decisions = string_array(summary.get("keyDecisions"));
    let files_modified = string_array(summary.get("filesModified"));

    let description = build_summary_action_description(&narrative, &key_decisions, &files_modified);
    if description.is_empty() && key_decisions.is_empty() {
        return None;
    }

    let project = normalize_project_path(working_dir);
    let project = if project.is_empty() {
        None
    } else {
        Some(project)
    };
    let created_by = format!("locus:session:{session_id}");
    let session_tag = format!("locus:session-id:{session_id}");

    let parent = CreateAgentMemoryActionRequest {
        title: action_title_from_content(title),
        description: Some(description),
        priority: Some(6),
        project: project.clone(),
        created_by: Some(created_by.clone()),
        tags: vec!["locus:session-summary".to_string(), session_tag],
        parent_id: None,
        requires: Vec::new(),
    };

    let children = key_decisions
        .into_iter()
        .filter(|decision| should_include_memory_content(decision))
        .map(|decision| CreateAgentMemoryActionRequest {
            title: action_title_from_content(&format!("跟进: {decision}")),
            description: Some(decision.clone()),
            priority: Some(5),
            project: project.clone(),
            created_by: Some(created_by.clone()),
            tags: vec!["locus:session-decision".to_string()],
            parent_id: None,
            requires: Vec::new(),
        })
        .collect();

    Some(SummaryActionBatch { parent, children })
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn build_summary_action_description(
    narrative: &str,
    key_decisions: &[String],
    files_modified: &[String],
) -> String {
    let mut parts = Vec::new();
    if !narrative.is_empty() {
        parts.push(narrative.to_string());
    }
    if !key_decisions.is_empty() {
        let joined = key_decisions
            .iter()
            .map(|decision| format!("- {decision}"))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("关键决策:\n{joined}"));
    }
    if !files_modified.is_empty() {
        let joined = files_modified
            .iter()
            .map(|file| format!("- {file}"))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("涉及文件:\n{joined}"));
    }
    parts.join("\n\n")
}

pub fn create_request_from_proposal_item(
    item: &MemoryProposalItem,
    working_dir: &str,
    session_id: &str,
) -> CreateAgentMemoryActionRequest {
    let project = normalize_project_path(working_dir);
    let project = if project.is_empty() {
        None
    } else {
        Some(project)
    };
    CreateAgentMemoryActionRequest {
        title: action_title_from_content(&item.content),
        description: Some(item.content.clone()),
        priority: Some(5),
        project,
        created_by: Some(format!("locus:session:{session_id}")),
        tags: tags_for_proposal_item(item),
        parent_id: None,
        requires: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_title_uses_first_line() {
        assert_eq!(
            action_title_from_content("第一行\n第二行"),
            "第一行"
        );
    }

    #[test]
    fn summary_action_batch_builds_parent_and_decision_children() {
        let body = serde_json::json!({
            "success": true,
            "summary": {
                "title": "接入 agentmemory actions",
                "narrative": "补齐 action 工具并在 session end 自动生成跟进项。",
                "keyDecisions": [
                    "PreToolUse 仅 enrich 不 observe",
                    "Agent 暴露 memory_action_create"
                ],
                "filesModified": ["src-tauri/src/agentmemory/mod.rs"]
            }
        });
        let batch = summary_action_batch_from_response(&body, "sess-1", r"G:\AI\Locus")
            .expect("batch");
        assert!(batch.parent.title.contains("agentmemory"));
        assert_eq!(batch.children.len(), 2);
        assert!(batch.children[0].title.starts_with("跟进:"));
        assert!(batch.parent.tags.iter().any(|tag| tag.contains("sess-1")));
    }

    #[test]
    fn summary_action_batch_skips_failed_summarize() {
        let body = serde_json::json!({ "success": false, "error": "no_observations" });
        assert!(summary_action_batch_from_response(&body, "sess-1", r"G:\AI\Locus").is_none());
    }
}
