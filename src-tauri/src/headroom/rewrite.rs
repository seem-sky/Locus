use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::process_util::{prepend_paths, suppress_command_window};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadroomRewriteMeta {
    pub enabled: bool,
    pub available: bool,
    pub rewritten: bool,
    pub original_command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_command: Option<String>,
}

pub fn rewrite_with_meta(command: &str) -> HeadroomRewriteMeta {
    let original_command = command.to_string();
    if !super::enabled() {
        return HeadroomRewriteMeta {
            enabled: false,
            available: false,
            rewritten: false,
            original_command,
            executed_command: None,
        };
    }
    let Some(rtk) = resolve_rtk() else {
        return HeadroomRewriteMeta {
            enabled: true,
            available: false,
            rewritten: false,
            original_command,
            executed_command: None,
        };
    };
    let rewritten = rewrite_command_with_rtk(&rtk, command);
    let executed_command = rewritten.clone().unwrap_or_else(|| original_command.clone());
    HeadroomRewriteMeta {
        enabled: true,
        available: true,
        rewritten: rewritten.is_some() && rewritten.as_deref() != Some(original_command.as_str()),
        original_command,
        executed_command: Some(executed_command),
    }
}

/// Remap xLua paths, rewrite through RTK, then remap again so rewritten commands
/// (e.g. `rtk grep`) still target `Assets.Lua/...`.
pub fn rewrite_bash_with_meta(command: &str, workdir: Option<&Path>) -> HeadroomRewriteMeta {
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

fn grep_tool_original_command(pattern: &str, path: &str, include: Option<&str>) -> String {
    match include.filter(|value| !value.is_empty()) {
        Some(include) => format!(
            "grep(pattern={pattern:?}, path={path:?}, include={include:?})"
        ),
        None => format!("grep(pattern={pattern:?}, path={path:?})"),
    }
}

/// Metadata when the built-in `grep` tool falls back to the native Rust searcher.
pub fn grep_tool_native_meta(pattern: &str, path: &str, include: Option<&str>) -> HeadroomRewriteMeta {
    let original_command = grep_tool_original_command(pattern, path, include);
    if !super::enabled() {
        return HeadroomRewriteMeta {
            enabled: false,
            available: false,
            rewritten: false,
            original_command,
            executed_command: None,
        };
    }
    let rtk_available = resolve_rtk().is_some();
    HeadroomRewriteMeta {
        enabled: true,
        available: rtk_available,
        rewritten: false,
        original_command,
        executed_command: Some("native Rust grep".to_string()),
    }
}

/// Run `rtk grep` when Headroom/RTK is enabled. Requires `rg` or Git `grep` on PATH (Git usr/bin is prepended).
pub fn try_execute_rtk_grep(
    pattern: &str,
    search_path: &str,
    include: Option<&str>,
    workdir: Option<&Path>,
) -> Option<(String, HeadroomRewriteMeta)> {
    if !super::enabled() {
        return None;
    }
    let rtk = resolve_rtk()?;
    let original_command = grep_tool_original_command(pattern, search_path, include);

    let mut executed = format!("rtk grep {pattern} {search_path}");
    let mut cmd = Command::new(&rtk);
    cmd.arg("grep").arg(pattern).arg(search_path);
    if let Some(include) = include.filter(|value| !value.is_empty()) {
        cmd.arg("--glob").arg(include);
        executed.push_str(&format!(" --glob {include}"));
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    suppress_command_window(&mut cmd);

    let path = augment_path_with_headroom_rtk(std::env::var_os("PATH"));
    let path = crate::process_util::augment_path_with_git(path);
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    if let Some(workdir) = workdir.filter(|path| path.is_dir()) {
        cmd.current_dir(workdir);
    }

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return None;
    }

    Some((
        stdout,
        HeadroomRewriteMeta {
            enabled: true,
            available: true,
            rewritten: true,
            original_command,
            executed_command: Some(executed),
        },
    ))
}

pub fn augment_path_with_headroom_rtk(
    current_path: Option<std::ffi::OsString>,
) -> Option<std::ffi::OsString> {
    if !super::enabled() {
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
        .or_else(|| crate::rtk_runtime::resolve_bundled_rtk())
        .or_else(resolve_rtk_from_headroom_bin)
        .or_else(resolve_rtk_from_path)
}

fn resolve_rtk_from_env() -> Option<PathBuf> {
    let raw = std::env::var("LOCUS_HEADROOM_RTK_PATH")
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

fn resolve_rtk_from_headroom_bin() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    rtk_binary_in_dir(&home.join(".headroom").join("bin"))
}

fn resolve_rtk_from_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in crate::rtk_runtime::rtk_binary_names() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn rtk_binary_in_dir(dir: &Path) -> Option<PathBuf> {
    crate::rtk_runtime::rtk_binary_in_dir(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_passthrough_when_disabled() {
        let prior = std::env::var("LOCUS_HEADROOM_DISABLED").ok();
        std::env::set_var("LOCUS_HEADROOM_DISABLED", "1");
        let meta = rewrite_with_meta("git status");
        assert!(!meta.enabled);
        assert_eq!(meta.original_command, "git status");
        if let Some(value) = prior {
            std::env::set_var("LOCUS_HEADROOM_DISABLED", value);
        } else {
            std::env::remove_var("LOCUS_HEADROOM_DISABLED");
        }
    }

    #[test]
    fn rewrite_bash_with_meta_remaps_paths_before_and_after_rtk() {
        if std::env::var("LOCUS_HEADROOM_DISABLED").is_ok() {
            return;
        }
        if resolve_rtk().is_none() {
            eprintln!("skipping: headroom rtk unavailable");
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
