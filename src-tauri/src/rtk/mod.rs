use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use crate::process_util::{prepend_paths, suppress_command_window};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RtkRewriteMeta {
    pub enabled: bool,
    pub available: bool,
    pub rewritten: bool,
    pub original_command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_command: Option<String>,
}

impl RtkRewriteMeta {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }
}

type ManagedRtkDirs = Mutex<Vec<PathBuf>>;

const RTK_BUNDLE_HINT: &str =
    "Run `bun run rtk:bundle` in the repo root, set `LOCUS_RTK_PATH`, or install `rtk` on PATH.";

pub fn enabled() -> bool {
    match std::env::var("LOCUS_RTK_DISABLED")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("1") | Some("true") | Some("yes") => false,
        _ => true,
    }
}

pub fn set_managed_rtk_resource_dir(path: PathBuf) {
    let bundle = path.join("rtk");
    let dirs = managed_rtk_resource_dirs();
    let mut dirs = dirs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !dirs.iter().any(|existing| same_path(existing, &bundle)) {
        dirs.push(bundle);
    }
}

/// Rewrite a shell command through RTK when supported; otherwise return the original.
pub fn rewrite_command(command: &str) -> String {
    rewrite_with_meta(command)
        .executed_command
        .unwrap_or_else(|| command.to_string())
}

pub fn rewrite_with_meta(command: &str) -> RtkRewriteMeta {
    let original_command = command.to_string();
    if !enabled() {
        return RtkRewriteMeta {
            enabled: false,
            available: false,
            rewritten: false,
            original_command,
            executed_command: None,
        };
    }
    let Some(rtk) = resolve_rtk() else {
        return RtkRewriteMeta {
            enabled: true,
            available: false,
            rewritten: false,
            original_command,
            executed_command: None,
        };
    };
    let rewritten = rewrite_command_with_rtk(&rtk, command);
    let executed_command = rewritten.clone().unwrap_or_else(|| original_command.clone());
    RtkRewriteMeta {
        enabled: true,
        available: true,
        rewritten: rewritten.is_some() && rewritten.as_deref() != Some(original_command.as_str()),
        original_command,
        executed_command: Some(executed_command),
    }
}

/// Remap xLua paths, rewrite through RTK, then remap again so rewritten commands
/// (e.g. `rtk grep`) still target `Assets.Lua/...`.
pub fn rewrite_bash_with_meta(command: &str, workdir: Option<&Path>) -> RtkRewriteMeta {
    let prepped = crate::commands::remap_assets_lua_mispath_in_text(command, workdir);
    let mut meta = rewrite_with_meta(&prepped);
    let post_rtk = meta
        .executed_command
        .as_deref()
        .unwrap_or(prepped.as_str());
    let final_command = crate::commands::remap_assets_lua_mispath_in_text(post_rtk, workdir);
    if final_command != post_rtk {
        meta.executed_command = Some(final_command);
    }
    meta
}

pub fn augment_path_with_rtk(current_path: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    if !enabled() {
        return current_path;
    }
    let rtk = resolve_rtk()?;
    let rtk_dir = rtk.parent()?;
    prepend_paths(current_path, vec![rtk_dir.to_path_buf()])
}

fn rewrite_command_with_rtk(rtk: &Path, command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut cmd = Command::new(rtk);
    cmd.arg("rewrite").arg(trimmed);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    suppress_command_window(&mut cmd);

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let rewritten = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if rewritten.is_empty() {
        None
    } else {
        Some(rewritten)
    }
}

fn resolve_rtk() -> Option<PathBuf> {
    resolve_rtk_from_env()
        .or_else(resolve_rtk_from_path)
        .or_else(resolve_rtk_from_bundle)
}

fn resolve_rtk_from_env() -> Option<PathBuf> {
    let raw = std::env::var("LOCUS_RTK_PATH")
        .ok()
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())?;
    let path = PathBuf::from(&raw);
    if path.is_file() {
        return Some(path);
    }
    if path.is_dir() {
        return rtk_binary_in_dir(&path);
    }
    None
}

fn resolve_rtk_from_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in rtk_binary_names() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn resolve_rtk_from_bundle() -> Option<PathBuf> {
    for root in rtk_bundle_roots() {
        if let Some(rtk) = rtk_binary_in_dir(&root) {
            return Some(rtk);
        }
    }
    None
}

fn rtk_bundle_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(registered) = managed_rtk_resource_dirs().lock() {
        for root in registered.iter() {
            push_unique_bundle_root(&mut roots, root);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_unique_bundle_root(&mut roots, &exe_dir.join("resources").join("rtk"));
            push_unique_bundle_root(&mut roots, &exe_dir.join("rtk"));
        }
    }

    push_unique_bundle_root(
        &mut roots,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gen").join("rtk"),
    );

    roots
}

fn rtk_binary_in_dir(dir: &Path) -> Option<PathBuf> {
    for name in rtk_binary_names() {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
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

fn managed_rtk_resource_dirs() -> &'static ManagedRtkDirs {
    static DIRS: OnceLock<ManagedRtkDirs> = OnceLock::new();
    DIRS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(windows)]
fn rtk_binary_names() -> [&'static str; 1] {
    ["rtk.exe"]
}

#[cfg(not(windows))]
fn rtk_binary_names() -> [&'static str; 1] {
    ["rtk"]
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

pub fn progress_info(meta: &RtkRewriteMeta) -> String {
    serde_json::to_string(meta).unwrap_or_else(|_| "{}".to_string())
}

pub fn bundle_hint() -> &'static str {
    RTK_BUNDLE_HINT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_command_passthrough_when_disabled() {
        let prior = std::env::var("LOCUS_RTK_DISABLED").ok();
        std::env::set_var("LOCUS_RTK_DISABLED", "1");
        assert_eq!(rewrite_command("git status"), "git status");
        if let Some(value) = prior {
            std::env::set_var("LOCUS_RTK_DISABLED", value);
        } else {
            std::env::remove_var("LOCUS_RTK_DISABLED");
        }
    }

    #[test]
    fn rewrite_git_status_when_bundle_available() {
        if std::env::var("LOCUS_RTK_DISABLED").is_ok() {
            return;
        }
        let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("rtk");
        let rtk = rtk_binary_in_dir(&bundle);
        if rtk.is_none() {
            eprintln!("skipping: bundled rtk unavailable");
            return;
        }
        let rewritten = rewrite_command_with_rtk(&rtk.unwrap(), "git status")
            .expect("git status should rewrite");
        assert!(
            rewritten.contains("git") && rewritten.contains("status"),
            "unexpected rewrite: {rewritten}"
        );
        assert!(rewritten.starts_with("rtk"));
    }

    #[test]
    fn rewrite_bash_with_meta_remaps_paths_before_and_after_rtk() {
        if std::env::var("LOCUS_RTK_DISABLED").is_ok() {
            return;
        }
        let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("rtk");
        if rtk_binary_in_dir(&bundle).is_none() {
            eprintln!("skipping: bundled rtk unavailable");
            return;
        }

        let temp = tempfile::tempdir().expect("temp dir");
        let workspace = temp.path().join("project");
        std::fs::create_dir_all(workspace.join("Assets.Lua/Core")).expect("create assets lua");

        let command = r#"grep -n foo Assets/Lua/Core/foo.lua"#;
        let meta = rewrite_bash_with_meta(command, Some(&workspace));
        let executed = meta.executed_command.expect("executed command");
        assert!(
            executed.contains("Assets.Lua/Core/foo.lua"),
            "unexpected command: {executed}"
        );
        assert!(!executed.contains("Assets/Lua/"));
    }
}
