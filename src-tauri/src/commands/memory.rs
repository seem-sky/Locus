use std::sync::Arc;



use serde::{Deserialize, Serialize};

use tauri::{AppHandle, State};



use crate::agentmemory::{
    AgentMemoryAction, AgentMemoryState, CreateAgentMemoryActionRequest,
    UpdateAgentMemoryActionRequest,
};

use crate::commands::default_app_storage_dir;

use crate::error::AppError;

use crate::memory::{

    apply_memory_entry, build_memory_entry_from_proposal_item, current_unix_millis,

    default_scope_for_category, new_entry_id, retrieve_entries, MemoryCategory, MemoryEntry,

    MemoryEntryPatch, MemoryListFilter, MemoryRetrieveHit, MemoryRetrieveOptions, MemoryScope,
    DEFAULT_PIN_WEIGHT,

};

use crate::session::models::{KnowledgeProposalStatus, MemoryProposal, MemoryProposalItem};

use crate::session::store::SessionStore;

use crate::workspace::Workspace;



use super::session::emit_memory_proposal_message;



fn app_storage_dir(app_handle: &AppHandle) -> Result<std::path::PathBuf, String> {

    default_app_storage_dir(app_handle)

}



fn map_category(value: &str) -> Result<MemoryCategory, String> {

    MemoryCategory::from_str(value).ok_or_else(|| format!("Unknown memory category: {}", value))

}



fn map_scope(value: &str) -> Result<MemoryScope, String> {

    MemoryScope::from_str(value).ok_or_else(|| format!("Unknown memory scope: {}", value))

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryListRequest {

    pub working_dir: String,

    pub category: Option<String>,

    pub scope: Option<String>,

    pub tags: Option<Vec<String>>,

    pub query: Option<String>,

    pub limit: Option<usize>,

    pub offset: Option<usize>,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryGetRequest {

    pub working_dir: String,

    pub scope: String,

    pub id: String,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryCreateRequest {

    pub working_dir: String,

    pub category: String,

    pub scope: Option<String>,

    pub content: String,

    #[serde(default)]

    pub tags: Vec<String>,

    pub pinned: Option<bool>,

    pub pin_weight: Option<i32>,

    pub source_session_id: Option<String>,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryUpdateRequest {

    pub working_dir: String,

    pub scope: String,

    pub id: String,

    pub category: Option<String>,

    pub content: Option<String>,

    pub tags: Option<Vec<String>>,

    pub pinned: Option<bool>,

    pub pin_weight: Option<i32>,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryDeleteRequest {

    pub working_dir: String,

    pub scope: String,

    pub id: String,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryPinRequest {

    pub working_dir: String,

    pub scope: String,

    pub id: String,

    pub pinned: bool,

    pub pin_weight: Option<i32>,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryTagUpdateRequest {

    pub working_dir: String,

    pub scope: String,

    pub id: String,

    pub tags: Vec<String>,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryRetrieveRequest {

    pub working_dir: String,

    pub query: String,

    pub limit: Option<usize>,

    pub token_budget: Option<usize>,

    pub scopes: Option<Vec<String>>,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct MemoryIgnoreProposalRequest {

    pub session_id: String,

    pub proposal_id: String,

}



#[derive(Debug, Clone, Serialize, Deserialize)]

#[serde(rename_all = "camelCase")]

pub struct AgentMemoryStatusResponse {

    pub available: bool,

    pub status: String,

    pub version: Option<String>,

    pub viewer_port: Option<u16>,

    pub base_url: String,

    pub autostart_enabled: bool,

    pub bundle_version: Option<String>,

    pub using_bundled_runtime: bool,

    pub error: Option<String>,

    pub llm_configured: bool,

    pub llm_provider: Option<String>,

    pub llm_warning: Option<String>,

}



fn build_create_entry(request: MemoryCreateRequest) -> Result<MemoryEntry, String> {

    let category = map_category(&request.category)?;

    let scope = request

        .scope

        .as_deref()

        .map(map_scope)

        .transpose()?

        .unwrap_or_else(|| default_scope_for_category(category));

    let now = current_unix_millis();

    Ok(MemoryEntry {

        id: new_entry_id(),

        category,

        scope,

        content: request.content,

        tags: request.tags,

        pinned: request.pinned.unwrap_or(false),

        pin_weight: request.pin_weight.unwrap_or(DEFAULT_PIN_WEIGHT),

        access_count: 0,

        last_accessed_at: 0,

        created_at: now,

        updated_at: now,

        source_session_id: request.source_session_id,

        linked_doc_path: None,

    })

}



fn status_response(store: &AgentMemoryState) -> AgentMemoryStatusResponse {

    let health = store.health();
    let llm_env = store.current_llm_env();

    AgentMemoryStatusResponse {

        available: health.available,

        status: health.status,

        version: health.version,

        viewer_port: health.viewer_port,

        base_url: store.client.base_url().to_string(),

        autostart_enabled: store.service.autostart_enabled(),

        bundle_version: store.bundle_version(),

        using_bundled_runtime: store.using_bundled_runtime(),

        error: health.error,

        llm_configured: llm_env.configured,

        llm_provider: if llm_env.provider_label.is_empty() || llm_env.provider_label == "none" {
            None
        } else {
            Some(llm_env.provider_label)
        },

        llm_warning: llm_env.warning,

    }

}

pub(crate) fn schedule_agentmemory_restart(store: &std::sync::Arc<AgentMemoryState>) {
    let store = store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = store.restart_if_running() {
            eprintln!(
                "[Locus] agentmemory restart after config change failed: {}",
                error
            );
        }
    });
}



async fn run_memory_blocking<T, F>(join_code: &str, join_message: &str, task: F) -> Result<T, AppError>

where

    T: Send + 'static,

    F: FnOnce() -> Result<T, String> + Send + 'static,

{

    tauri::async_runtime::spawn_blocking(task)

        .await

        .map_err(|e| AppError::new(join_code, format!("{join_message}: {e}")))?

        .map_err(AppError::from)

}



#[tauri::command]

pub async fn agentmemory_status(

    store: State<'_, Arc<AgentMemoryState>>,

) -> Result<AgentMemoryStatusResponse, AppError> {

    let store = store.inner().clone();

    tauri::async_runtime::spawn_blocking(move || status_response(&store))

        .await

        .map_err(|e| {

            AppError::new(

                "agentmemory.status.join_failed",

                format!("Failed to read agentmemory status: {}", e),

            )

        })

}



#[tauri::command]

pub async fn agentmemory_start(

    store: State<'_, Arc<AgentMemoryState>>,

) -> Result<AgentMemoryStatusResponse, AppError> {

    let store = store.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {

        store.start().map_err(AppError::from)?;

        Ok(status_response(&store))

    })

    .await

    .map_err(|e| {

        AppError::new(

            "agentmemory.start.join_failed",

            format!("Failed to start agentmemory: {}", e),

        )

    })?

}



#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryReplayResponse {
    pub replayed: usize,
}

#[tauri::command]
pub async fn agentmemory_replay_session(
    memory_store: State<'_, Arc<AgentMemoryState>>,
    session_store: State<'_, Arc<SessionStore>>,
    workspace: State<'_, Arc<Workspace>>,
    session_id: String,
) -> Result<AgentMemoryReplayResponse, AppError> {
    let memory_store = memory_store.inner().clone();
    let session_store = session_store.inner().clone();
    let working_dir = workspace.path.read().await.clone();
    if working_dir.trim().is_empty() {
        return Err(AppError::new(
            "agentmemory.replay.no_workspace",
            "Select a workspace before replaying observations",
        ));
    }
    let replayed = tauri::async_runtime::spawn_blocking(move || {
        memory_store.replay_tool_observations_from_session_tree(
            &session_store,
            &session_id,
            &working_dir,
        )
    })
    .await
    .map_err(|e| {
        AppError::new(
            "agentmemory.replay.join_failed",
            format!("Failed to replay session observations: {}", e),
        )
    })??;
    Ok(AgentMemoryReplayResponse { replayed })
}

#[tauri::command]

pub async fn agentmemory_stop(

    store: State<'_, Arc<AgentMemoryState>>,

) -> Result<AgentMemoryStatusResponse, AppError> {

    let store = store.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {

        store.stop().map_err(AppError::from)?;

        Ok(status_response(&store))

    })

    .await

    .map_err(|e| {

        AppError::new(

            "agentmemory.stop.join_failed",

            format!("Failed to stop agentmemory: {}", e),

        )

    })?

}



#[tauri::command]

pub async fn memory_list(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryListRequest,

) -> Result<Vec<MemoryEntry>, AppError> {

    let filter = MemoryListFilter {

        category: request

            .category

            .as_deref()

            .map(map_category)

            .transpose()?,

        scope: request.scope.as_deref().map(map_scope).transpose()?,

        tags: request.tags,

        query: request.query,

        limit: request.limit,

        offset: request.offset,

    };

    let store = store.inner().clone();

    let working_dir = request.working_dir;

    run_memory_blocking("memory.list.join_failed", "Failed to list memory entries", move || {

        store.list(&working_dir, None, &filter)

    })

    .await

}



#[tauri::command]

pub async fn memory_get(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryGetRequest,

) -> Result<Option<MemoryEntry>, AppError> {

    let scope = map_scope(&request.scope)?;

    let store = store.inner().clone();

    let working_dir = request.working_dir;

    let id = request.id;

    run_memory_blocking("memory.get.join_failed", "Failed to get memory entry", move || {

        store.get(&working_dir, None, scope, &id)

    })

    .await

}



#[tauri::command]

pub async fn memory_create(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryCreateRequest,

) -> Result<MemoryEntry, AppError> {

    let working_dir = request.working_dir.clone();

    let entry = build_create_entry(request)?;

    let store = store.inner().clone();

    run_memory_blocking("memory.create.join_failed", "Failed to create memory entry", move || {

        apply_memory_entry(store.as_ref(), &working_dir, None, entry, None)

    })

    .await

}



#[tauri::command]

pub async fn memory_update(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryUpdateRequest,

) -> Result<MemoryEntry, AppError> {

    let scope = map_scope(&request.scope)?;

    let patch = MemoryEntryPatch {

        category: request

            .category

            .as_deref()

            .map(map_category)

            .transpose()?,

        content: request.content,

        tags: request.tags,

        pinned: request.pinned,

        pin_weight: request.pin_weight,

    };

    let store = store.inner().clone();

    let working_dir = request.working_dir;

    let id = request.id;

    run_memory_blocking("memory.update.join_failed", "Failed to update memory entry", move || {

        store.update(&working_dir, None, scope, &id, &patch, None)

    })

    .await

}



#[tauri::command]

pub async fn memory_delete(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryDeleteRequest,

) -> Result<(), AppError> {

    let scope = map_scope(&request.scope)?;

    let store = store.inner().clone();

    let working_dir = request.working_dir;

    let id = request.id;

    run_memory_blocking("memory.delete.join_failed", "Failed to delete memory entry", move || {

        store.delete(&working_dir, None, scope, &id)

    })

    .await

}



#[tauri::command]

pub async fn memory_pin(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryPinRequest,

) -> Result<MemoryEntry, AppError> {

    let scope = map_scope(&request.scope)?;

    let store = store.inner().clone();

    let working_dir = request.working_dir;

    let id = request.id;

    let pinned = request.pinned;

    let pin_weight = request.pin_weight;

    run_memory_blocking("memory.pin.join_failed", "Failed to pin memory entry", move || {

        store.pin(&working_dir, None, scope, &id, pinned, pin_weight)

    })

    .await

}



#[tauri::command]

pub async fn memory_tag_update(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryTagUpdateRequest,

) -> Result<MemoryEntry, AppError> {

    let scope = map_scope(&request.scope)?;

    let store = store.inner().clone();

    let working_dir = request.working_dir;

    let id = request.id;

    let tags = request.tags;

    run_memory_blocking(

        "memory.tag_update.join_failed",

        "Failed to update memory entry tags",

        move || store.update_tags(&working_dir, None, scope, &id, tags),

    )

    .await

}



#[tauri::command]

pub async fn memory_retrieve(

    store: State<'_, Arc<AgentMemoryState>>,

    request: MemoryRetrieveRequest,

) -> Result<Vec<MemoryRetrieveHit>, AppError> {

    let scopes = if let Some(raw) = request.scopes {

        raw.iter().map(|s| map_scope(s)).collect::<Result<Vec<_>, _>>()?

    } else {

        vec![MemoryScope::Project, MemoryScope::User]

    };

    let options = MemoryRetrieveOptions {

        query: request.query.clone(),

        limit: request.limit,

        token_budget: request.token_budget,

        scopes: Some(scopes),

    };

    let store = store.inner().clone();

    let working_dir = request.working_dir.clone();

    tauri::async_runtime::spawn_blocking(move || {

        retrieve_entries(store.as_ref(), &working_dir, None, &options, None, &[])

    })

    .await

    .map_err(|e| {

        AppError::new(

            "memory.retrieve.join_failed",

            format!("Failed to retrieve memory: {}", e),

        )

    })?

    .map_err(AppError::from)

}



#[tauri::command]

pub async fn stale_memory_proposals(

    app_handle: AppHandle,

    store: State<'_, Arc<SessionStore>>,

    session_id: String,

) -> Result<(), AppError> {

    let updated = store.stale_pending_memory_proposals(&session_id)?;

    for message in updated {

        emit_memory_proposal_message(&app_handle, store.inner().as_ref(), &session_id, message);

    }

    Ok(())

}



#[tauri::command]

pub async fn ignore_memory_proposal(

    app_handle: AppHandle,

    store: State<'_, Arc<SessionStore>>,

    session_id: String,

    proposal_id: String,

) -> Result<(), AppError> {

    let updated = store.update_memory_proposal_status(

        &session_id,

        &proposal_id,

        KnowledgeProposalStatus::Invalidated,

    )?;

    if let Some(message) = updated {

        emit_memory_proposal_message(&app_handle, store.inner().as_ref(), &session_id, message);

    }

    Ok(())

}



#[tauri::command]

pub async fn apply_memory_proposal(

    app_handle: AppHandle,

    memory_store: State<'_, Arc<AgentMemoryState>>,

    session_store: State<'_, Arc<SessionStore>>,

    workspace: State<'_, Arc<Workspace>>,

    session_id: String,

    proposal_id: String,

) -> Result<(), AppError> {

    let working_dir = workspace.path.read().await.clone();

    if working_dir.trim().is_empty() {

        return Err("Workspace is not selected".into());

    }



    let Some(message) = session_store.get_memory_proposal_message(&session_id, &proposal_id)?

    else {

        return Err(format!("Memory proposal not found: {}", proposal_id).into());

    };

    let Some(proposal) = message.memory_proposal.clone() else {

        return Err(format!(

            "Message does not contain a memory proposal: {}",

            proposal_id

        )

        .into());

    };

    match proposal.status {
        KnowledgeProposalStatus::Applied => return Ok(()),
        KnowledgeProposalStatus::Pending => {}
        KnowledgeProposalStatus::Applying => {
            // Concurrent or repeated apply while the first call is still running.
        }
        other => {
            return Err(format!(
                "Memory proposal '{}' cannot be applied (current status: {:?})",
                proposal_id, other
            )
            .into());
        }
    }

    if proposal.status == KnowledgeProposalStatus::Pending {
        if let Some(applying_message) = session_store.update_memory_proposal_status(
            &session_id,
            &proposal_id,
            KnowledgeProposalStatus::Applying,
        )? {
            emit_memory_proposal_message(
                &app_handle,
                session_store.inner().as_ref(),
                &session_id,
                applying_message,
            );
        }
    }

    let memory_store = memory_store.inner().clone();
    let working_dir = working_dir.clone();
    let session_id_for_apply = session_id.clone();
    let proposal_items = proposal.items.clone();

    let apply_error = run_memory_blocking(
        "memory.apply_proposal.join_failed",
        "Failed to apply memory proposal",
        move || {
            let mut error: Option<String> = None;
            for item in &proposal_items {
                if !crate::agentmemory::mapping::should_include_memory_content(&item.content) {
                    continue;
                }
                let entry =
                    build_memory_entry_from_proposal_item(item, Some(session_id_for_apply.clone()));
                if let Err(err) =
                    apply_memory_entry(memory_store.as_ref(), &working_dir, None, entry, None)
                {
                    error = Some(err);
                    break;
                }
                if let Err(err) = memory_store.create_action_from_proposal_item(
                    item,
                    &working_dir,
                    &session_id_for_apply,
                ) {
                    eprintln!(
                        "[Locus] memory proposal saved but agentmemory action create failed: {}",
                        err
                    );
                }
            }
            Ok(error)
        },
    )
    .await?;

    let next_status = if apply_error.is_some() {
        KnowledgeProposalStatus::Pending
    } else {
        KnowledgeProposalStatus::Applied
    };

    if let Some(message) = session_store.update_memory_proposal_status(
        &session_id,
        &proposal_id,
        next_status,
    )? {
        emit_memory_proposal_message(
            &app_handle,
            session_store.inner().as_ref(),
            &session_id,
            message,
        );
    }

    if let Some(error) = apply_error {
        return Err(error.into());
    }

    Ok(())
}



pub fn build_memory_proposal(items: Vec<MemoryProposalItem>, confidence: f32) -> MemoryProposal {

    let now = current_unix_millis();

    let est_tokens = items

        .iter()

        .map(|item| (item.content.len() / 4).max(1) as u32)

        .sum();

    MemoryProposal {

        proposal_id: format!("mp_{}", uuid::Uuid::new_v4()),

        status: KnowledgeProposalStatus::Pending,

        confidence,

        verify: crate::session::models::KnowledgeProposalVerify::None,

        est_tokens,

        items,

        created_at: now,

        updated_at: now,

    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryActionListRequest {
    pub working_dir: String,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryActionCreateRequest {
    pub working_dir: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub parent_id: Option<String>,
    pub requires: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryActionUpdateRequest {
    pub working_dir: String,
    pub action_id: String,
    pub status: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub result: Option<String>,
}

#[tauri::command]
pub async fn agentmemory_action_list(
    store: State<'_, Arc<AgentMemoryState>>,
    request: AgentMemoryActionListRequest,
) -> Result<Vec<AgentMemoryAction>, AppError> {
    let store = store.inner().clone();
    let working_dir = request.working_dir;
    let status = request.status;
    run_memory_blocking(
        "agentmemory.action_list.join_failed",
        "Failed to list agentmemory actions",
        move || store.list_actions(&working_dir, status.as_deref()),
    )
    .await
}

#[tauri::command]
pub async fn agentmemory_action_create(
    store: State<'_, Arc<AgentMemoryState>>,
    request: AgentMemoryActionCreateRequest,
) -> Result<AgentMemoryAction, AppError> {
    let store = store.inner().clone();
    let working_dir = request.working_dir.clone();
    let project = crate::agentmemory::mapping::normalize_project_path(&working_dir);
    let project = if project.is_empty() {
        None
    } else {
        Some(project)
    };
    let create = CreateAgentMemoryActionRequest {
        title: request.title,
        description: request.description,
        priority: request.priority,
        project,
        created_by: Some("locus".to_string()),
        tags: request.tags.unwrap_or_default(),
        parent_id: request.parent_id,
        requires: request.requires.unwrap_or_default(),
    };
    run_memory_blocking(
        "agentmemory.action_create.join_failed",
        "Failed to create agentmemory action",
        move || store.create_action(create),
    )
    .await
}

#[tauri::command]
pub async fn agentmemory_action_update(
    store: State<'_, Arc<AgentMemoryState>>,
    request: AgentMemoryActionUpdateRequest,
) -> Result<AgentMemoryAction, AppError> {
    let store = store.inner().clone();
    let update = UpdateAgentMemoryActionRequest {
        action_id: request.action_id,
        status: request.status,
        title: request.title,
        description: request.description,
        priority: request.priority,
        result: request.result,
    };
    run_memory_blocking(
        "agentmemory.action_update.join_failed",
        "Failed to update agentmemory action",
        move || store.update_action(update),
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct AgentMemoryInsightsRequest {
    #[serde(rename = "workingDir")]
    pub working_dir: String,
}

#[derive(Debug, Serialize)]
pub struct AgentMemoryInsightsResponse {
    pub sessions: serde_json::Value,
    pub profile: Option<serde_json::Value>,
    pub patterns: Option<serde_json::Value>,
    pub graph_stats: Option<serde_json::Value>,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AgentMemoryConsolidateRequest {
    pub tier: Option<String>,
    pub force: Option<bool>,
}

#[tauri::command]
pub async fn agentmemory_insights(
    store: State<'_, Arc<AgentMemoryState>>,
    request: AgentMemoryInsightsRequest,
) -> Result<AgentMemoryInsightsResponse, AppError> {
    let store = store.inner().clone();
    let working_dir = request.working_dir;
    run_memory_blocking(
        "agentmemory.insights.join_failed",
        "Failed to load agentmemory insights",
        move || fetch_agentmemory_insights(&store, &working_dir),
    )
    .await
}

#[tauri::command]
pub async fn agentmemory_consolidate(
    store: State<'_, Arc<AgentMemoryState>>,
    request: AgentMemoryConsolidateRequest,
) -> Result<serde_json::Value, AppError> {
    let store = store.inner().clone();
    let tier = request.tier;
    let force = request.force;
    run_memory_blocking(
        "agentmemory.consolidate.join_failed",
        "Failed to run agentmemory consolidation",
        move || store.run_consolidate(tier.as_deref(), force),
    )
    .await
}

fn fetch_agentmemory_insights(
    store: &AgentMemoryState,
    working_dir: &str,
) -> Result<AgentMemoryInsightsResponse, String> {
    store.ensure_ready()?;
    let mut errors = Vec::new();

    let sessions = match store.list_sessions() {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("sessions: {error}"));
            serde_json::json!({})
        }
    };

    let profile = match store.fetch_profile(working_dir) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("profile: {error}"));
            None
        }
    };

    let patterns = match store.fetch_patterns(working_dir) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("patterns: {error}"));
            None
        }
    };

    let graph_stats = match store.fetch_graph_stats() {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("graph: {error}"));
            None
        }
    };

    Ok(AgentMemoryInsightsResponse {
        sessions,
        profile,
        patterns,
        graph_stats,
        errors,
    })
}


