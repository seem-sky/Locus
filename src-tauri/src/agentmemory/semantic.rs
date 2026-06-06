use serde_json::{json, Value};

use super::client::AgentMemoryClient;
use super::mapping::{action_project_matches_workspace, normalize_project_path, should_include_memory_content};

/// Extract semantic fact payloads from a session summary object (no LLM).
pub fn semantic_facts_from_summary(summary: &Value) -> Vec<Value> {
    let mut facts = Vec::new();

    if let Some(decisions) = summary.get("keyDecisions").and_then(|v| v.as_array()) {
        for decision in decisions {
            if let Some(text) = decision
                .as_str()
                .map(str::trim)
                .filter(|value| should_include_memory_content(value) && value.len() >= 8)
            {
                facts.push(json!({
                    "fact": text,
                    "confidence": 0.62,
                }));
            }
        }
    }
    if let Some(concepts) = summary.get("concepts").and_then(|v| v.as_array()) {
        for concept in concepts {
            if let Some(text) = concept
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.len() >= 3)
            {
                facts.push(json!({
                    "fact": text,
                    "confidence": 0.5,
                }));
            }
        }
    }
    if let Some(title) = summary
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| should_include_memory_content(value) && value.len() >= 8)
    {
        facts.push(json!({
            "fact": title,
            "confidence": 0.54,
        }));
    }
    if let Some(narrative) = summary
        .get("narrative")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| value.len() >= 24)
    {
        let truncated = if narrative.chars().count() > 240 {
            format!("{}…", narrative.chars().take(240).collect::<String>())
        } else {
            narrative.to_string()
        };
        facts.push(json!({
            "fact": truncated,
            "confidence": 0.58,
        }));
    }

    facts.truncate(16);
    facts
}

pub fn upsert_semantic_facts_checked(
    client: &AgentMemoryClient,
    facts: &[Value],
    session_id: Option<&str>,
    project: Option<&str>,
    log_context: &str,
) {
    if facts.is_empty() {
        return;
    }
    let mut body = json!({ "facts": facts });
    if let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        body["sessionId"] = json!(session_id);
    }
    if let Some(project) = project.map(str::trim).filter(|value| !value.is_empty()) {
        body["project"] = json!(project);
    }
    match client.upsert_semantic_facts(body) {
        Ok(response) => {
            if response.get("success").and_then(|v| v.as_bool()) != Some(true) {
                let error = response
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("semantic upsert returned success=false");
                eprintln!("[agentmemory] {log_context}: {error}");
            }
        }
        Err(error) => {
            eprintln!("[agentmemory] {log_context}: {error}");
        }
    }
}

/// Mirror durable memory text into semantic KV (fast path, no LLM).
pub fn mirror_memory_content_to_semantic(
    client: &AgentMemoryClient,
    content: &str,
    project: Option<&str>,
    confidence: f64,
    log_context: &str,
) {
    let trimmed = content.trim();
    if trimmed.len() < 8 || !should_include_memory_content(trimmed) {
        return;
    }
    upsert_semantic_facts_checked(
        client,
        &[json!({
            "fact": trimmed,
            "confidence": confidence,
        })],
        None,
        project,
        log_context,
    );
}

/// Upsert semantic facts from stored session summaries (no LLM, idempotent).
pub fn backfill_semantic_from_stored_summaries_sync(
    client: &AgentMemoryClient,
    working_dir: Option<&str>,
) {
    let Ok(body) = client.list_summaries() else {
        return;
    };
    let Some(summaries) = body.get("summaries").and_then(|v| v.as_array()) else {
        return;
    };
    let project_filter = working_dir
        .map(normalize_project_path)
        .filter(|value| !value.is_empty());

    for summary in summaries {
        if let Some(project) = project_filter.as_deref() {
            let summary_project = summary
                .get("project")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if !action_project_matches_workspace(summary_project, project) {
                continue;
            }
        }
        let facts = semantic_facts_from_summary(summary);
        if facts.is_empty() {
            continue;
        }
        let session_id = summary
            .get("sessionId")
            .or_else(|| summary.get("session_id"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let project = summary
            .get("project")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        upsert_semantic_facts_checked(
            client,
            &facts,
            session_id,
            project,
            &format!(
                "semantic backfill from summary {}",
                session_id.unwrap_or("unknown")
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_facts_from_summary_collects_decisions_and_narrative() {
        let summary = json!({
            "title": "接入 agentmemory semantic",
            "narrative": "Session 结束时把 summarize 里的关键信息写入 semantic KV，避免只靠 LLM consolidate。",
            "keyDecisions": ["直接 upsert 为主路径"],
            "concepts": ["semantic", "summarize"]
        });
        let facts = semantic_facts_from_summary(&summary);
        assert!(facts.len() >= 3);
    }

    #[test]
    fn semantic_facts_from_summary_skips_empty_payload() {
        let summary = json!({ "title": "x", "narrative": "short" });
        assert!(semantic_facts_from_summary(&summary).is_empty());
    }
}
