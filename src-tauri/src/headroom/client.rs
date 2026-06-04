use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::process_util::suppress_command_window;
use crate::session::models::ChatMessage;

use super::messages::{apply_compressed_messages, to_headroom_openai_messages};
use super::{HeadroomCompressMeta, HeadroomRewriteMeta};

const DEFAULT_MODEL: &str = "gpt-4o";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompressResponse {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    messages: Option<Vec<Value>>,
    #[serde(default)]
    tokens_before: Option<u64>,
    #[serde(default)]
    tokens_after: Option<u64>,
    #[serde(default)]
    tokens_saved: Option<u64>,
    #[serde(default)]
    compression_ratio: Option<f64>,
    #[serde(default)]
    transforms_applied: Vec<String>,
    #[serde(default)]
    ccr_hashes: Vec<String>,
    #[serde(default)]
    error: Option<String>,
}

struct JsRuntime {
    program: PathBuf,
    script_args: Vec<String>,
}

pub fn min_compress_chars() -> usize {
    super::settings::min_compress_chars()
}

pub fn library_available() -> bool {
    if !super::enabled() {
        return false;
    }
    if !compress_script_path().is_file() {
        return false;
    }
    if !headroom_module_root().join("node_modules").join("headroom-ai").is_dir() {
        return false;
    }
    resolve_js_runtime().is_some()
}

pub fn context_compress_enabled() -> bool {
    super::settings::context_compress_enabled()
}

pub fn context_library_available() -> bool {
    context_compress_enabled() && library_available()
}

/// RTK-rewritten commands are already compressed at execution time; only fall back to
/// headroom-ai for passthrough commands with large output.
pub fn compress_bash_output(
    content: &str,
    model: Option<&str>,
    rewrite: &HeadroomRewriteMeta,
) -> (String, Option<HeadroomCompressMeta>) {
    if rewrite.rewritten {
        return (content.to_string(), None);
    }
    let (out, meta) = compress_tool_output(content, model);
    (out, Some(meta))
}

pub fn compress_chat_messages(
    system_parts: &[&str],
    messages: &[ChatMessage],
    model: Option<&str>,
) -> (Vec<ChatMessage>, HeadroomCompressMeta) {
    let original_chars = total_message_chars(messages);
    let skipped = HeadroomCompressMeta {
        enabled: super::enabled(),
        available: false,
        compressed: false,
        original_chars,
        compressed_chars: None,
        tokens_before: None,
        tokens_after: None,
        tokens_saved: None,
        compression_ratio: None,
        transforms_applied: Vec::new(),
        ccr_hashes: Vec::new(),
        error: None,
    };

    if !context_compress_enabled() {
        return (messages.to_vec(), skipped);
    }

    if messages.is_empty() {
        return (
            messages.to_vec(),
            HeadroomCompressMeta {
                enabled: true,
                available: false,
                compressed: false,
                original_chars,
                compressed_chars: Some(0),
                ..skipped
            },
        );
    }

    let model_name = normalize_model(model);
    let payload = json!({
        "mode": "messages",
        "model": model_name,
        "messages": to_headroom_openai_messages(system_parts, messages),
    });

    match run_compress_script(&payload) {
        Ok(parsed) => {
            let compressed_messages = parsed
                .messages
                .as_ref()
                .filter(|values| !values.is_empty())
                .and_then(|values| apply_compressed_messages(messages, values).ok())
                .unwrap_or_else(|| messages.to_vec());

            let compressed_chars = total_message_chars(&compressed_messages);
            let changed = compressed_messages
                .iter()
                .zip(messages.iter())
                .any(|(left, right)| left.content != right.content)
                || compressed_messages.len() != messages.len();
            let meta = HeadroomCompressMeta {
                enabled: true,
                available: true,
                compressed: changed,
                original_chars,
                compressed_chars: Some(compressed_chars),
                tokens_before: parsed.tokens_before,
                tokens_after: parsed.tokens_after,
                tokens_saved: parsed.tokens_saved,
                compression_ratio: parsed.compression_ratio,
                transforms_applied: parsed.transforms_applied,
                ccr_hashes: parsed.ccr_hashes,
                error: None,
            };
            (compressed_messages, meta)
        }
        Err(meta) => (messages.to_vec(), meta),
    }
}

pub fn compress_tool_output(content: &str, model: Option<&str>) -> (String, HeadroomCompressMeta) {
    let original_chars = content.chars().count();
    let skipped = HeadroomCompressMeta {
        enabled: super::enabled(),
        available: false,
        compressed: false,
        original_chars,
        compressed_chars: None,
        tokens_before: None,
        tokens_after: None,
        tokens_saved: None,
        compression_ratio: None,
        transforms_applied: Vec::new(),
        ccr_hashes: Vec::new(),
        error: None,
    };

    if !super::enabled() {
        return (content.to_string(), skipped);
    }

    if original_chars < min_compress_chars() {
        return (
            content.to_string(),
            HeadroomCompressMeta {
                enabled: true,
                available: false,
                compressed: false,
                original_chars,
                compressed_chars: Some(original_chars),
                ..skipped
            },
        );
    }

    let payload = json!({
        "content": content,
        "model": normalize_model(model),
        "toolCallId": "bash-output",
    });

    match run_compress_script(&payload) {
        Ok(parsed) => {
            let compressed = parsed
                .content
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| content.to_string());
            let compressed_chars = compressed.chars().count();
            let meta = HeadroomCompressMeta {
                enabled: true,
                available: true,
                compressed: compressed != content,
                original_chars,
                compressed_chars: Some(compressed_chars),
                tokens_before: parsed.tokens_before,
                tokens_after: parsed.tokens_after,
                tokens_saved: parsed.tokens_saved,
                compression_ratio: parsed.compression_ratio,
                transforms_applied: parsed.transforms_applied,
                ccr_hashes: parsed.ccr_hashes,
                error: None,
            };
            (compressed, meta)
        }
        Err(meta) => (content.to_string(), meta),
    }
}

fn run_compress_script(payload: &Value) -> Result<CompressResponse, HeadroomCompressMeta> {
    let original_chars = payload
        .get("content")
        .and_then(|value| value.as_str())
        .map(|value| value.chars().count())
        .unwrap_or_else(|| {
            payload
                .get("messages")
                .and_then(|value| value.as_array())
                .map(|messages| {
                    messages
                        .iter()
                        .filter_map(|message| {
                            message
                                .get("content")
                                .and_then(|content| content.as_str())
                                .map(|text| text.chars().count())
                        })
                        .sum()
                })
                .unwrap_or(0)
        });

    if crate::headroom::settings::proxy_autostart_wanted() {
        if let Err(error) = super::ensure_proxy_ready() {
            return Err(skipped_meta(original_chars, false, Some(error)));
        }
    }

    let script = compress_script_path();
    if !script.is_file() {
        return Err(skipped_meta(
            original_chars,
            false,
            Some(format!("Headroom compress script not found: {}", script.display())),
        ));
    }

    let module_root = headroom_module_root();
    if !module_root.join("node_modules").join("headroom-ai").is_dir() {
        return Err(skipped_meta(
            original_chars,
            false,
            Some(format!(
                "headroom-ai is not installed (expected node_modules under {})",
                module_root.display()
            )),
        ));
    }

    let Some(runtime) = resolve_js_runtime() else {
        return Err(skipped_meta(
            original_chars,
            false,
            Some(
                "JavaScript runtime unavailable (install bun or node, or set LOCUS_BUN_EXE)"
                    .to_string(),
            ),
        ));
    };

    let mut cmd = Command::new(&runtime.program);
    for arg in &runtime.script_args {
        cmd.arg(arg);
    }
    cmd.arg(&script)
        .current_dir(&module_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_headroom_env(&mut cmd);
    suppress_command_window(&mut cmd);

    let mut child = cmd.spawn().map_err(|error| {
        skipped_meta(
            original_chars,
            false,
            Some(format!("failed to spawn Headroom compress script: {error}")),
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(payload.to_string().as_bytes());
    }

    let output = child.wait_with_output().map_err(|error| {
        skipped_meta(
            original_chars,
            false,
            Some(format!("Headroom compress process failed: {error}")),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: CompressResponse = serde_json::from_str(stdout.trim()).map_err(|error| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        skipped_meta(
            original_chars,
            true,
            Some(format!(
                "invalid Headroom compress response: {error}; stderr={stderr}"
            )),
        )
    })?;

    if !output.status.success() {
        return Err(skipped_meta(
            original_chars,
            true,
            Some(
                parsed
                    .error
                    .unwrap_or_else(|| format!("Headroom compress exited {}", output.status)),
            ),
        ));
    }

    if let Some(error) = parsed
        .error
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
    {
        return Err(skipped_meta(original_chars, true, Some(error)));
    }

    Ok(parsed)
}

fn skipped_meta(
    original_chars: usize,
    available: bool,
    error: Option<String>,
) -> HeadroomCompressMeta {
    HeadroomCompressMeta {
        enabled: super::enabled(),
        available,
        compressed: false,
        original_chars,
        compressed_chars: None,
        tokens_before: None,
        tokens_after: None,
        tokens_saved: None,
        compression_ratio: None,
        transforms_applied: Vec::new(),
        ccr_hashes: Vec::new(),
        error,
    }
}

fn normalize_model(model: Option<&str>) -> String {
    model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MODEL)
        .to_string()
}

fn total_message_chars(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|message| message.content.chars().count()).sum()
}

fn apply_headroom_env(cmd: &mut Command) {
    cmd.env("HEADROOM_BASE_URL", super::settings::base_url());
    match super::settings::api_key() {
        Some(key) => {
            cmd.env("HEADROOM_API_KEY", key);
        }
        None => {
            cmd.env_remove("HEADROOM_API_KEY");
        }
    }
}

fn compress_script_path() -> PathBuf {
    if let Ok(raw) = std::env::var("LOCUS_HEADROOM_SCRIPT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if let Some(bundled) = bundled_headroom_root() {
        let script = bundled.join("headroom-compress.mjs");
        if script.is_file() {
            return script;
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let bundled = exe_dir
                .join("resources")
                .join("headroom")
                .join("headroom-compress.mjs");
            if bundled.is_file() {
                return bundled;
            }
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scripts")
        .join("headroom-compress.mjs")
}

fn headroom_module_root() -> PathBuf {
    if let Some(bundled) = bundled_headroom_root() {
        if bundled.join("node_modules").join("headroom-ai").is_dir() {
            return bundled;
        }
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    if repo_root.join("node_modules").join("headroom-ai").is_dir() {
        return repo_root;
    }

    bundled_headroom_root().unwrap_or(repo_root)
}

fn bundled_headroom_root() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("LOCUS_HEADROOM_BUNDLE") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for candidate in [
                exe_dir.join("resources").join("headroom-bundle"),
                exe_dir.join("headroom-bundle"),
            ] {
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    }

    let dev_bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("gen")
        .join("headroom-bundle");
    if dev_bundle.is_dir() {
        return Some(dev_bundle);
    }

    None
}

fn resolve_js_runtime() -> Option<JsRuntime> {
    if let Some(bun) = find_bun_exe() {
        return Some(JsRuntime {
            program: bun,
            script_args: Vec::new(),
        });
    }

    find_node_exe().map(|node| JsRuntime {
        program: node,
        script_args: Vec::new(),
    })
}

fn find_bun_exe() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("LOCUS_BUN_EXE") {
        let path = PathBuf::from(explicit.trim());
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(home) = dirs::home_dir() {
        for name in bun_binary_names() {
            let candidate = home.join(".bun").join("bin").join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    find_on_path(bun_binary_names())
}

fn find_node_exe() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("LOCUS_NODE_EXE") {
        let path = PathBuf::from(explicit.trim());
        if path.is_file() {
            return Some(path);
        }
    }

    find_on_path(node_binary_names())
}

fn find_on_path(names: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn bun_binary_names() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["bun.exe", "bun"]
    }
    #[cfg(not(target_os = "windows"))]
    {
        &["bun"]
    }
}

fn node_binary_names() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["node.exe", "node"]
    }
    #[cfg(not(target_os = "windows"))]
    {
        &["node"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_compress_chars_default() {
        let prior = std::env::var("LOCUS_HEADROOM_MIN_COMPRESS_CHARS").ok();
        std::env::remove_var("LOCUS_HEADROOM_MIN_COMPRESS_CHARS");
        assert_eq!(min_compress_chars(), 2000);
        if let Some(value) = prior {
            std::env::set_var("LOCUS_HEADROOM_MIN_COMPRESS_CHARS", value);
        }
    }

    #[test]
    fn compress_script_path_points_at_repo_script_in_dev() {
        let path = compress_script_path();
        assert!(
            path.ends_with("headroom-compress.mjs"),
            "unexpected script path: {}",
            path.display()
        );
    }

    #[test]
    fn compress_bash_output_skips_post_compress_when_rtk_rewrote() {
        use super::super::HeadroomRewriteMeta;

        let rewrite = HeadroomRewriteMeta {
            enabled: true,
            available: true,
            rewritten: true,
            original_command: "git status".to_string(),
            executed_command: Some("rtk git status".to_string()),
        };
        let (out, meta) = compress_bash_output("large output", None, &rewrite);
        assert_eq!(out, "large output");
        assert!(meta.is_none());
    }

    #[test]
    fn context_compress_enabled_by_default() {
        let prior = std::env::var("LOCUS_HEADROOM_CONTEXT_COMPRESS").ok();
        std::env::remove_var("LOCUS_HEADROOM_CONTEXT_COMPRESS");
        assert!(context_compress_enabled());
        if let Some(value) = prior {
            std::env::set_var("LOCUS_HEADROOM_CONTEXT_COMPRESS", value);
        }
    }
}
