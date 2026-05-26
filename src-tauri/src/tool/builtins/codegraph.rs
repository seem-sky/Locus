use std::time::Duration;

use super::{make_exec, ToolDef, ToolExecutionContext, ToolResult};
use crate::process_util::async_command;

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_OUTPUT_CHARS: usize = 200_000;

pub fn register_all(registry: &mut super::ToolRegistry) {
    registry.register_builtin(codegraph_search());
    registry.register_builtin(codegraph_context());
    registry.register_builtin(codegraph_callers());
    registry.register_builtin(codegraph_callees());
    registry.register_builtin(codegraph_impact());
    registry.register_builtin(codegraph_files());
    registry.register_builtin(codegraph_status());
    registry.register_builtin(codegraph_sync());
}

fn codegraph_search() -> ToolDef {
    tool_from_prompt(
        "codegraph_search",
        crate::prompt::tools::CODEGRAPH_SEARCH,
        |args, ctx| async move {
            let query = match require_str(&args, "query") {
                Ok(query) => query,
                Err(error) => return error,
            };
            let mut cmd_args = vec!["query".to_string(), query];
            append_common_flags(&mut cmd_args, &args, true);
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn codegraph_context() -> ToolDef {
    tool_from_prompt(
        "codegraph_context",
        crate::prompt::tools::CODEGRAPH_CONTEXT,
        |args, ctx| async move {
            let task = match require_str(&args, "task") {
                Ok(task) => task,
                Err(error) => return error,
            };
            let mut cmd_args = vec!["context".to_string(), task];
            if let Some(max_nodes) = args.get("maxNodes").and_then(|v| v.as_u64()) {
                cmd_args.push("--max-nodes".to_string());
                cmd_args.push(max_nodes.to_string());
            }
            if let Some(max_code) = args.get("maxCode").and_then(|v| v.as_u64()) {
                cmd_args.push("--max-code".to_string());
                cmd_args.push(max_code.to_string());
            }
            if args.get("noCode").and_then(|v| v.as_bool()) == Some(true) {
                cmd_args.push("--no-code".to_string());
            }
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn codegraph_callers() -> ToolDef {
    tool_from_prompt(
        "codegraph_callers",
        crate::prompt::tools::CODEGRAPH_CALLERS,
        |args, ctx| async move {
            let target = match require_str(&args, "target") {
                Ok(target) => target,
                Err(error) => return error,
            };
            let mut cmd_args = vec!["callers".to_string(), target];
            append_common_flags(&mut cmd_args, &args, true);
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn codegraph_callees() -> ToolDef {
    tool_from_prompt(
        "codegraph_callees",
        crate::prompt::tools::CODEGRAPH_CALLEES,
        |args, ctx| async move {
            let target = match require_str(&args, "target") {
                Ok(target) => target,
                Err(error) => return error,
            };
            let mut cmd_args = vec!["callees".to_string(), target];
            append_common_flags(&mut cmd_args, &args, true);
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn codegraph_impact() -> ToolDef {
    tool_from_prompt(
        "codegraph_impact",
        crate::prompt::tools::CODEGRAPH_IMPACT,
        |args, ctx| async move {
            let target = match require_str(&args, "target") {
                Ok(target) => target,
                Err(error) => return error,
            };
            let mut cmd_args = vec!["impact".to_string(), target];
            if let Some(depth) = args.get("depth").and_then(|v| v.as_u64()) {
                cmd_args.push("--depth".to_string());
                cmd_args.push(depth.to_string());
            }
            let project_path = match resolve_project_path(&args, &ctx) {
                Ok(path) => path,
                Err(error) => return error,
            };
            append_project_path_flag(&mut cmd_args, project_path);
            cmd_args.push("-j".to_string());
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn codegraph_files() -> ToolDef {
    tool_from_prompt(
        "codegraph_files",
        crate::prompt::tools::CODEGRAPH_FILES,
        |args, ctx| async move {
            let mut cmd_args = vec!["files".to_string()];
            if let Some(filter) = optional_str(&args, "filter") {
                cmd_args.push("--filter".to_string());
                cmd_args.push(filter);
            }
            if let Some(pattern) = optional_str(&args, "pattern") {
                cmd_args.push("--pattern".to_string());
                cmd_args.push(pattern);
            }
            if let Some(format) = optional_str(&args, "format") {
                cmd_args.push("--format".to_string());
                cmd_args.push(format);
            }
            if let Some(max_depth) = args.get("maxDepth").and_then(|v| v.as_u64()) {
                cmd_args.push("--max-depth".to_string());
                cmd_args.push(max_depth.to_string());
            }
            let project_path = match resolve_project_path(&args, &ctx) {
                Ok(path) => path,
                Err(error) => return error,
            };
            append_project_path_flag(&mut cmd_args, project_path);
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn codegraph_status() -> ToolDef {
    tool_from_prompt(
        "codegraph_status",
        crate::prompt::tools::CODEGRAPH_STATUS,
        |args, ctx| async move {
            let project_path = match resolve_project_path(&args, &ctx) {
                Ok(path) => path,
                Err(error) => return error,
            };
            let cmd_args = vec!["status".to_string(), project_path];
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn codegraph_sync() -> ToolDef {
    tool_from_prompt(
        "codegraph_sync",
        crate::prompt::tools::CODEGRAPH_SYNC,
        |args, ctx| async move {
            let project_path = match resolve_project_path(&args, &ctx) {
                Ok(path) => path,
                Err(error) => return error,
            };
            let cmd_args = vec!["sync".to_string(), project_path];
            run_codegraph(ctx, cmd_args, &args).await
        },
    )
}

fn tool_from_prompt<F, Fut>(name: &str, prompt_json: &str, handler: F) -> ToolDef
where
    F: Fn(serde_json::Value, ToolExecutionContext) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ToolResult> + Send + 'static,
{
    let prompt = crate::prompt::parse_tool_prompt(prompt_json);
    ToolDef {
        name: name.to_string(),
        description: prompt.description,
        parameters: prompt.parameters,
        execute: make_exec(move |args, ctx| Box::pin(handler(args, ctx))),
    }
}

fn require_str(args: &serde_json::Value, key: &str) -> Result<String, ToolResult> {
    match optional_str(args, key) {
        Some(value) => Ok(value),
        None => Err(ToolResult {
            output: format!("Missing required parameter: {}", key),
            is_error: true,
        }),
    }
}

fn optional_str(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn resolve_project_path(
    args: &serde_json::Value,
    ctx: &ToolExecutionContext,
) -> Result<String, ToolResult> {
    if let Some(path) = optional_str(args, "path") {
        return Ok(path);
    }
    if let Some(working_dir) = ctx
        .working_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(working_dir.to_string());
    }
    Err(ToolResult {
        output: "Missing project path: set `path` or select a working directory.".to_string(),
        is_error: true,
    })
}

fn append_project_path_flag(cmd_args: &mut Vec<String>, project_path: String) {
    cmd_args.push("-p".to_string());
    cmd_args.push(project_path);
}

fn append_common_flags(cmd_args: &mut Vec<String>, args: &serde_json::Value, json_output: bool) {
    if let Some(limit) = args.get("limit").and_then(|v| v.as_u64()) {
        cmd_args.push("-l".to_string());
        cmd_args.push(limit.to_string());
    }
    if let Some(kind) = optional_str(args, "kind") {
        cmd_args.push("-k".to_string());
        cmd_args.push(kind);
    }
    if json_output {
        cmd_args.push("-j".to_string());
    }
}

async fn run_codegraph(
    ctx: ToolExecutionContext,
    mut cmd_args: Vec<String>,
    args: &serde_json::Value,
) -> ToolResult {
    let uses_positional_project_path = matches!(
        cmd_args.first().map(String::as_str),
        Some("status") | Some("sync")
    );
    if !uses_positional_project_path && !cmd_args.iter().any(|arg| arg == "-p") {
        if let Ok(project_path) = resolve_project_path(args, &ctx) {
            append_project_path_flag(&mut cmd_args, project_path);
        }
    }

    let timeout_secs = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(5, 600);

    let mut command = async_command("codegraph");
    command.args(&cmd_args);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return ToolResult {
                output: format!(
                    "Failed to run codegraph CLI: {}. Install CodeGraph and ensure `codegraph` is on PATH.",
                    error
                ),
                is_error: true,
            };
        }
    };

    let output = match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
        .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return ToolResult {
                output: format!("codegraph process failed: {}", error),
                is_error: true,
            };
        }
        Err(_) => {
            return ToolResult {
                output: format!(
                    "codegraph timed out after {} seconds",
                    timeout_secs
                ),
                is_error: true,
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = stdout.trim().to_string();
    if !stderr.trim().is_empty() {
        if !combined.is_empty() {
            combined.push_str("\n\n");
        }
        combined.push_str("stderr:\n");
        combined.push_str(stderr.trim());
    }

    if combined.len() > MAX_OUTPUT_CHARS {
        combined.truncate(MAX_OUTPUT_CHARS);
        combined.push_str("\n\n(Output truncated)");
    }

    if combined.is_empty() && !output.status.success() {
        combined = format!("codegraph exited with status {}", output.status);
    }

    ToolResult {
        output: combined,
        is_error: !output.status.success(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{ToolExecutionContext, ToolRegistry};

    #[test]
    fn codegraph_tools_are_registered_as_builtins() {
        let registry = ToolRegistry::with_builtins();
        for name in [
            "codegraph_search",
            "codegraph_context",
            "codegraph_callers",
            "codegraph_callees",
            "codegraph_impact",
            "codegraph_files",
            "codegraph_status",
            "codegraph_sync",
        ] {
            assert!(
                registry.canonical_name(name).is_some(),
                "missing builtin tool {}",
                name
            );
            assert!(registry.is_built_in(name));
        }
    }

    #[tokio::test]
    async fn codegraph_search_requires_query() {
        let registry = ToolRegistry::with_builtins();
        let result = registry
            .execute_with_context(
                "codegraph_search",
                &serde_json::json!({}),
                ToolExecutionContext {
                    working_dir: Some(env!("CARGO_MANIFEST_DIR").to_string()),
                    ..Default::default()
                },
            )
            .await;
        assert!(result.is_error);
        assert!(result.output.contains("query"));
    }
}
