use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone)]
pub struct ResolvedLuaRuntime {
    pub lua: PathBuf,
    pub luac: PathBuf,
    pub dir: PathBuf,
}

type ManagedLuaDirs = Mutex<Vec<PathBuf>>;

const LUA_BUNDLE_HINT: &str =
    "Place Lua 5.3 binaries under src-tauri/gen/lua53, set `LOCUS_LUA_PATH`, or install `lua`/`luac` on PATH.";

pub fn bundle_hint() -> &'static str {
    LUA_BUNDLE_HINT
}

pub fn set_managed_lua_resource_dir(path: PathBuf) {
    let bundle = path.join("lua53");
    let dirs = managed_lua_resource_dirs();
    let mut dirs = dirs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !dirs.iter().any(|existing| same_path(existing, &bundle)) {
        dirs.push(bundle);
    }
}

pub fn resolve_bundled_lua() -> Option<ResolvedLuaRuntime> {
    resolve_lua_from_env().or_else(resolve_lua_from_bundle)
}

pub fn augment_path_with_lua(current_path: Option<OsString>) -> Option<OsString> {
    let runtime = resolve_bundled_lua()?;
    prepend_lua_to_path(current_path, &runtime)
}

pub fn prepend_lua_to_path(
    current_path: Option<OsString>,
    runtime: &ResolvedLuaRuntime,
) -> Option<OsString> {
    let mut paths = Vec::new();
    if let Some(shim_dir) = ensure_lua_shim_dir(runtime) {
        paths.push(shim_dir);
    }
    paths.push(runtime.dir.clone());
    crate::process_util::prepend_paths(current_path, paths)
}

pub fn sh_lua_function_prefix(runtime: &ResolvedLuaRuntime) -> String {
    let lua = shell_quote_posix(&runtime.lua.display().to_string().replace('\\', "/"));
    let luac = shell_quote_posix(&runtime.luac.display().to_string().replace('\\', "/"));
    format!(
        "lua() {{ {} \"$@\"; }}\nluac() {{ {} \"$@\"; }}\n",
        lua, luac
    )
}

fn resolve_lua_from_env() -> Option<ResolvedLuaRuntime> {
    let raw = std::env::var("LOCUS_LUA_PATH")
        .ok()
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())?;
    let path = PathBuf::from(&raw);
    if path.is_file() {
        let dir = path.parent()?.to_path_buf();
        return runtime_from_dir(&dir);
    }
    if path.is_dir() {
        return runtime_from_dir(&path);
    }
    None
}

fn resolve_lua_from_bundle() -> Option<ResolvedLuaRuntime> {
    for root in lua_bundle_roots() {
        if let Some(runtime) = runtime_from_dir(&root) {
            return Some(runtime);
        }
    }
    None
}

fn runtime_from_dir(dir: &Path) -> Option<ResolvedLuaRuntime> {
    let lua = lua_binary_in_dir(dir, &lua_binary_names())?;
    let luac = lua_binary_in_dir(dir, &luac_binary_names()).unwrap_or_else(|| lua.clone());
    Some(ResolvedLuaRuntime {
        lua,
        luac,
        dir: dir.to_path_buf(),
    })
}

fn lua_bundle_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(registered) = managed_lua_resource_dirs().lock() {
        for root in registered.iter() {
            push_unique_bundle_root(&mut roots, root);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_unique_bundle_root(&mut roots, &exe_dir.join("resources").join("lua53"));
            push_unique_bundle_root(&mut roots, &exe_dir.join("lua53"));
        }
    }

    push_unique_bundle_root(
        &mut roots,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gen").join("lua53"),
    );

    roots
}

fn lua_binary_in_dir(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    for name in names {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(windows)]
fn lua_binary_names() -> [&'static str; 2] {
    ["lua53.exe", "lua.exe"]
}

#[cfg(not(windows))]
fn lua_binary_names() -> [&'static str; 2] {
    ["lua53", "lua"]
}

#[cfg(windows)]
fn luac_binary_names() -> [&'static str; 2] {
    ["luac53.exe", "luac.exe"]
}

#[cfg(not(windows))]
fn luac_binary_names() -> [&'static str; 2] {
    ["luac53", "luac"]
}

fn ensure_lua_shim_dir(runtime: &ResolvedLuaRuntime) -> Option<PathBuf> {
    let dir = crate::commands::persistent_config_dir()
        .ok()?
        .join("runtime-shims")
        .join("lua");
    std::fs::create_dir_all(&dir).ok()?;

    #[cfg(target_os = "windows")]
    {
        let lua_target = runtime.lua.display().to_string();
        let luac_target = runtime.luac.display().to_string();
        let lua_shim = format!("@echo off\r\n\"{}\" %*\r\n", lua_target);
        let luac_shim = format!("@echo off\r\n\"{}\" %*\r\n", luac_target);
        std::fs::write(dir.join("lua.cmd"), &lua_shim).ok()?;
        std::fs::write(dir.join("luac.cmd"), &luac_shim).ok()?;
    }

    let lua_target = shell_quote_posix(&runtime.lua.display().to_string().replace('\\', "/"));
    let luac_target = shell_quote_posix(&runtime.luac.display().to_string().replace('\\', "/"));
    let lua_sh = format!("#!/bin/sh\nexec {} \"$@\"\n", lua_target);
    let luac_sh = format!("#!/bin/sh\nexec {} \"$@\"\n", luac_target);
    let lua = dir.join("lua");
    let luac = dir.join("luac");
    std::fs::write(&lua, &lua_sh).ok()?;
    std::fs::write(&luac, &luac_sh).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::Permissions::from_mode(0o755);
        let _ = std::fs::set_permissions(&lua, mode.clone());
        let _ = std::fs::set_permissions(&luac, mode);
    }

    Some(dir)
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

fn managed_lua_resource_dirs() -> &'static ManagedLuaDirs {
    static DIRS: OnceLock<ManagedLuaDirs> = OnceLock::new();
    DIRS.get_or_init(|| Mutex::new(Vec::new()))
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

fn shell_quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_prefix_defines_lua_and_luac_functions() {
        let runtime = ResolvedLuaRuntime {
            lua: PathBuf::from("G:/AI/Locus/src-tauri/gen/lua53/lua53.exe"),
            luac: PathBuf::from("G:/AI/Locus/src-tauri/gen/lua53/luac53.exe"),
            dir: PathBuf::from("G:/AI/Locus/src-tauri/gen/lua53"),
        };
        let prefix = sh_lua_function_prefix(&runtime);
        assert!(prefix.contains("lua()"));
        assert!(prefix.contains("luac()"));
        assert!(prefix.contains("'G:/AI/Locus/src-tauri/gen/lua53/lua53.exe'"));
        assert!(prefix.contains("'G:/AI/Locus/src-tauri/gen/lua53/luac53.exe'"));
    }

    #[test]
    fn bundled_lua_resolves_from_gen_dir() {
        let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("lua53");
        if !bundle.is_dir() {
            eprintln!("skipping: bundled lua53 unavailable");
            return;
        }
        let runtime = runtime_from_dir(&bundle);
        assert!(
            runtime.is_some(),
            "expected lua runtime under {}",
            bundle.display()
        );
        let runtime = runtime.unwrap();
        assert!(runtime.lua.is_file(), "lua binary missing");
        assert!(runtime.luac.is_file(), "luac binary missing");
    }
}
