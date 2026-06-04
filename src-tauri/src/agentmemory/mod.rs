pub mod actions;
pub mod advanced;
pub mod client;
pub mod llm_env;
pub mod mapping;
pub mod resolve;
pub mod service;

pub use actions::{
    AgentMemoryAction, CreateAgentMemoryActionRequest, UpdateAgentMemoryActionRequest,
};

pub use client::{AgentMemoryClient, AgentMemoryHealthStatus};
pub use service::AgentMemoryService;

use std::sync::Arc;

use crate::memory::models::{
    MemoryCategory, MemoryEntry, MemoryEntryPatch, MemoryListFilter, MemoryRetrieveHit,
    MemoryRetrieveOptions, MemoryScope,
};

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::session::models::MessageRole;
use crate::session::store::SessionStore;

use mapping::{
    build_concepts, category_to_agent_type, entry_belongs_to_workspace, entry_from_remember_response,
    entry_matches_filter, enrich_targets_for_tool,
    extract_smart_search_result_content, normalize_project_path, action_project_matches_workspace,
    remote_memory_to_entry, search_result_category,
};

pub struct AgentMemoryState {
    pub client: AgentMemoryClient,
    pub service: AgentMemoryService,
    started_sessions: Mutex<HashSet<String>>,
}

impl AgentMemoryState {
    pub fn new() -> Self {
        Self {
            client: AgentMemoryClient::from_env(),
            service: AgentMemoryService::from_env(),
            started_sessions: Mutex::new(HashSet::new()),
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

    pub fn current_llm_env(&self) -> llm_env::AgentMemoryLlmEnv {
        llm_env::resolve_for_agentmemory()
    }

    /// Restart the sidecar so new LLM env vars take effect (reclaims port if needed).
    pub fn restart_if_running(&self) -> Result<(), String> {
        self.stop()?;
        self.start_and_wait()
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
        if entry.content.trim().is_empty() {
            return Err("Memory content cannot be empty".to_string());
        }
        let body = self.client.remember(
            &entry.content,
            category_to_agent_type(entry.category),
            &concepts,
            project_arg,
            agent_id,
        )?;
        let scope = entry.scope;
        entry_from_remember_response(entry, &body, scope, &project)
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
        let query = options.query.trim();
        let limit = options.limit.unwrap_or(crate::memory::models::DEFAULT_RETRIEVE_LIMIT);
        let scopes = options
            .scopes
            .as_deref()
            .filter(|scopes| !scopes.is_empty())
            .unwrap_or(&[MemoryScope::Project, MemoryScope::User]);

        // agentmemory /search requires a non-empty query; browse recent entries instead.
        if query.is_empty() {
            let entries = self.list_all_for_retrieval(working_dir, None, scopes)?;
            return Ok(entries
                .into_iter()
                .take(limit)
                .map(|entry| MemoryRetrieveHit {
                    entry,
                    score: 1.0,
                    keyword_score: 0.0,
                    semantic_score: 0.0,
                })
                .collect());
        }

        let project = normalize_project_path(working_dir);
        let _token_budget = options
            .token_budget
            .unwrap_or(crate::memory::models::DEFAULT_TOKEN_BUDGET);
        let cwd = normalize_project_path(working_dir);
        let cwd_ref = if cwd.is_empty() { working_dir } else { cwd.as_str() };
        let body = self.client.smart_search(
            query,
            Some(project.as_str()),
            Some(cwd_ref),
            Some(limit),
            None,
        )?;
        let results = body
            .get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut hits = Vec::new();
        for result in results {
            let Some(content) = extract_smart_search_result_content(&result) else {
                continue;
            };
            let score = result
                .get("score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let obs_id = result
                .get("obsId")
                .or_else(|| result.get("id"))
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
                    category: search_result_category(&result),
                    scope: MemoryScope::Project,
                    content,
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
        if let Some(lessons) = body.get("lessons").and_then(|v| v.as_array()) {
            for lesson in lessons {
                let content = lesson
                    .get("content")
                    .or_else(|| lesson.get("lesson"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if content.is_empty() || !mapping::should_include_memory_content(content) {
                    continue;
                }
                let score = lesson
                    .get("confidence")
                    .or_else(|| lesson.get("score"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.75) as f32;
                hits.push(MemoryRetrieveHit {
                    entry: MemoryEntry {
                        id: lesson
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("lesson")
                            .to_string(),
                        category: MemoryCategory::Reference,
                        scope: MemoryScope::Project,
                        content: content.to_string(),
                        tags: vec!["agentmemory:lesson".to_string()],
                        pinned: false,
                        pin_weight: 100,
                        access_count: 0,
                        last_accessed_at: 0,
                        created_at: 0,
                        updated_at: 0,
                        source_session_id: None,
                        linked_doc_path: None,
                    },
                    score,
                    keyword_score: score * 0.3,
                    semantic_score: score * 0.7,
                });
            }
        }
        Ok(hits.into_iter().take(limit).collect())
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

        let session_context = self
            .ensure_session_started(session_id, working_dir, Some(query))
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

    pub fn observe_pre_tool_use(
        &self,
        session_id: &str,
        working_dir: &str,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) {
        if !mapping::should_observe_pre_tool_use(tool_name, tool_input) {
            return;
        }
        let project = normalize_project_path(working_dir);
        if project.is_empty() {
            return;
        }
        let data = serde_json::json!({
            "tool_name": tool_name.trim(),
            "tool_input": tool_input,
        });
        let cwd = normalize_project_path(working_dir);
        let cwd_ref = if cwd.is_empty() { working_dir } else { cwd.as_str() };
        if let Err(error) = self.client.observe("pre_tool_use", session_id, &project, cwd_ref, data)
        {
            eprintln!(
                "[agentmemory] observe pre_tool_use failed session={session_id}: {error}"
            );
        }
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
        // agentmemory only extracts tool_name/tool_input/tool_output when hookType is
        // post_tool_use | post_tool_failure (see @agentmemory/agentmemory mem::observe).
        if is_error && !mapping::should_observe_tool_failure(tool_output) {
            return;
        }
        let (hook, data) = mapping::observe_tool_payload(tool_name, tool_input, tool_output, is_error);
        let cwd = normalize_project_path(working_dir);
        let cwd_ref = if cwd.is_empty() { working_dir } else { cwd.as_str() };
        if let Err(error) = self.client.observe(hook, session_id, &project, cwd_ref, data) {
            eprintln!(
                "[agentmemory] observe {hook} failed session={session_id} tool={tool_name}: {error}"
            );
        }
    }

    /// Backfill agentmemory timeline from persisted Locus tool results (parent + subagent sessions).
    pub fn replay_tool_observations_from_session_tree(
        &self,
        store: &SessionStore,
        root_session_id: &str,
        working_dir: &str,
    ) -> Result<usize, String> {
        if working_dir.trim().is_empty() {
            return Ok(0);
        }
        let _ = self.ensure_ready();
        let session_ids = store.list_session_tree_ids(root_session_id)?;
        let mut replayed = 0usize;
        for source_session_id in session_ids {
            let messages = store.get_messages(&source_session_id)?;
            let mut tool_calls: HashMap<String, (String, serde_json::Value)> = HashMap::new();
            for message in &messages {
                if message.role != MessageRole::Assistant {
                    continue;
                }
                let Some(calls) = message.tool_calls.as_ref() else {
                    continue;
                };
                for call in calls {
                    let args = serde_json::from_str::<serde_json::Value>(&call.arguments)
                        .unwrap_or_else(|_| serde_json::json!({ "raw": call.arguments }));
                    tool_calls.insert(call.id.clone(), (call.name.clone(), args));
                }
            }
            for message in messages {
                if message.role != MessageRole::Tool {
                    continue;
                }
                let Some(tool_call_id) = message.tool_call_id.as_ref() else {
                    continue;
                };
                let Some((tool_name, tool_input)) = tool_calls.get(tool_call_id) else {
                    continue;
                };
                let output = message.content.trim();
                if output.is_empty()
                    || output == crate::session::history::INTERRUPTED_TOOL_RESULT
                {
                    continue;
                }
                let is_error = output.starts_with("Error:")
                    || output.starts_with("error:")
                    || output.contains("\"isError\":true")
                    || output.contains("\"is_error\":true");
                self.observe_tool_use(
                    root_session_id,
                    working_dir,
                    tool_name,
                    tool_input,
                    output,
                    is_error,
                );
                replayed += 1;
            }
        }
        if replayed > 0 {
            eprintln!(
                "[agentmemory] replayed {replayed} tool observations into session {root_session_id}"
            );
        }
        Ok(replayed)
    }

    pub fn ensure_session_started(
        &self,
        session_id: &str,
        working_dir: &str,
        title: Option<&str>,
    ) -> Result<Option<String>, String> {
        {
            let guard = self
                .started_sessions
                .lock()
                .map_err(|error| error.to_string())?;
            if guard.contains(session_id) {
                return Ok(None);
            }
        }
        let context = self.session_start(session_id, working_dir, title)?;
        if let Ok(mut guard) = self.started_sessions.lock() {
            guard.insert(session_id.to_string());
        }
        Ok(context)
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
        let cwd = normalize_project_path(working_dir);
        let cwd_ref = if cwd.is_empty() { working_dir } else { cwd.as_str() };
        let _ = self.client.observe(
            "prompt_submit",
            session_id,
            &project,
            cwd_ref,
            data,
        );
    }

    pub fn fetch_enrich_context(
        &self,
        session_id: &str,
        working_dir: &str,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> Option<String> {
        let (files, terms) = enrich_targets_for_tool(tool_name, tool_input);
        if files.is_empty() {
            return None;
        }
        let _ = self.ensure_ready();
        let project = normalize_project_path(working_dir);
        let project_ref = if project.is_empty() {
            None
        } else {
            Some(project.as_str())
        };
        self.client
            .enrich(session_id, project_ref, &files, &terms, tool_name)
            .ok()
            .flatten()
    }

    pub fn fetch_compact_context(
        &self,
        session_id: &str,
        working_dir: &str,
        token_budget: usize,
    ) -> Option<String> {
        let _ = self.ensure_ready();
        let project = normalize_project_path(working_dir);
        if project.is_empty() {
            return None;
        }
        self.client
            .fetch_context(session_id, &project, token_budget)
            .ok()
            .flatten()
    }

    pub fn recall_search(
        &self,
        working_dir: &str,
        query: &str,
        limit: Option<usize>,
        format: &str,
    ) -> Result<serde_json::Value, String> {
        self.ensure_ready()?;
        let query = query.trim();
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        let project = normalize_project_path(working_dir);
        let cwd = normalize_project_path(working_dir);
        let cwd_ref = if cwd.is_empty() { working_dir } else { cwd.as_str() };
        self.client.search(
            query,
            Some(project.as_str()),
            Some(cwd_ref),
            limit,
            None,
            format,
        )
    }

    pub fn smart_search_raw(
        &self,
        working_dir: &str,
        query: &str,
        limit: Option<usize>,
        expand_ids: Option<Vec<String>>,
    ) -> Result<serde_json::Value, String> {
        self.ensure_ready()?;
        let query = query.trim();
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        let project = normalize_project_path(working_dir);
        let cwd = normalize_project_path(working_dir);
        let cwd_ref = if cwd.is_empty() { working_dir } else { cwd.as_str() };
        self.client.smart_search(
            query,
            Some(project.as_str()),
            Some(cwd_ref),
            limit,
            expand_ids.as_deref(),
        )
    }

    pub fn save_memory(
        &self,
        working_dir: &str,
        content: &str,
        mem_type: Option<&str>,
        concepts: &[String],
    ) -> Result<serde_json::Value, String> {
        self.ensure_ready()?;
        let content = content.trim();
        if content.is_empty() {
            return Err("content is required".to_string());
        }
        let project = normalize_project_path(working_dir);
        let mem_type = mem_type.unwrap_or("fact");
        self.client.remember(
            content,
            mem_type,
            concepts,
            Some(project.as_str()),
            Some("locus"),
        )
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
        let cwd = normalize_project_path(working_dir);
        let cwd_ref = if cwd.is_empty() { working_dir } else { cwd.as_str() };
        let body = self
            .client
            .session_start(session_id, &project, cwd_ref, title)?;
        Ok(body
            .get("context")
            .and_then(|v| v.as_str())
            .map(str::to_string))
    }

    pub fn session_end(&self, session_id: &str, working_dir: Option<&str>) -> Result<(), String> {
        if !self.client.health().available {
            if let Ok(mut guard) = self.started_sessions.lock() {
                guard.remove(session_id);
            }
            return Ok(());
        }
        let _ = self.client.session_end(session_id);
        let summarize_body = self.client.summarize_session(session_id).ok();
        if let (Some(working_dir), Some(body)) = (
            working_dir.map(str::trim).filter(|value| !value.is_empty()),
            summarize_body.as_ref(),
        ) {
            self.create_actions_from_session_summary(body, session_id, working_dir);
        }
        if session_end_auto_consolidate_enabled() {
            let _ = self
                .client
                .run_consolidate_pipeline(Some("all"), Some(false));
        }
        if let Ok(mut guard) = self.started_sessions.lock() {
            guard.remove(session_id);
        }
        Ok(())
    }

    fn create_actions_from_session_summary(
        &self,
        summarize_body: &serde_json::Value,
        session_id: &str,
        working_dir: &str,
    ) {
        let session_tag = format!("locus:session-id:{session_id}");
        if let Ok(existing) = self.list_actions(working_dir, None) {
            let already_created = existing
                .iter()
                .any(|action| action.tags.iter().any(|tag| tag == &session_tag));
            if already_created {
                return;
            }
        }

        let Some(batch) =
            actions::summary_action_batch_from_response(summarize_body, session_id, working_dir)
        else {
            return;
        };
        match self.create_action(batch.parent) {
            Ok(parent) => {
                for mut child in batch.children {
                    child.parent_id = Some(parent.id.clone());
                    if let Err(err) = self.create_action(child) {
                        eprintln!(
                            "[agentmemory] session summary decision action create failed for {session_id}: {err}"
                        );
                    }
                }
            }
            Err(err) => {
                eprintln!(
                    "[agentmemory] session summary action create failed for {session_id}: {err}"
                );
            }
        }
    }

    pub fn list_actions(
        &self,
        working_dir: &str,
        status: Option<&str>,
    ) -> Result<Vec<actions::AgentMemoryAction>, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        let body = self.client.list_actions(None, status, None)?;
        let mut actions = actions::parse_action_list(&body);
        if !project.is_empty() {
            actions.retain(|action| {
                action_project_matches_workspace(action.project.as_deref(), &project)
            });
        }
        Ok(actions)
    }

    pub fn create_action(
        &self,
        request: CreateAgentMemoryActionRequest,
    ) -> Result<actions::AgentMemoryAction, String> {
        self.ensure_ready()?;
        let mut body = serde_json::json!({
            "title": request.title,
            "tags": request.tags,
        });
        if let Some(description) = request
            .description
            .filter(|value| !value.trim().is_empty())
        {
            body["description"] = serde_json::json!(description);
        }
        if let Some(priority) = request.priority {
            body["priority"] = serde_json::json!(priority);
        }
        if let Some(project) = request.project.filter(|value| !value.trim().is_empty()) {
            body["project"] = serde_json::json!(project);
        }
        if let Some(created_by) = request.created_by.filter(|value| !value.trim().is_empty()) {
            body["createdBy"] = serde_json::json!(created_by);
        }
        if let Some(parent_id) = request.parent_id.filter(|value| !value.trim().is_empty()) {
            body["parentId"] = serde_json::json!(parent_id);
        }
        if !request.requires.is_empty() {
            let edges: Vec<serde_json::Value> = request
                .requires
                .iter()
                .map(|target_action_id| {
                    serde_json::json!({
                        "type": "requires",
                        "targetActionId": target_action_id,
                    })
                })
                .collect();
            body["edges"] = serde_json::json!(edges);
        }
        let response = self.client.create_action(body)?;
        actions::parse_action(&response)
            .or_else(|| {
                response
                    .get("action")
                    .and_then(|value| actions::parse_action(value))
            })
            .ok_or_else(|| "agentmemory create action returned no action payload".to_string())
    }

    pub fn update_action(
        &self,
        request: UpdateAgentMemoryActionRequest,
    ) -> Result<actions::AgentMemoryAction, String> {
        self.ensure_ready()?;
        let mut body = serde_json::json!({ "actionId": request.action_id });
        if let Some(status) = request.status.filter(|value| !value.trim().is_empty()) {
            body["status"] = serde_json::json!(status);
        }
        if let Some(title) = request.title.filter(|value| !value.trim().is_empty()) {
            body["title"] = serde_json::json!(title);
        }
        if let Some(description) = request
            .description
            .filter(|value| !value.trim().is_empty())
        {
            body["description"] = serde_json::json!(description);
        }
        if let Some(priority) = request.priority {
            body["priority"] = serde_json::json!(priority);
        }
        if let Some(result) = request.result.filter(|value| !value.trim().is_empty()) {
            body["result"] = serde_json::json!(result);
        }
        let response = self.client.update_action(body)?;
        actions::parse_action(&response)
            .or_else(|| {
                response
                    .get("action")
                    .and_then(|value| actions::parse_action(value))
            })
            .ok_or_else(|| "agentmemory update action returned no action payload".to_string())
    }

    pub fn fetch_frontier(
        &self,
        working_dir: &str,
        limit: Option<usize>,
    ) -> Result<serde_json::Value, String> {
        self.ensure_ready()?;
        let project = normalize_project_path(working_dir);
        let project_ref = if project.is_empty() {
            None
        } else {
            Some(project.as_str())
        };
        self.client.fetch_frontier(project_ref, Some("locus"), limit)
    }

    pub fn create_action_from_proposal_item(
        &self,
        item: &crate::session::models::MemoryProposalItem,
        working_dir: &str,
        session_id: &str,
    ) -> Result<actions::AgentMemoryAction, String> {
        let request = actions::create_request_from_proposal_item(item, working_dir, session_id);
        self.create_action(request)
    }
}

pub type SharedAgentMemoryState = Arc<AgentMemoryState>;

/// Session-end consolidation is opt-out via `LOCUS_AGENTMEMORY_SESSION_END_CONSOLIDATE=0|false`.
fn session_end_auto_consolidate_enabled() -> bool {
    match std::env::var("LOCUS_AGENTMEMORY_SESSION_END_CONSOLIDATE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
    {
        Some(value) if matches!(value.as_str(), "0" | "false" | "no" | "off") => false,
        _ => true,
    }
}

#[cfg(test)]
mod session_end_tests {
    use super::session_end_auto_consolidate_enabled;

    #[test]
    fn session_end_consolidate_respects_opt_out_env() {
        std::env::set_var("LOCUS_AGENTMEMORY_SESSION_END_CONSOLIDATE", "false");
        assert!(!session_end_auto_consolidate_enabled());
        std::env::remove_var("LOCUS_AGENTMEMORY_SESSION_END_CONSOLIDATE");
    }
}

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
