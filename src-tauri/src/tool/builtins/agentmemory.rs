use std::sync::Arc;

use super::{make_exec, ToolDef, ToolExecutionContext, ToolResult};
use tauri::Manager;

fn working_dir_or_error(ctx: &ToolExecutionContext, tool_name: &str) -> Result<String, ToolResult> {
    match ctx.working_dir.as_deref().map(str::trim) {
        Some(path) if !path.is_empty() => Ok(path.to_string()),
        _ => Err(ToolResult {
            output: format!(
                "Tool '{tool_name}' requires a selected project working directory."
            ),
            is_error: true,
        }),
    }
}

fn memory_store_or_error(
    ctx: &ToolExecutionContext,
    tool_name: &str,
) -> Result<Arc<crate::agentmemory::AgentMemoryState>, ToolResult> {
    let app_handle = ctx.app_handle.clone().ok_or_else(|| ToolResult {
        output: format!("Tool '{tool_name}' requires the Locus desktop runtime."),
        is_error: true,
    })?;
    app_handle
        .try_state::<Arc<crate::agentmemory::AgentMemoryState>>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| ToolResult {
            output: format!("Tool '{tool_name}' could not access agentmemory state."),
            is_error: true,
        })
}

fn json_output<T: serde::Serialize>(value: &T) -> ToolResult {
    match serde_json::to_string_pretty(value) {
        Ok(output) => ToolResult {
            output,
            is_error: false,
        },
        Err(error) => ToolResult {
            output: format!("Failed to serialize tool output: {error}"),
            is_error: true,
        },
    }
}

fn require_str(args: &serde_json::Value, key: &str) -> Result<String, ToolResult> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ToolResult {
            output: format!("Missing required parameter: {key}"),
            is_error: true,
        })
}

fn parse_concepts(args: &serde_json::Value) -> Vec<String> {
    args.get("concepts")
        .and_then(|value| value.as_str())
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_csv_files(args: &serde_json::Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn run_store_json<F>(ctx: &ToolExecutionContext, tool_name: &str, run: F) -> ToolResult
where
    F: FnOnce(std::sync::Arc<crate::agentmemory::AgentMemoryState>) -> Result<serde_json::Value, String>
        + Send
        + 'static,
{
    let store = match memory_store_or_error(ctx, tool_name) {
        Ok(store) => store,
        Err(result) => return result,
    };
    match tokio::task::spawn_blocking(move || run(store)).await {
        Ok(Ok(body)) => json_output(&body),
        Ok(Err(error)) => ToolResult {
            output: error,
            is_error: true,
        },
        Err(error) => ToolResult {
            output: format!("{tool_name} failed: {error}"),
            is_error: true,
        },
    }
}

pub fn register_all(registry: &mut super::ToolRegistry) {
    registry.register_builtin(memory_recall());
    registry.register_builtin(memory_smart_search());
    registry.register_builtin(memory_save());
    registry.register_builtin(memory_action_create());
    registry.register_builtin(memory_action_update());
    registry.register_builtin(memory_action_list());
    registry.register_builtin(memory_frontier());
    register_advanced_tools(registry);
}

fn register_advanced_tools(registry: &mut super::ToolRegistry) {
    registry.register_builtin(memory_sessions());
    registry.register_builtin(memory_patterns());
    registry.register_builtin(memory_timeline());
    registry.register_builtin(memory_profile());
    registry.register_builtin(memory_file_history());
    registry.register_builtin(memory_next());
    registry.register_builtin(memory_consolidate());
    registry.register_builtin(memory_graph_query());
    registry.register_builtin(memory_graph_stats());
    registry.register_builtin(memory_forget());
    registry.register_builtin(memory_evolve());
    registry.register_builtin(memory_commits());
    registry.register_builtin(memory_commit_lookup());
}

fn memory_recall() -> ToolDef {
    ToolDef {
        name: "memory_recall".to_string(),
        description: "Search past session observations for relevant context. Use when you need to recall what happened in previous sessions, find past decisions, or look up how a file was modified before.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query (keywords, file names, concepts)"
                },
                "limit": {
                    "type": "number",
                    "description": "Max results to return (default 10)"
                },
                "format": {
                    "type": "string",
                    "description": "Result format: full, compact, or narrative (default narrative)"
                }
            },
            "required": ["query"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
            let working_dir = match working_dir_or_error(&ctx, "memory_recall") {
                Ok(path) => path,
                Err(result) => return result,
            };
            let store = match memory_store_or_error(&ctx, "memory_recall") {
                Ok(store) => store,
                Err(result) => return result,
            };
            let query = match require_str(&args, "query") {
                Ok(query) => query,
                Err(result) => return result,
            };
            let limit = args.get("limit").and_then(|value| value.as_u64()).map(|v| v as usize);
            let format = args
                .get("format")
                .and_then(|value| value.as_str())
                .unwrap_or("narrative")
                .to_string();
            match tokio::task::spawn_blocking(move || {
                store.recall_search(&working_dir, &query, limit, &format)
            })
            .await
            {
                Ok(Ok(body)) => json_output(&body),
                Ok(Err(error)) => ToolResult {
                    output: error,
                    is_error: true,
                },
                Err(error) => ToolResult {
                    output: format!("memory_recall failed: {error}"),
                    is_error: true,
                },
            }
            })
        }),
    }
}

fn memory_smart_search() -> ToolDef {
    ToolDef {
        name: "memory_smart_search".to_string(),
        description: "Hybrid semantic+keyword search with progressive disclosure.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "expandIds": {
                    "type": "string",
                    "description": "Comma-separated observation IDs to expand"
                },
                "limit": {
                    "type": "number",
                    "description": "Max results (default 10)"
                }
            },
            "required": ["query"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
            let working_dir = match working_dir_or_error(&ctx, "memory_smart_search") {
                Ok(path) => path,
                Err(result) => return result,
            };
            let store = match memory_store_or_error(&ctx, "memory_smart_search") {
                Ok(store) => store,
                Err(result) => return result,
            };
            let query = match require_str(&args, "query") {
                Ok(query) => query,
                Err(result) => return result,
            };
            let limit = args.get("limit").and_then(|value| value.as_u64()).map(|v| v as usize);
            let expand_ids = args
                .get("expandIds")
                .and_then(|value| value.as_str())
                .map(|raw| {
                    raw.split(',')
                        .map(str::trim)
                        .filter(|part| !part.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|items| !items.is_empty());
            match tokio::task::spawn_blocking(move || {
                store.smart_search_raw(&working_dir, &query, limit, expand_ids)
            })
            .await
            {
                Ok(Ok(body)) => json_output(&body),
                Ok(Err(error)) => ToolResult {
                    output: error,
                    is_error: true,
                },
                Err(error) => ToolResult {
                    output: format!("memory_smart_search failed: {error}"),
                    is_error: true,
                },
            }
            })
        }),
    }
}

fn memory_save() -> ToolDef {
    ToolDef {
        name: "memory_save".to_string(),
        description: "Explicitly save an important insight, decision, or pattern to long-term memory.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The insight or decision to remember"
                },
                "type": {
                    "type": "string",
                    "description": "Memory type: pattern, preference, architecture, bug, workflow, or fact"
                },
                "concepts": {
                    "type": "string",
                    "description": "Comma-separated key concepts"
                }
            },
            "required": ["content"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
            let working_dir = match working_dir_or_error(&ctx, "memory_save") {
                Ok(path) => path,
                Err(result) => return result,
            };
            let store = match memory_store_or_error(&ctx, "memory_save") {
                Ok(store) => store,
                Err(result) => return result,
            };
            let content = match require_str(&args, "content") {
                Ok(content) => content,
                Err(result) => return result,
            };
            let mem_type = args
                .get("type")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let concepts = parse_concepts(&args);
            match tokio::task::spawn_blocking(move || {
                store.save_memory(
                    &working_dir,
                    &content,
                    mem_type.as_deref(),
                    &concepts,
                )
            })
            .await
            {
                Ok(Ok(body)) => json_output(&body),
                Ok(Err(error)) => ToolResult {
                    output: error,
                    is_error: true,
                },
                Err(error) => ToolResult {
                    output: format!("memory_save failed: {error}"),
                    is_error: true,
                },
            }
            })
        }),
    }
}

fn parse_requires(args: &serde_json::Value) -> Vec<String> {
    args.get("requires")
        .and_then(|value| value.as_str())
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn memory_action_create() -> ToolDef {
    ToolDef {
        name: "memory_action_create".to_string(),
        description: "Create a trackable follow-up work item (action) in agentmemory. Use when the session leaves concrete next steps, unfinished tasks, or dependencies for future runs.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Short action title"
                },
                "description": {
                    "type": "string",
                    "description": "Detailed description of the work"
                },
                "priority": {
                    "type": "number",
                    "description": "Priority 1-10 (10 highest, default 5)"
                },
                "tags": {
                    "type": "string",
                    "description": "Comma-separated tags"
                },
                "parentId": {
                    "type": "string",
                    "description": "Parent action ID for hierarchical actions"
                },
                "requires": {
                    "type": "string",
                    "description": "Comma-separated action IDs that must complete first"
                }
            },
            "required": ["title"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
            let working_dir = match working_dir_or_error(&ctx, "memory_action_create") {
                Ok(path) => path,
                Err(result) => return result,
            };
            let store = match memory_store_or_error(&ctx, "memory_action_create") {
                Ok(store) => store,
                Err(result) => return result,
            };
            let title = match require_str(&args, "title") {
                Ok(title) => title,
                Err(result) => return result,
            };
            let description = args
                .get("description")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let priority = args
                .get("priority")
                .and_then(|value| value.as_i64())
                .map(|value| value as i32);
            let tags = args
                .get("tags")
                .and_then(|value| value.as_str())
                .map(|raw| {
                    raw.split(',')
                        .map(str::trim)
                        .filter(|part| !part.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let parent_id = args
                .get("parentId")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let requires = parse_requires(&args);
            let session_id = ctx.session_id.clone().unwrap_or_else(|| "unknown".to_string());
            let project = crate::agentmemory::mapping::normalize_project_path(&working_dir);
            let project = if project.is_empty() { None } else { Some(project) };
            let request = crate::agentmemory::CreateAgentMemoryActionRequest {
                title,
                description,
                priority,
                project,
                created_by: Some(format!("locus:session:{session_id}")),
                tags,
                parent_id,
                requires,
            };
            match tokio::task::spawn_blocking(move || store.create_action(request)).await {
                Ok(Ok(action)) => json_output(&action),
                Ok(Err(error)) => ToolResult {
                    output: error,
                    is_error: true,
                },
                Err(error) => ToolResult {
                    output: format!("memory_action_create failed: {error}"),
                    is_error: true,
                },
            }
            })
        }),
    }
}

fn memory_action_update() -> ToolDef {
    ToolDef {
        name: "memory_action_update".to_string(),
        description: "Update an action status or details. Set status to 'done' when work is complete.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "actionId": {
                    "type": "string",
                    "description": "Action ID to update"
                },
                "status": {
                    "type": "string",
                    "description": "New status: pending, active, done, blocked, cancelled"
                },
                "title": {
                    "type": "string",
                    "description": "New title"
                },
                "description": {
                    "type": "string",
                    "description": "New description"
                },
                "priority": {
                    "type": "number",
                    "description": "New priority 1-10"
                },
                "result": {
                    "type": "string",
                    "description": "Outcome description when completing"
                }
            },
            "required": ["actionId"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
            let _working_dir = match working_dir_or_error(&ctx, "memory_action_update") {
                Ok(path) => path,
                Err(result) => return result,
            };
            let store = match memory_store_or_error(&ctx, "memory_action_update") {
                Ok(store) => store,
                Err(result) => return result,
            };
            let action_id = match require_str(&args, "actionId") {
                Ok(action_id) => action_id,
                Err(result) => return result,
            };
            let status = args
                .get("status")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let title = args
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let description = args
                .get("description")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let priority = args
                .get("priority")
                .and_then(|value| value.as_i64())
                .map(|value| value as i32);
            let result_text = args
                .get("result")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let request = crate::agentmemory::UpdateAgentMemoryActionRequest {
                action_id,
                status,
                title,
                description,
                priority,
                result: result_text,
            };
            match tokio::task::spawn_blocking(move || store.update_action(request)).await {
                Ok(Ok(action)) => json_output(&action),
                Ok(Err(error)) => ToolResult {
                    output: error,
                    is_error: true,
                },
                Err(error) => ToolResult {
                    output: format!("memory_action_update failed: {error}"),
                    is_error: true,
                },
            }
            })
        }),
    }
}

fn memory_action_list() -> ToolDef {
    ToolDef {
        name: "memory_action_list".to_string(),
        description: "List pending or active follow-up actions for the current project.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "description": "Filter by status: pending, active, done, blocked, cancelled"
                }
            }
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
            let working_dir = match working_dir_or_error(&ctx, "memory_action_list") {
                Ok(path) => path,
                Err(result) => return result,
            };
            let store = match memory_store_or_error(&ctx, "memory_action_list") {
                Ok(store) => store,
                Err(result) => return result,
            };
            let status = args
                .get("status")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            match tokio::task::spawn_blocking(move || {
                store.list_actions(&working_dir, status.as_deref())
            })
            .await
            {
                Ok(Ok(actions)) => json_output(&actions),
                Ok(Err(error)) => ToolResult {
                    output: error,
                    is_error: true,
                },
                Err(error) => ToolResult {
                    output: format!("memory_action_list failed: {error}"),
                    is_error: true,
                },
            }
            })
        }),
    }
}

fn memory_frontier() -> ToolDef {
    ToolDef {
        name: "memory_frontier".to_string(),
        description: "Get unblocked actions ranked by priority — the actionable frontier for this project.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "number",
                    "description": "Max actions to return (default 10)"
                }
            }
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
            let working_dir = match working_dir_or_error(&ctx, "memory_frontier") {
                Ok(path) => path,
                Err(result) => return result,
            };
            let store = match memory_store_or_error(&ctx, "memory_frontier") {
                Ok(store) => store,
                Err(result) => return result,
            };
            let limit = args.get("limit").and_then(|value| value.as_u64()).map(|v| v as usize);
            match tokio::task::spawn_blocking(move || store.fetch_frontier(&working_dir, limit)).await
            {
                Ok(Ok(body)) => json_output(&body),
                Ok(Err(error)) => ToolResult {
                    output: error,
                    is_error: true,
                },
                Err(error) => ToolResult {
                    output: format!("memory_frontier failed: {error}"),
                    is_error: true,
                },
            }
            })
        }),
    }
}

fn memory_sessions() -> ToolDef {
    ToolDef {
        name: "memory_sessions".to_string(),
        description: "List recent agentmemory sessions with status and observation counts.".to_string(),
        parameters: serde_json::json!({ "type": "object", "properties": {} }),
        execute: make_exec(|_args, ctx| {
            Box::pin(async move {
                run_store_json(&ctx, "memory_sessions", |store| store.list_sessions()).await
            })
        }),
    }
}

fn memory_patterns() -> ToolDef {
    ToolDef {
        name: "memory_patterns".to_string(),
        description: "Detect recurring patterns across sessions for the current project.".to_string(),
        parameters: serde_json::json!({ "type": "object", "properties": {} }),
        execute: make_exec(|_args, ctx| {
            Box::pin(async move {
                let working_dir = match working_dir_or_error(&ctx, "memory_patterns") {
                    Ok(path) => path,
                    Err(result) => return result,
                };
                run_store_json(&ctx, "memory_patterns", move |store| {
                    store.fetch_patterns(&working_dir)
                })
                .await
            })
        }),
    }
}

fn memory_timeline() -> ToolDef {
    ToolDef {
        name: "memory_timeline".to_string(),
        description: "Chronological observations around an anchor (ISO date or keyword).".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "anchor": { "type": "string", "description": "ISO timestamp or keyword anchor" },
                "before": { "type": "number", "description": "Observations before anchor (default 5)" },
                "after": { "type": "number", "description": "Observations after anchor (default 5)" }
            },
            "required": ["anchor"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let working_dir = match working_dir_or_error(&ctx, "memory_timeline") {
                    Ok(path) => path,
                    Err(result) => return result,
                };
                let anchor = match require_str(&args, "anchor") {
                    Ok(anchor) => anchor,
                    Err(result) => return result,
                };
                let before = args.get("before").and_then(|v| v.as_u64()).map(|v| v as usize);
                let after = args.get("after").and_then(|v| v.as_u64()).map(|v| v as usize);
                run_store_json(&ctx, "memory_timeline", move |store| {
                    store.fetch_timeline(&working_dir, &anchor, before, after)
                })
                .await
            })
        }),
    }
}

fn memory_profile() -> ToolDef {
    ToolDef {
        name: "memory_profile".to_string(),
        description: "Project memory profile: top concepts, file patterns, and usage summary.".to_string(),
        parameters: serde_json::json!({ "type": "object", "properties": {} }),
        execute: make_exec(|_args, ctx| {
            Box::pin(async move {
                let working_dir = match working_dir_or_error(&ctx, "memory_profile") {
                    Ok(path) => path,
                    Err(result) => return result,
                };
                run_store_json(&ctx, "memory_profile", move |store| {
                    store.fetch_profile(&working_dir, false)
                })
                .await
            })
        }),
    }
}

fn memory_file_history() -> ToolDef {
    ToolDef {
        name: "memory_file_history".to_string(),
        description: "Past observations about specific files across sessions.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "files": { "type": "string", "description": "Comma-separated file paths" }
            },
            "required": ["files"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let files = parse_csv_files(&args, "files");
                if files.is_empty() {
                    return ToolResult {
                        output: "Missing required parameter: files".to_string(),
                        is_error: true,
                    };
                }
                let session_id = ctx.session_id.clone();
                run_store_json(&ctx, "memory_file_history", move |store| {
                    store.fetch_file_history(&files, session_id.as_deref())
                })
                .await
            })
        }),
    }
}

fn memory_next() -> ToolDef {
    ToolDef {
        name: "memory_next".to_string(),
        description: "Pick the highest-priority unblocked action from the frontier.".to_string(),
        parameters: serde_json::json!({ "type": "object", "properties": {} }),
        execute: make_exec(|_args, ctx| {
            Box::pin(async move {
                let working_dir = match working_dir_or_error(&ctx, "memory_next") {
                    Ok(path) => path,
                    Err(result) => return result,
                };
                run_store_json(&ctx, "memory_next", move |store| {
                    store.fetch_next_action(&working_dir)
                })
                .await
            })
        }),
    }
}

fn memory_consolidate() -> ToolDef {
    ToolDef {
        name: "memory_consolidate".to_string(),
        description: "Run agentmemory 4-tier consolidation pipeline. Requires CONSOLIDATION_ENABLED on sidecar.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "tier": { "type": "string", "description": "Tier: working, episodic, semantic, procedural, or all" },
                "force": { "type": "boolean", "description": "Force consolidation even if recently run" }
            }
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let tier = args.get("tier").and_then(|v| v.as_str()).map(str::to_string);
                let force = args.get("force").and_then(|v| v.as_bool());
                run_store_json(&ctx, "memory_consolidate", move |store| {
                    store.run_consolidate(tier.as_deref(), force)
                })
                .await
            })
        }),
    }
}

fn memory_graph_query() -> ToolDef {
    ToolDef {
        name: "memory_graph_query".to_string(),
        description: "Query the agentmemory knowledge graph (requires GRAPH_EXTRACTION_ENABLED).".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search nodes by name" },
                "startNodeId": { "type": "string", "description": "BFS start node id" },
                "nodeType": { "type": "string", "description": "Filter by node type" },
                "maxDepth": { "type": "number", "description": "Max traversal depth (1-8)" }
            }
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let start_node_id = args
                    .get("startNodeId")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let node_type = args
                    .get("nodeType")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let max_depth = args.get("maxDepth").and_then(|v| v.as_u64()).map(|v| v as usize);
                run_store_json(&ctx, "memory_graph_query", move |store| {
                    store.query_graph(
                        query.as_deref(),
                        start_node_id.as_deref(),
                        node_type.as_deref(),
                        max_depth,
                    )
                })
                .await
            })
        }),
    }
}

fn memory_graph_stats() -> ToolDef {
    ToolDef {
        name: "memory_graph_stats".to_string(),
        description: "Knowledge graph node/edge counts and health.".to_string(),
        parameters: serde_json::json!({ "type": "object", "properties": {} }),
        execute: make_exec(|_args, ctx| {
            Box::pin(async move {
                run_store_json(&ctx, "memory_graph_stats", |store| store.fetch_graph_stats()).await
            })
        }),
    }
}

fn memory_forget() -> ToolDef {
    ToolDef {
        name: "memory_forget".to_string(),
        description: "Delete a long-term memory entry by id.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "memoryId": { "type": "string", "description": "Memory id to delete" }
            },
            "required": ["memoryId"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let memory_id = match require_str(&args, "memoryId") {
                    Ok(memory_id) => memory_id,
                    Err(result) => return result,
                };
                run_store_json(&ctx, "memory_forget", move |store| {
                    store.forget_memory_by_id(&memory_id)
                })
                .await
            })
        }),
    }
}

fn memory_evolve() -> ToolDef {
    ToolDef {
        name: "memory_evolve".to_string(),
        description: "Update an existing memory with revised content (creates lineage).".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "memoryId": { "type": "string", "description": "Memory id to evolve" },
                "newContent": { "type": "string", "description": "Updated memory content" },
                "newTitle": { "type": "string", "description": "Optional new title" }
            },
            "required": ["memoryId", "newContent"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let memory_id = match require_str(&args, "memoryId") {
                    Ok(memory_id) => memory_id,
                    Err(result) => return result,
                };
                let new_content = match require_str(&args, "newContent") {
                    Ok(content) => content,
                    Err(result) => return result,
                };
                let new_title = args
                    .get("newTitle")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string);
                run_store_json(&ctx, "memory_evolve", move |store| {
                    store.evolve_memory_entry(&memory_id, &new_content, new_title.as_deref())
                })
                .await
            })
        }),
    }
}

fn memory_commits() -> ToolDef {
    ToolDef {
        name: "memory_commits".to_string(),
        description: "List git commits linked to agent sessions.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "branch": { "type": "string", "description": "Filter by branch" },
                "repo": { "type": "string", "description": "Filter by remote URL" },
                "limit": { "type": "number", "description": "Max results (default 100)" }
            }
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let branch = args.get("branch").and_then(|v| v.as_str()).map(str::to_string);
                let repo = args.get("repo").and_then(|v| v.as_str()).map(str::to_string);
                let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);
                run_store_json(&ctx, "memory_commits", move |store| {
                    store.list_linked_commits(branch.as_deref(), repo.as_deref(), limit)
                })
                .await
            })
        }),
    }
}

fn memory_commit_lookup() -> ToolDef {
    ToolDef {
        name: "memory_commit_lookup".to_string(),
        description: "Find agent session(s) that produced a git commit SHA.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "sha": { "type": "string", "description": "Git commit SHA" }
            },
            "required": ["sha"]
        }),
        execute: make_exec(|args, ctx| {
            Box::pin(async move {
                let sha = match require_str(&args, "sha") {
                    Ok(sha) => sha,
                    Err(result) => return result,
                };
                run_store_json(&ctx, "memory_commit_lookup", move |store| {
                    store.lookup_session_by_commit(&sha)
                })
                .await
            })
        }),
    }
}
