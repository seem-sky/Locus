pub mod client;
pub mod mapping;
pub mod resolve;
pub mod service;

pub use client::{AgentMemoryClient, AgentMemoryHealthStatus};
pub use service::AgentMemoryService;

use std::sync::Arc;

use crate::memory::models::{
    MemoryEntry, MemoryEntryPatch, MemoryListFilter, MemoryRetrieveHit, MemoryRetrieveOptions,
    MemoryScope,
};

use mapping::{
    build_concepts, category_to_agent_type, entry_belongs_to_workspace, entry_matches_filter,
    normalize_project_path, remote_memory_to_entry,
};

pub struct AgentMemoryState {
    pub client: AgentMemoryClient,
    pub service: AgentMemoryService,
}

impl AgentMemoryState {
    pub fn new() -> Self {
        Self {
            client: AgentMemoryClient::from_env(),
            service: AgentMemoryService::from_env(),
        }
    }

    pub fn ensure_ready(&self) -> Result<(), String> {
        self.service.ensure_running(&self.client)
    }

    pub fn health(&self) -> AgentMemoryHealthStatus {
        self.service.health(&self.client)
    }

    pub fn start(&self) -> Result<(), String> {
        self.service.start(&self.client)
    }

    pub fn start_and_wait(&self) -> Result<(), String> {
        self.service.start_and_wait(&self.client)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.service.stop()
    }

    pub fn bundle_version(&self) -> Option<String> {
        self.service.bundle_version()
    }

    pub fn using_bundled_runtime(&self) -> bool {
        self.service.using_bundled_runtime()
    }

    pub fn set_export_root(&self, path: std::path::PathBuf) {
        self.service.set_export_root(path);
    }

    fn load_entries_for_workspace(
        &self,
        working_dir: &str,
        filter: &MemoryListFilter,
    ) -> Result<Vec<MemoryEntry>, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        let body = self.client.list_memories(true, filter.limit.or(Some(500)))?;
        let memories = body
            .get("memories")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut entries = Vec::new();
        for memory in memories {
            if memory
                .get("isLatest")
                .and_then(|v| v.as_bool())
                == Some(false)
            {
                continue;
            }
            if let Some(entry) =
                remote_memory_to_entry(&memory, MemoryScope::Project, &project)
            {
                let memory_project = memory
                    .get("project")
                    .and_then(|value| value.as_str());
                if entry_belongs_to_workspace(&entry, memory_project, &project)
                    && entry_matches_filter(&entry, filter)
                {
                    entries.push(entry);
                }
            }
        }
        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let offset = filter.offset.unwrap_or(0);
        let limit = filter.limit.unwrap_or(200);
        Ok(entries.into_iter().skip(offset).take(limit).collect())
    }

    pub fn list(
        &self,
        working_dir: &str,
        _app_storage_dir: Option<&std::path::Path>,
        filter: &MemoryListFilter,
    ) -> Result<Vec<MemoryEntry>, String> {
        self.load_entries_for_workspace(working_dir, filter)
    }

    pub fn get(
        &self,
        working_dir: &str,
        _app_storage_dir: Option<&std::path::Path>,
        scope: MemoryScope,
        id: &str,
    ) -> Result<Option<MemoryEntry>, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        let body = self.client.get_memory(id)?;
        let memory = body.get("memory").unwrap_or(&body);
        Ok(remote_memory_to_entry(memory, scope, &project).filter(|entry| entry.scope == scope))
    }

    pub fn find_by_id_any_scope(
        &self,
        working_dir: &str,
        app_storage_dir: Option<&std::path::Path>,
        id: &str,
    ) -> Result<Option<MemoryEntry>, String> {
        if let Some(entry) = self.get(working_dir, app_storage_dir, MemoryScope::Project, id)? {
            return Ok(Some(entry));
        }
        self.get(working_dir, app_storage_dir, MemoryScope::User, id)
    }

    pub fn create(
        &self,
        working_dir: &str,
        _app_storage_dir: Option<&std::path::Path>,
        entry: MemoryEntry,
        _embedding: Option<Vec<f32>>,
    ) -> Result<MemoryEntry, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        let concepts = build_concepts(entry.category, entry.scope, entry.pinned, &entry.tags);
        let (project_arg, agent_id) = match entry.scope {
            MemoryScope::Project => (Some(project.as_str()), Some("locus")),
            MemoryScope::User => (None, Some("locus-user")),
        };
        let body = self.client.remember(
            &entry.content,
            category_to_agent_type(entry.category),
            &concepts,
            project_arg,
            agent_id,
        )?;
        let memory = body.get("memory").unwrap_or(&body);
        remote_memory_to_entry(memory, entry.scope, &project).ok_or_else(|| {
            "agentmemory remember succeeded but response could not be mapped".to_string()
        })
    }

    pub fn update(
        &self,
        working_dir: &str,
        app_storage_dir: Option<&std::path::Path>,
        scope: MemoryScope,
        id: &str,
        patch: &MemoryEntryPatch,
        _embedding: Option<Vec<f32>>,
    ) -> Result<MemoryEntry, String> {
        let existing = self
            .get(working_dir, app_storage_dir, scope, id)?
            .ok_or_else(|| format!("Memory entry not found: {}", id))?;
        let mut next = existing;
        if let Some(category) = patch.category {
            next.category = category;
        }
        if let Some(content) = &patch.content {
            next.content = content.clone();
        }
        if let Some(tags) = &patch.tags {
            next.tags = tags.clone();
        }
        if let Some(pinned) = patch.pinned {
            next.pinned = pinned;
        }
        if let Some(pin_weight) = patch.pin_weight {
            next.pin_weight = pin_weight;
        }
        next.updated_at = crate::memory::store::current_unix_millis();
        let _ = self.client.forget(id);
        self.create(working_dir, app_storage_dir, next, None)
    }

    pub fn delete(
        &self,
        _working_dir: &str,
        _app_storage_dir: Option<&std::path::Path>,
        _scope: MemoryScope,
        id: &str,
    ) -> Result<(), String> {
        self.ensure_ready()?;
        self.client.forget(id)?;
        Ok(())
    }

    pub fn pin(
        &self,
        working_dir: &str,
        app_storage_dir: Option<&std::path::Path>,
        scope: MemoryScope,
        id: &str,
        pinned: bool,
        pin_weight: Option<i32>,
    ) -> Result<MemoryEntry, String> {
        self.update(
            working_dir,
            app_storage_dir,
            scope,
            id,
            &MemoryEntryPatch {
                category: None,
                content: None,
                tags: None,
                pinned: Some(pinned),
                pin_weight,
            },
            None,
        )
    }

    pub fn update_tags(
        &self,
        working_dir: &str,
        app_storage_dir: Option<&std::path::Path>,
        scope: MemoryScope,
        id: &str,
        tags: Vec<String>,
    ) -> Result<MemoryEntry, String> {
        self.update(
            working_dir,
            app_storage_dir,
            scope,
            id,
            &MemoryEntryPatch {
                category: None,
                content: None,
                tags: Some(tags),
                pinned: None,
                pin_weight: None,
            },
            None,
        )
    }

    pub fn list_all_for_retrieval(
        &self,
        working_dir: &str,
        app_storage_dir: Option<&std::path::Path>,
        scopes: &[MemoryScope],
    ) -> Result<Vec<MemoryEntry>, String> {
        let filter = MemoryListFilter {
            category: None,
            scope: None,
            tags: None,
            query: None,
            limit: Some(500),
            offset: None,
        };
        let entries = self.list(working_dir, app_storage_dir, &filter)?;
        Ok(entries
            .into_iter()
            .filter(|entry| scopes.contains(&entry.scope))
            .collect())
    }

    pub fn retrieve(
        &self,
        working_dir: &str,
        options: &MemoryRetrieveOptions,
    ) -> Result<Vec<MemoryRetrieveHit>, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        let limit = options.limit.unwrap_or(crate::memory::models::DEFAULT_RETRIEVE_LIMIT);
        let token_budget = options
            .token_budget
            .unwrap_or(crate::memory::models::DEFAULT_TOKEN_BUDGET);
        let body = self.client.search(
            &options.query,
            Some(project.as_str()),
            Some(working_dir),
            Some(limit),
            Some(token_budget),
            "narrative",
        )?;
        if let Some(text) = body.get("text").and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(vec![MemoryRetrieveHit {
                    entry: MemoryEntry {
                        id: "agentmemory-context".to_string(),
                        category: crate::memory::models::MemoryCategory::Topic,
                        scope: MemoryScope::Project,
                        content: trimmed.to_string(),
                        tags: Vec::new(),
                        pinned: false,
                        pin_weight: 100,
                        access_count: 0,
                        last_accessed_at: 0,
                        created_at: 0,
                        updated_at: 0,
                        source_session_id: None,
                        linked_doc_path: None,
                    },
                    score: 1.0,
                    keyword_score: 0.5,
                    semantic_score: 0.5,
                }]);
            }
        }
        let results = body
            .get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut hits = Vec::new();
        for result in results {
            let narrative = result
                .get("narrative")
                .or_else(|| result.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if narrative.is_empty() {
                continue;
            }
            let score = result
                .get("score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let obs_type = result
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("fact");
            let obs_id = result
                .get("obsId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            hits.push(MemoryRetrieveHit {
                entry: MemoryEntry {
                    id: if obs_id.is_empty() {
                        format!("obs_{}", hits.len())
                    } else {
                        obs_id
                    },
                    category: mapping::agent_type_to_category(obs_type),
                    scope: MemoryScope::Project,
                    content: narrative,
                    tags: Vec::new(),
                    pinned: false,
                    pin_weight: 100,
                    access_count: 0,
                    last_accessed_at: 0,
                    created_at: 0,
                    updated_at: 0,
                    source_session_id: result
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    linked_doc_path: None,
                },
                score,
                keyword_score: score * 0.4,
                semantic_score: score * 0.6,
            });
        }
        Ok(hits)
    }

    /// Build the memory prefix injected into chat prompts. Must run on a blocking thread.
    pub fn build_chat_memory_prefix(
        &self,
        session_id: &str,
        working_dir: &str,
        query: &str,
    ) -> Option<String> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }

        let _ = self.ensure_ready();
        self.observe_user_prompt(session_id, working_dir, query);

        let session_context = self
            .session_start(session_id, working_dir, Some(query))
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty());

        let hits = crate::memory::retrieve_entries(
            self,
            working_dir,
            None,
            &crate::memory::MemoryRetrieveOptions {
                query: query.to_string(),
                limit: None,
                token_budget: Some(crate::memory::models::DEFAULT_TOKEN_BUDGET),
                scopes: None,
            },
            None,
            &[],
        )
        .ok()?;

        let search_prefix = if hits.is_empty() {
            None
        } else {
            let prefix = crate::memory::build_relevant_memory_prefix(&hits);
            if prefix.trim().is_empty() {
                None
            } else {
                Some(prefix)
            }
        };

        merge_memory_prompt_blocks(session_context.as_deref(), search_prefix.as_deref())
    }

    pub fn record_access(
        &self,
        _working_dir: &str,
        _app_storage_dir: Option<&std::path::Path>,
        _scope: MemoryScope,
        _ids: &[String],
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn get_embedding(
        &self,
        _working_dir: &str,
        _app_storage_dir: Option<&std::path::Path>,
        _scope: MemoryScope,
        _id: &str,
    ) -> Result<Option<Vec<f32>>, String> {
        Ok(None)
    }

    pub fn set_embedding(
        &self,
        _working_dir: &str,
        _app_storage_dir: Option<&std::path::Path>,
        _scope: MemoryScope,
        _id: &str,
        _embedding: &[f32],
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn observe_tool_use(
        &self,
        session_id: &str,
        working_dir: &str,
        tool_name: &str,
        tool_input: &serde_json::Value,
        tool_output: &str,
        is_error: bool,
    ) {
        let project = normalize_project_path(working_dir);
        if project.is_empty() {
            return;
        }
        let hook = if is_error {
            "PostToolUseFailure"
        } else {
            "PostToolUse"
        };
        let data = serde_json::json!({
            "toolName": tool_name,
            "toolInput": tool_input,
            "toolOutput": tool_output,
            "isError": is_error,
        });
        let _ = self.client.observe(hook, session_id, &project, working_dir, data);
    }

    pub fn observe_user_prompt(
        &self,
        session_id: &str,
        working_dir: &str,
        prompt: &str,
    ) {
        let project = normalize_project_path(working_dir);
        if project.is_empty() || prompt.trim().is_empty() {
            return;
        }
        let data = serde_json::json!({ "prompt": prompt });
        let _ = self.client.observe(
            "UserPromptSubmit",
            session_id,
            &project,
            working_dir,
            data,
        );
    }

    pub fn session_start(
        &self,
        session_id: &str,
        working_dir: &str,
        title: Option<&str>,
    ) -> Result<Option<String>, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        if project.is_empty() {
            return Ok(None);
        }
        let body = self
            .client
            .session_start(session_id, &project, working_dir, title)?;
        Ok(body
            .get("context")
            .and_then(|v| v.as_str())
            .map(str::to_string))
    }

    pub fn session_end(&self, session_id: &str) -> Result<(), String> {
        if !self.client.health().available {
            return Ok(());
        }
        let _ = self.client.session_end(session_id);
        let _ = self.client.summarize_session(session_id);
        Ok(())
    }
}

pub type SharedAgentMemoryState = Arc<AgentMemoryState>;

fn merge_memory_prompt_blocks(first: Option<&str>, second: Option<&str>) -> Option<String> {
    let first = first.map(str::trim).filter(|value| !value.is_empty());
    let second = second.map(str::trim).filter(|value| !value.is_empty());
    match (first, second) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value.to_string()),
        (Some(left), Some(right)) if left == right || left.contains(right) => {
            Some(left.to_string())
        }
        (Some(left), Some(right)) if right.contains(left) => Some(right.to_string()),
        (Some(left), Some(right)) => Some(format!("{}\n\n{}", left, right)),
    }
}
