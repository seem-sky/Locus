use serde_json::Value;

use super::mapping::normalize_project_path;
use super::mapping;
use super::AgentMemoryState;

impl AgentMemoryState {
    pub fn list_sessions(&self) -> Result<Value, String> {
        self.ensure_ready()?;
        self.client.list_sessions()
    }

    pub fn fetch_patterns(&self, working_dir: &str) -> Result<Value, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        if project.is_empty() {
            return Err("working directory is required".to_string());
        }
        self.client.fetch_patterns(&project).map(mapping::filter_patterns_response)
    }

    pub fn fetch_timeline(
        &self,
        working_dir: &str,
        anchor: &str,
        before: Option<usize>,
        after: Option<usize>,
    ) -> Result<Value, String> {
        self.ensure_ready()?;
        let anchor = anchor.trim();
        if anchor.is_empty() {
            return Err("anchor is required".to_string());
        }
        let project = normalize_project_path(working_dir);
        let project_ref = if project.is_empty() {
            None
        } else {
            Some(project.as_str())
        };
        self.client
            .fetch_timeline(anchor, project_ref, before, after)
    }

    pub fn fetch_profile(&self, working_dir: &str, refresh: bool) -> Result<Value, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        if project.is_empty() {
            return Err("working directory is required".to_string());
        }
        let body = self.client.fetch_profile(&project, refresh)?;
        normalize_profile_response(&body)
    }

    pub fn fetch_config_flags(&self) -> Result<Value, String> {
        self.ensure_ready()?;
        self.client.fetch_config_flags()
    }

    pub fn fetch_file_history(
        &self,
        files: &[String],
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        self.ensure_ready()?;
        if files.is_empty() {
            return Err("files is required".to_string());
        }
        self.client.fetch_file_context(files, session_id)
    }

    pub fn fetch_next_action(&self, working_dir: &str) -> Result<Value, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        let project_ref = if project.is_empty() {
            None
        } else {
            Some(project.as_str())
        };
        self.client.fetch_next(project_ref, Some("locus"))
    }

    pub fn run_consolidate(
        &self,
        tier: Option<&str>,
        force: Option<bool>,
    ) -> Result<Value, String> {
        self.ensure_ready()?;
        self.client.run_consolidate_pipeline(tier, force)
    }

    pub fn run_consolidate_memories(
        &self,
        project: Option<&str>,
        min_observations: Option<usize>,
    ) -> Result<Value, String> {
        self.ensure_ready()?;
        self.client.run_consolidate_memories(project, min_observations)
    }

    pub fn query_graph(
        &self,
        query: Option<&str>,
        start_node_id: Option<&str>,
        node_type: Option<&str>,
        max_depth: Option<usize>,
    ) -> Result<Value, String> {
        self.ensure_ready()?;
        let mut body = serde_json::json!({});
        if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
            body["query"] = serde_json::json!(query);
        }
        if let Some(start_node_id) = start_node_id.filter(|value| !value.trim().is_empty()) {
            body["startNodeId"] = serde_json::json!(start_node_id);
        }
        if let Some(node_type) = node_type.filter(|value| !value.trim().is_empty()) {
            body["nodeType"] = serde_json::json!(node_type);
        }
        if let Some(max_depth) = max_depth {
            body["maxDepth"] = serde_json::json!(max_depth);
        }
        self.client.graph_query(body)
    }

    pub fn fetch_graph_stats(&self) -> Result<Value, String> {
        self.ensure_ready()?;
        let body = self.client.graph_stats()?;
        Ok(normalize_graph_stats_response(&body))
    }

    pub fn forget_memory_by_id(&self, memory_id: &str) -> Result<Value, String> {
        self.ensure_ready()?;
        let memory_id = memory_id.trim();
        if memory_id.is_empty() {
            return Err("memoryId is required".to_string());
        }
        self.client.forget(memory_id)
    }

    pub fn evolve_memory_entry(
        &self,
        memory_id: &str,
        new_content: &str,
        new_title: Option<&str>,
    ) -> Result<Value, String> {
        self.ensure_ready()?;
        let memory_id = memory_id.trim();
        let new_content = new_content.trim();
        if memory_id.is_empty() || new_content.is_empty() {
            return Err("memoryId and newContent are required".to_string());
        }
        self.client
            .evolve_memory(memory_id, new_content, new_title)
    }

    pub fn list_linked_commits(
        &self,
        branch: Option<&str>,
        repo: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value, String> {
        self.ensure_ready()?;
        self.client.list_commits(branch, repo, limit)
    }

    pub fn lookup_session_by_commit(&self, sha: &str) -> Result<Value, String> {
        self.ensure_ready()?;
        let sha = sha.trim();
        if sha.is_empty() {
            return Err("sha is required".to_string());
        }
        self.client.session_by_commit(sha)
    }

    pub fn list_session_observations(&self, session_id: &str) -> Result<Value, String> {
        self.ensure_ready()?;
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Err("sessionId is required".to_string());
        }
        self.client.list_observations(session_id)
    }
}

fn normalize_profile_response(body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let profile = body.get("profile").unwrap_or(body);
    if profile.is_null() {
        let reason = body
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("no_profile");
        return Err(format!("agentmemory profile unavailable: {reason}"));
    }
    let top_concepts = profile
        .get("topConcepts")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("concept")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let top_files = profile
        .get("topFiles")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("file")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let recent_activity = profile
        .get("recentActivity")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let summary = recent_activity
        .first()
        .cloned()
        .or_else(|| {
            profile
                .get("conventions")
                .and_then(|v| v.as_array())
                .and_then(|items| items.first())
                .and_then(|item| item.as_str())
                .map(str::to_string)
        });
    Ok(serde_json::json!({
        "project": profile.get("project"),
        "sessionCount": profile.get("sessionCount"),
        "totalObservations": profile.get("totalObservations"),
        "concepts": top_concepts,
        "topFiles": top_files,
        "conventions": profile.get("conventions"),
        "commonErrors": profile.get("commonErrors"),
        "recentActivity": recent_activity,
        "summary": summary,
        "updatedAt": profile.get("updatedAt"),
        "cached": body.get("cached").and_then(|v| v.as_bool()).unwrap_or(false),
    }))
}

fn normalize_graph_stats_response(body: &serde_json::Value) -> serde_json::Value {
    let node_count = body
        .get("totalNodes")
        .or_else(|| body.get("nodeCount"))
        .and_then(|v| v.as_u64());
    let edge_count = body
        .get("totalEdges")
        .or_else(|| body.get("edgeCount"))
        .and_then(|v| v.as_u64());
    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(node_count.unwrap_or(0) > 0 || edge_count.unwrap_or(0) > 0);
    serde_json::json!({
        "nodeCount": node_count,
        "edgeCount": edge_count,
        "nodesByType": body.get("nodesByType"),
        "edgesByType": body.get("edgesByType"),
        "enabled": enabled,
        "healthy": enabled && node_count.unwrap_or(0) > 0,
    })
}
