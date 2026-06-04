use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{json, Value};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:3111";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct AgentMemoryClient {
    base_url: String,
    secret: Option<String>,
    http: Client,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryHealthStatus {
    pub available: bool,
    pub status: String,
    pub version: Option<String>,
    pub viewer_port: Option<u16>,
    pub error: Option<String>,
    #[serde(skip)]
    pub orphaned_listener: bool,
    #[serde(skip)]
    pub worker_count: usize,
}

impl AgentMemoryClient {
    pub fn from_env() -> Self {
        let base_url = std::env::var("LOCUS_AGENTMEMORY_URL")
            .or_else(|_| std::env::var("AGENTMEMORY_URL"))
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let secret = std::env::var("LOCUS_AGENTMEMORY_SECRET")
            .or_else(|_| std::env::var("AGENTMEMORY_SECRET"))
            .ok()
            .filter(|value| !value.trim().is_empty());
        Self::new(base_url, secret)
    }

    pub fn new(base_url: String, secret: Option<String>) -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            secret,
            http,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(secret) = &self.secret {
            if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", secret)) {
                headers.insert(AUTHORIZATION, value);
            }
        }
        headers
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn send_json(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let method_label = method.as_str().to_string();
        let mut request = self
            .http
            .request(method, self.url(path))
            .headers(self.auth_headers());
        if let Some(payload) = body {
            request = request.json(&payload);
        }
        let response = request
            .send()
            .map_err(|e| format!("agentmemory request failed: {}", e))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|e| format!("agentmemory response read failed: {}", e))?;
        if !status.is_success() {
            return Err(format!(
                "agentmemory {} {} -> {}: {}",
                method_label,
                path,
                status,
                text.chars().take(400).collect::<String>()
            ));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text)
            .map_err(|e| format!("agentmemory invalid JSON from {}: {}", path, e))
    }

    fn probe_get(&self, path: &str) -> Result<(reqwest::StatusCode, Value), String> {
        let response = self
            .http
            .get(self.url(path))
            .headers(self.auth_headers())
            .send()
            .map_err(|e| format!("agentmemory request failed: {}", e))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|e| format!("agentmemory response read failed: {}", e))?;
        if text.trim().is_empty() {
            return Ok((status, Value::Null));
        }
        let body = serde_json::from_str(&text)
            .map_err(|e| format!("agentmemory invalid JSON from {}: {}", path, e))?;
        Ok((status, body))
    }

    fn health_from_body(body: &Value) -> AgentMemoryHealthStatus {
        let status = body
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("healthy")
            .to_string();
        let available = matches!(status.as_str(), "healthy" | "ok");
        let worker_count = body
            .get("health")
            .and_then(|health| health.get("workers"))
            .and_then(|workers| workers.as_array())
            .map(|workers| workers.len())
            .unwrap_or(0);
        AgentMemoryHealthStatus {
            available,
            status,
            version: body
                .get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            viewer_port: body
                .get("viewerPort")
                .and_then(|v| v.as_u64())
                .and_then(|v| u16::try_from(v).ok()),
            error: None,
            orphaned_listener: false,
            worker_count,
        }
    }

    pub fn health(&self) -> AgentMemoryHealthStatus {
        for path in ["/agentmemory/livez", "/agentmemory/health"] {
            match self.probe_get(path) {
                Ok((status, body)) if status.is_success() => {
                    return Self::health_from_body(&body);
                }
                Ok((status, _)) if status.as_u16() == 404 => continue,
                Ok((status, body)) => {
                    return AgentMemoryHealthStatus {
                        available: false,
                        status: "unavailable".to_string(),
                        version: None,
                        viewer_port: None,
                        error: Some(format!(
                            "agentmemory GET {} -> {}: {}",
                            path,
                            status,
                            body.to_string().chars().take(200).collect::<String>()
                        )),
                        orphaned_listener: false,
                        worker_count: 0,
                    };
                }
                Err(error) if path == "/agentmemory/health" => {
                    return AgentMemoryHealthStatus {
                        available: false,
                        status: "unavailable".to_string(),
                        version: None,
                        viewer_port: None,
                        error: Some(error),
                        orphaned_listener: false,
                        worker_count: 0,
                    };
                }
                Err(_) => continue,
            }
        }

        if self.base_url_reachable() {
            AgentMemoryHealthStatus {
                available: false,
                status: "orphaned".to_string(),
                version: None,
                viewer_port: None,
                error: Some(format!(
                    "Port {} responds but agentmemory routes are missing (orphan iii-engine). Restart the service.",
                    self.base_url
                )),
                orphaned_listener: true,
                worker_count: 0,
            }
        } else {
            AgentMemoryHealthStatus {
                available: false,
                status: "unavailable".to_string(),
                version: None,
                viewer_port: None,
                error: Some(format!(
                    "agentmemory server is not running at {}",
                    self.base_url
                )),
                orphaned_listener: false,
                worker_count: 0,
            }
        }
    }

    fn base_url_reachable(&self) -> bool {
        self.http
            .get(&self.base_url)
            .send()
            .map(|response| response.status().as_u16() != 0)
            .unwrap_or(false)
    }

    pub fn session_start(
        &self,
        session_id: &str,
        project: &str,
        cwd: &str,
        title: Option<&str>,
    ) -> Result<Value, String> {
        let mut body = json!({
            "sessionId": session_id,
            "project": project,
            "cwd": cwd,
            "agentId": "locus",
        });
        if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
            body["title"] = json!(title);
        }
        self.send_json(reqwest::Method::POST, "/agentmemory/session/start", Some(body))
    }

    pub fn session_end(&self, session_id: &str) -> Result<Value, String> {
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/session/end",
            Some(json!({ "sessionId": session_id })),
        )
    }

    pub fn summarize_session(&self, session_id: &str) -> Result<Value, String> {
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/summarize",
            Some(json!({ "sessionId": session_id })),
        )
    }

    pub fn observe(
        &self,
        hook_type: &str,
        session_id: &str,
        project: &str,
        cwd: &str,
        data: Value,
    ) -> Result<Value, String> {
        let timestamp = chrono::Utc::now().to_rfc3339();
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/observe",
            Some(json!({
                "hookType": hook_type,
                "sessionId": session_id,
                "project": project,
                "cwd": cwd,
                "timestamp": timestamp,
                "data": data,
            })),
        )
    }

    pub fn remember(
        &self,
        content: &str,
        mem_type: &str,
        concepts: &[String],
        project: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Value, String> {
        let mut body = json!({
            "content": content,
            "type": mem_type,
            "concepts": concepts,
        });
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            body["project"] = json!(project);
        }
        if let Some(agent_id) = agent_id.filter(|value| !value.trim().is_empty()) {
            body["agentId"] = json!(agent_id);
        }
        self.send_json(reqwest::Method::POST, "/agentmemory/remember", Some(body))
    }

    pub fn forget(&self, memory_id: &str) -> Result<Value, String> {
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/forget",
            Some(json!({ "memoryId": memory_id })),
        )
    }

    pub fn list_memories(&self, latest: bool, limit: Option<usize>) -> Result<Value, String> {
        let mut path = format!("/agentmemory/memories?latest={}", latest);
        if let Some(limit) = limit {
            path.push_str(&format!("&limit={}", limit));
        }
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn get_memory(&self, memory_id: &str) -> Result<Value, String> {
        self.send_json(
            reqwest::Method::GET,
            &format!("/agentmemory/memories/{}", memory_id),
            None,
        )
    }

    pub fn search(
        &self,
        query: &str,
        project: Option<&str>,
        cwd: Option<&str>,
        limit: Option<usize>,
        token_budget: Option<usize>,
        format: &str,
    ) -> Result<Value, String> {
        let mut body = json!({
            "query": query,
            "format": format,
        });
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            body["project"] = json!(project);
        }
        if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
            body["cwd"] = json!(cwd);
        }
        if let Some(limit) = limit {
            body["limit"] = json!(limit);
        }
        if let Some(token_budget) = token_budget {
            body["token_budget"] = json!(token_budget);
        }
        self.send_json(reqwest::Method::POST, "/agentmemory/search", Some(body))
    }

    pub fn smart_search(
        &self,
        query: &str,
        project: Option<&str>,
        cwd: Option<&str>,
        limit: Option<usize>,
        expand_ids: Option<&[String]>,
    ) -> Result<Value, String> {
        let mut body = json!({ "query": query });
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            body["project"] = json!(project);
        }
        if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
            body["cwd"] = json!(cwd);
        }
        if let Some(limit) = limit {
            body["limit"] = json!(limit);
        }
        if let Some(ids) = expand_ids.filter(|items| !items.is_empty()) {
            body["expandIds"] = json!(ids);
        }
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/smart-search",
            Some(body),
        )
    }

    pub fn fetch_context(
        &self,
        session_id: &str,
        project: &str,
        token_budget: usize,
    ) -> Result<Option<String>, String> {
        let body = self.send_json(
            reqwest::Method::POST,
            "/agentmemory/context",
            Some(json!({
                "sessionId": session_id,
                "project": project,
                "budget": token_budget,
            })),
        )?;
        Ok(body
            .get("context")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string))
    }

    pub fn enrich(
        &self,
        session_id: &str,
        project: Option<&str>,
        files: &[String],
        terms: &[String],
        tool_name: &str,
    ) -> Result<Option<String>, String> {
        if files.is_empty() {
            return Ok(None);
        }
        let mut body = json!({
            "sessionId": session_id,
            "files": files,
            "terms": terms,
            "toolName": tool_name,
        });
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            body["project"] = json!(project);
        }
        let response = self.send_json(reqwest::Method::POST, "/agentmemory/enrich", Some(body))?;
        Ok(response
            .get("context")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string))
    }

    pub fn list_actions(
        &self,
        project: Option<&str>,
        status: Option<&str>,
        parent_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let mut parts = Vec::new();
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("project={}", urlencoding_encode(project)));
        }
        if let Some(status) = status.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("status={}", urlencoding_encode(status)));
        }
        if let Some(parent_id) = parent_id.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("parentId={}", urlencoding_encode(parent_id)));
        }
        let path = if parts.is_empty() {
            "/agentmemory/actions".to_string()
        } else {
            format!("/agentmemory/actions?{}", parts.join("&"))
        };
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn create_action(&self, body: serde_json::Value) -> Result<serde_json::Value, String> {
        self.send_json(reqwest::Method::POST, "/agentmemory/actions", Some(body))
    }

    pub fn update_action(&self, body: serde_json::Value) -> Result<serde_json::Value, String> {
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/actions/update",
            Some(body),
        )
    }

    pub fn get_action(&self, action_id: &str) -> Result<serde_json::Value, String> {
        let path = format!(
            "/agentmemory/actions/get?actionId={}",
            urlencoding_encode(action_id)
        );
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn fetch_frontier(
        &self,
        project: Option<&str>,
        agent_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value, String> {
        let mut parts = Vec::new();
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("project={}", urlencoding_encode(project)));
        }
        if let Some(agent_id) = agent_id.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("agentId={}", urlencoding_encode(agent_id)));
        }
        if let Some(limit) = limit {
            parts.push(format!("limit={limit}"));
        }
        let path = if parts.is_empty() {
            "/agentmemory/frontier".to_string()
        } else {
            format!("/agentmemory/frontier?{}", parts.join("&"))
        };
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn list_sessions(&self) -> Result<Value, String> {
        self.send_json(reqwest::Method::GET, "/agentmemory/sessions", None)
    }

    pub fn fetch_patterns(&self, project: &str) -> Result<Value, String> {
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/patterns",
            Some(json!({ "project": project })),
        )
    }

    pub fn fetch_timeline(
        &self,
        anchor: &str,
        project: Option<&str>,
        before: Option<usize>,
        after: Option<usize>,
    ) -> Result<Value, String> {
        let mut body = json!({ "anchor": anchor });
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            body["project"] = json!(project);
        }
        if let Some(before) = before {
            body["before"] = json!(before);
        }
        if let Some(after) = after {
            body["after"] = json!(after);
        }
        self.send_json(reqwest::Method::POST, "/agentmemory/timeline", Some(body))
    }

    pub fn fetch_profile(&self, project: &str) -> Result<Value, String> {
        let path = format!(
            "/agentmemory/profile?project={}",
            urlencoding_encode(project)
        );
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn fetch_file_context(
        &self,
        files: &[String],
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        let mut body = json!({ "files": files });
        if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
            body["sessionId"] = json!(session_id);
        }
        self.send_json(reqwest::Method::POST, "/agentmemory/file-context", Some(body))
    }

    pub fn fetch_next(
        &self,
        project: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Value, String> {
        let mut parts = Vec::new();
        if let Some(project) = project.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("project={}", urlencoding_encode(project)));
        }
        if let Some(agent_id) = agent_id.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("agentId={}", urlencoding_encode(agent_id)));
        }
        let path = if parts.is_empty() {
            "/agentmemory/next".to_string()
        } else {
            format!("/agentmemory/next?{}", parts.join("&"))
        };
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn run_consolidate_pipeline(
        &self,
        tier: Option<&str>,
        force: Option<bool>,
    ) -> Result<Value, String> {
        let mut body = json!({});
        if let Some(tier) = tier.filter(|value| !value.trim().is_empty()) {
            body["tier"] = json!(tier);
        }
        if let Some(force) = force {
            body["force"] = json!(force);
        }
        self.send_json(
            reqwest::Method::POST,
            "/agentmemory/consolidate-pipeline",
            Some(body),
        )
    }

    pub fn graph_query(&self, body: Value) -> Result<Value, String> {
        self.send_json(reqwest::Method::POST, "/agentmemory/graph/query", Some(body))
    }

    pub fn graph_stats(&self) -> Result<Value, String> {
        self.send_json(reqwest::Method::GET, "/agentmemory/graph/stats", None)
    }

    pub fn evolve_memory(
        &self,
        memory_id: &str,
        new_content: &str,
        new_title: Option<&str>,
    ) -> Result<Value, String> {
        let mut body = json!({
            "memoryId": memory_id,
            "newContent": new_content,
        });
        if let Some(title) = new_title.filter(|value| !value.trim().is_empty()) {
            body["newTitle"] = json!(title);
        }
        self.send_json(reqwest::Method::POST, "/agentmemory/evolve", Some(body))
    }

    pub fn list_commits(
        &self,
        branch: Option<&str>,
        repo: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value, String> {
        let mut parts = Vec::new();
        if let Some(branch) = branch.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("branch={}", urlencoding_encode(branch)));
        }
        if let Some(repo) = repo.filter(|value| !value.trim().is_empty()) {
            parts.push(format!("repo={}", urlencoding_encode(repo)));
        }
        if let Some(limit) = limit {
            parts.push(format!("limit={limit}"));
        }
        let path = if parts.is_empty() {
            "/agentmemory/commits".to_string()
        } else {
            format!("/agentmemory/commits?{}", parts.join("&"))
        };
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn session_by_commit(&self, sha: &str) -> Result<Value, String> {
        let path = format!(
            "/agentmemory/session/by-commit?sha={}",
            urlencoding_encode(sha)
        );
        self.send_json(reqwest::Method::GET, &path, None)
    }

    pub fn list_observations(&self, session_id: &str) -> Result<Value, String> {
        let path = format!(
            "/agentmemory/observations?sessionId={}",
            urlencoding_encode(session_id)
        );
        self.send_json(reqwest::Method::GET, &path, None)
    }
}

fn urlencoding_encode(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => ch.to_string(),
            _ => format!("%{:02X}", ch as u32),
        })
        .collect()
}
