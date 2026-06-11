/*
 * @Author         : seem.sky@gmail.com
 * @Email          : seem.sky@gmail.com
 * @Description    :
 * @FilePath       : \src-tauri\src\tool\builtins\codegraph.rs
 * @Date           : 2026-05-25 15:07:23
 * @LastEditTime   : 2026-05-28 10:55:26
 * @LastEditors    : seem.sky@gmail.com seem.sky@gmail.com
 */
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use super::{make_exec, ToolDef, ToolExecutionContext, ToolResult};
use crate::process_util::async_command;

const CODEGRAPH_BUNDLE_HINT: &str = "Run `bun run codegraph:bundle` in the repo root, set `LOCUS_CODEGRAPH_PATH` to the bundle or CLI executable, or install `codegraph` on PATH.";

#[derive(Debug, Clone)]
struct ResolvedCodegraph {
    program: PathBuf,
    prefix_args: Vec<String>,
}

type ManagedCodegraphDirs = Mutex<Vec<PathBuf>>;

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const SYNC_TIMEOUT_SECS: u64 = 600;
const INIT_TIMEOUT_SECS: u64 = 1_800;
const MAX_OUTPUT_CHARS: usize = 200_000;

fn ensure_index_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub fn has_initialized_index(project_path: &Path) -> bool {
    project_path
        .join(".codegraph")
        .join("codegraph.db")
        .is_file()
}

pub async fn init_project_index(project_path: &Path) -> Result<(), String> {
    let project = cli_arg_path(project_path);
    eprintln!(
        "[CodeGraph] initializing index for {} (this may take a while)...",
        project_path.display()
    );
    let cmd_args = vec!["init".to_string(), "-i".to_string(), project];
    run_codegraph_command(cmd_args, INIT_TIMEOUT_SECS)
        .await
        .map(|_| ())?;
    if !has_initialized_index(project_path) {
        return Err(format!(
            "CodeGraph init finished but index is still missing at {}",
            project_path
                .join(".codegraph")
                .join("codegraph.db")
                .display()
        ));
    }
    eprintln!(
        "[CodeGraph] index ready at {}",
        project_path.join(".codegraph").display()
    );
    Ok(())
}

pub async fn ensure_project_index(project_path: &Path) -> Result<(), String> {
    if has_initialized_index(project_path) {
        return Ok(());
    }
    let _guard = ensure_index_lock().lock().await;
    if has_initialized_index(project_path) {
        return Ok(());
    }
    init_project_index(project_path).await
}

pub async fn sync_project_index(project_path: &Path) -> Result<(), String> {
    ensure_project_index(project_path).await?;
    let project = cli_arg_path(project_path);
    let cmd_args = vec!["sync".to_string(), "-q".to_string(), project];
    run_codegraph_command(cmd_args, SYNC_TIMEOUT_SECS)
        .await
        .map(|_| ())
}

pub fn set_managed_codegraph_resource_dir(path: PathBuf) {
    let bundle = path.join("codegraph-bundle");
    let dirs = managed_codegraph_resource_dirs();
    let mut dirs = dirs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !dirs.iter().any(|existing| same_path(existing, &bundle)) {
        dirs.push(bundle);
    }
}

/// Returns embedded Node (or Windows node.exe) plus args needed to run an arbitrary JS entrypoint
/// from the bundled codegraph runtime.
pub fn resolve_codegraph_node_for_script(script: &Path) -> Option<(PathBuf, Vec<String>)> {
    for root in codegraph_bundle_roots() {
        if let Some(resolved) = resolve_codegraph_node_for_script_in_root(&root, script) {
            return Some(resolved);
        }
        let npm_root = root
            .join("node_modules")
            .join(format!("@colbymchenry/codegraph-{}", codegraph_platform_target()));
        if let Some(resolved) = resolve_codegraph_node_for_script_in_root(&npm_root, script) {
            return Some(resolved);
        }
    }
    None
}

fn resolve_codegraph_node_for_script_in_root(
    root: &Path,
    script: &Path,
) -> Option<(PathBuf, Vec<String>)> {
    #[cfg(windows)]
    {
        let node_exe = root.join("node.exe");
        if node_exe.is_file() && script.is_file() {
            return Some((
                node_exe,
                vec![
                    "--liftoff-only".to_string(),
                    cli_arg_path(script),
                ],
            ));
        }
        return None;
    }

    #[cfg(not(windows))]
    {
        for node in embedded_node_candidates(root) {
            if node.is_file() && script.is_file() {
                return Some((
                    node,
                    vec![
                        "--liftoff-only".to_string(),
                        cli_arg_path(script),
                    ],
                ));
            }
        }
        None
    }
}

#[cfg(not(windows))]
fn embedded_node_candidates(root: &Path) -> [PathBuf; 2] {
    [
        root.join("lib").join("node").join("bin").join("node"),
        root.join("bin").join("node"),
    ]
}

pub fn register_all(registry: &mut super::ToolRegistry) {
    registry.register_builtin(codegraph_search());
    registry.register_builtin(codegraph_context());
    registry.register_builtin(codegraph_callers());
    registry.register_builtin(codegraph_callees());
    registry.register_builtin(codegraph_impact());
    registry.register_builtin(codegraph_files());
    registry.register_builtin(codegraph_status());
    registry.register_builtin(codegraph_sync());
    registry.register_builtin(codegraph_trace());
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

fn codegraph_trace() -> ToolDef {
    tool_from_prompt(
        "codegraph_trace",
        crate::prompt::tools::CODEGRAPH_TRACE,
        |args, ctx| async move {
            let from = match require_str(&args, "from") {
                Ok(from) => from,
                Err(error) => return error,
            };
            let to = match require_str(&args, "to") {
                Ok(to) => to,
                Err(error) => return error,
            };
            let mut cmd_args = vec!["trace".to_string(), from, to];
            if let Some(depth) = args.get("depth").and_then(|v| v.as_u64()) {
                cmd_args.push("--depth".to_string());
                cmd_args.push(depth.to_string());
            }
            append_common_flags(&mut cmd_args, &args, true);
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
        mutates_workspace: false,
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
        return Ok(cli_arg_path(Path::new(&path)));
    }
    if let Some(working_dir) = ctx
        .working_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(cli_arg_path(Path::new(working_dir)));
    }
    Err(ToolResult {
        output: "Missing project path: set `path` or select a working directory.".to_string(),
        is_error: true,
    })
}

fn append_project_path_flag(cmd_args: &mut Vec<String>, project_path: String) {
    cmd_args.push("-p".to_string());
    cmd_args.push(cli_arg_path(Path::new(&project_path)));
}

/// Node on Windows re-parses argv from the command line; backslash sequences such as `\n`
/// in `\node_modules` or `\b` in `\bin` corrupt paths like `G:\AI\...` down to `G:`.
fn cli_arg_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        text.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
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

    if let Some(project_path) = project_path_for_command(&cmd_args, args, &ctx) {
        if let Err(message) = ensure_project_index(&project_path).await {
            return ToolResult {
                output: message,
                is_error: true,
            };
        }
    }

    let timeout_secs = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(5, 600);

    let original_command = format!("codegraph {}", cmd_args.join(" "));
    let tool_name = cmd_args
        .first()
        .map(|value| format!("codegraph_{value}"))
        .unwrap_or_else(|| "codegraph".to_string());

    match run_codegraph_command(cmd_args, timeout_secs).await {
        Ok(output) => {
            let rewrite_meta = crate::headroom::tool_native_meta(
                &tool_name,
                &original_command,
                Some("codegraph CLI"),
            );
            let (mut body, compress_meta) = crate::headroom::maybe_compress_tool_output(
                output,
                ctx.llm_model.as_deref(),
            )
            .await;
            body = cap_codegraph_output(body);
            crate::headroom::record_execution_meta(
                ctx.execution_meta_sink.as_ref(),
                rewrite_meta,
                compress_meta,
            );
            ToolResult {
                output: body,
                is_error: false,
            }
        }
        Err(message) => ToolResult {
            output: message,
            is_error: true,
        },
    }
}

fn project_path_for_command(
    cmd_args: &[String],
    args: &serde_json::Value,
    ctx: &ToolExecutionContext,
) -> Option<PathBuf> {
    if matches!(
        cmd_args.first().map(String::as_str),
        Some("status") | Some("sync")
    ) {
        return cmd_args
            .get(1)
            .map(|path| PathBuf::from(path))
            .or_else(|| resolve_project_path(args, ctx).ok().map(PathBuf::from));
    }

    let flag_index = cmd_args.iter().position(|arg| arg == "-p")?;
    cmd_args
        .get(flag_index + 1)
        .map(|path| PathBuf::from(path))
        .or_else(|| resolve_project_path(args, ctx).ok().map(PathBuf::from))
}

async fn run_codegraph_command(
    cmd_args: Vec<String>,
    timeout_secs: u64,
) -> Result<String, String> {
    let resolved = resolve_codegraph()?;

    let program = resolved
        .program
        .to_str()
        .unwrap_or("codegraph")
        .to_string();
    let mut command = async_command(&program);
    if !resolved.prefix_args.is_empty() {
        command.args(&resolved.prefix_args);
    }
    command.args(&cmd_args);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let child = command
        .spawn()
        .map_err(|error| format!("Failed to run codegraph CLI: {}. {}", error, CODEGRAPH_BUNDLE_HINT))?;

    let output = match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
        .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("codegraph process failed: {}", error)),
        Err(_) => {
            return Err(format!(
                "codegraph timed out after {} seconds",
                timeout_secs
            ));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            format!("codegraph exited with status {}", output.status)
        };
        return Err(detail);
    }

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

    Ok(combined)
}

fn cap_codegraph_output(mut output: String) -> String {
    if output.len() > MAX_OUTPUT_CHARS {
        output.truncate(MAX_OUTPUT_CHARS);
        output.push_str("\n\n(Output truncated)");
    }
    output
}

fn resolve_codegraph() -> Result<ResolvedCodegraph, String> {
    resolve_codegraph_from_env()
        .or_else(resolve_codegraph_from_path)
        .or_else(resolve_codegraph_from_bundle)
        .ok_or_else(|| format!("CodeGraph is not available. {}", CODEGRAPH_BUNDLE_HINT))
}

fn resolve_codegraph_from_env() -> Option<ResolvedCodegraph> {
    let raw = std::env::var("LOCUS_CODEGRAPH_PATH")
        .ok()
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())?;
    let path = PathBuf::from(&raw);
    if path.is_file() {
        return Some(ResolvedCodegraph {
            program: path,
            prefix_args: Vec::new(),
        });
    }
    if path.is_dir() {
        return resolve_codegraph_from_bundle_root(&path);
    }
    None
}

fn resolve_codegraph_from_path() -> Option<ResolvedCodegraph> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in codegraph_path_binary_names() {
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            #[cfg(windows)]
            if candidate.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cmd")) {
                return Some(ResolvedCodegraph {
                    program: PathBuf::from(std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string())),
                    prefix_args: vec![
                        "/C".to_string(),
                        cli_arg_path(&candidate),
                    ],
                });
            }
            return Some(ResolvedCodegraph {
                program: candidate,
                prefix_args: Vec::new(),
            });
        }
    }
    None
}

fn resolve_codegraph_from_bundle() -> Option<ResolvedCodegraph> {
    for root in codegraph_bundle_roots() {
        if let Some(resolved) = resolve_codegraph_from_bundle_root(&root) {
            return Some(resolved);
        }
    }
    None
}

fn resolve_codegraph_from_bundle_root(root: &Path) -> Option<ResolvedCodegraph> {
    if let Some(resolved) = resolve_codegraph_from_runtime_root(root) {
        return Some(resolved);
    }

    let npm_root = root
        .join("node_modules")
        .join(format!("@colbymchenry/codegraph-{}", codegraph_platform_target()));
    resolve_codegraph_from_runtime_root(&npm_root)
}

fn resolve_codegraph_from_runtime_root(root: &Path) -> Option<ResolvedCodegraph> {
    #[cfg(windows)]
    {
        let node_exe = root.join("node.exe");
        let entry = root
            .join("lib")
            .join("dist")
            .join("bin")
            .join("codegraph.js");
        if node_exe.is_file() && entry.is_file() {
            return Some(ResolvedCodegraph {
                program: node_exe,
                prefix_args: vec![
                    "--liftoff-only".to_string(),
                    cli_arg_path(&entry),
                ],
            });
        }
        return None;
    }

    #[cfg(not(windows))]
    {
        let launcher = root.join("bin").join("codegraph");
        if launcher.is_file() {
            return Some(ResolvedCodegraph {
                program: launcher,
                prefix_args: Vec::new(),
            });
        }
        None
    }
}

fn codegraph_bundle_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(registered) = managed_codegraph_resource_dirs().lock() {
        for root in registered.iter() {
            push_unique_bundle_root(&mut roots, root);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_unique_bundle_root(&mut roots, &exe_dir.join("resources").join("codegraph-bundle"));
            push_unique_bundle_root(&mut roots, &exe_dir.join("codegraph-bundle"));
        }
    }

    push_unique_bundle_root(
        &mut roots,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("codegraph-bundle"),
    );

    roots
}

fn push_unique_bundle_root(target: &mut Vec<PathBuf>, candidate: &Path) {
    if !candidate.is_dir() {
        return;
    }
    if target.iter().any(|existing| same_path(existing, candidate)) {
        return;
    }
    target.push(candidate.to_path_buf());
}

fn managed_codegraph_resource_dirs() -> &'static ManagedCodegraphDirs {
    static DIRS: OnceLock<ManagedCodegraphDirs> = OnceLock::new();
    DIRS.get_or_init(|| Mutex::new(Vec::new()))
}

fn codegraph_platform_target() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return "win32-x64";
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        return "win32-arm64";
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return "darwin-x64";
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return "darwin-arm64";
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return "linux-x64";
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return "linux-arm64";
    }
    #[allow(unreachable_code)]
    "unknown"
}

fn codegraph_path_binary_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["codegraph.exe", "codegraph.cmd", "codegraph"]
    }
    #[cfg(not(windows))]
    {
        &["codegraph"]
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    dunce::canonicalize(left)
        .unwrap_or_else(|_| left.to_path_buf())
        .as_os_str()
        .eq_ignore_ascii_case(
            &dunce::canonicalize(right)
                .unwrap_or_else(|_| right.to_path_buf())
                .as_os_str(),
        )
}

#[cfg(test)]
pub(crate) fn resolve_codegraph_from_bundle_root_for_test(root: &Path) -> Option<ResolvedCodegraph> {
    resolve_codegraph_from_bundle_root(root)
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
            "codegraph_trace",
        ] {
            assert!(
                registry.canonical_name(name).is_some(),
                "missing builtin tool {}",
                name
            );
            assert!(registry.is_built_in(name));
        }
    }

    #[test]
    fn resolve_codegraph_prefers_flat_windows_bundle_layout() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("node.exe"), b"").expect("node.exe");
        let entry_dir = root.join("lib").join("dist").join("bin");
        std::fs::create_dir_all(&entry_dir).expect("entry dirs");
        std::fs::write(entry_dir.join("codegraph.js"), b"").expect("entry");

        let resolved =
            resolve_codegraph_from_bundle_root_for_test(root).expect("bundle layout");
        assert!(resolved.program.ends_with("node.exe"));
        assert_eq!(resolved.prefix_args.len(), 2);
        assert_eq!(resolved.prefix_args[0], "--liftoff-only");
        assert!(resolved.prefix_args[1].ends_with("codegraph.js"));
    }

    #[cfg(not(windows))]
    #[test]
    fn resolve_codegraph_prefers_unix_bundle_launcher() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let bin_dir = root.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("bin dir");
        let launcher = bin_dir.join("codegraph");
        std::fs::write(&launcher, b"#!/bin/sh\n").expect("launcher");
        let mut permissions = std::fs::metadata(&launcher)
            .expect("metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&launcher, permissions).expect("chmod");

        let resolved =
            resolve_codegraph_from_bundle_root_for_test(root).expect("bundle layout");
        assert!(resolved.program.ends_with("bin/codegraph"));
        assert!(resolved.prefix_args.is_empty());
    }

    #[test]
    fn cli_arg_path_normalizes_windows_backslashes() {
        let normalized = cli_arg_path(Path::new(r"G:\AI\Locus\src\node_modules\bin\codegraph.js"));
        #[cfg(windows)]
        assert_eq!(
            normalized,
            "G:/AI/Locus/src/node_modules/bin/codegraph.js"
        );
        #[cfg(not(windows))]
        assert_eq!(
            normalized,
            r"G:\AI\Locus\src\node_modules\bin\codegraph.js"
        );
    }

    #[tokio::test]
    async fn codegraph_search_runs_against_g_drive_workspace() {
        if resolve_codegraph().is_err() {
            eprintln!("skipping: CodeGraph bundle/CLI unavailable in this test environment");
            return;
        }

        let registry = ToolRegistry::with_builtins();
        let result = registry
            .execute_with_context(
                "codegraph_search",
                &serde_json::json!({ "query": "run_codegraph", "limit": 1 }),
                ToolExecutionContext {
                    working_dir: Some(r"G:\AI\Locus".to_string()),
                    ..Default::default()
                },
            )
            .await;
        assert!(
            !result.is_error,
            "codegraph_search failed: {}",
            result.output
        );
        assert!(!result.output.trim().is_empty());
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
