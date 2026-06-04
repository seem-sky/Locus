use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tauri::AppHandle;

pub const HEADROOM_PROXY_BUNDLE_HINT: &str =
    "Run `bun run python:bundle && bun run headroom:proxy:bundle`, set `LOCUS_HEADROOM_CLI`, \
     or install `pip install \"headroom-ai[proxy]\"` on PATH.";

const PROXY_MODULE: &str = "headroom.cli";
const DEFAULT_PROXY_PORT: u16 = 8787;

#[derive(Debug, Clone)]
pub struct ResolvedHeadroomProxy {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
    pub working_dir: PathBuf,
    pub using_bundled_runtime: bool,
    pub bundle_version: Option<String>,
}

type ManagedHeadroomProxyDirs = Mutex<Vec<PathBuf>>;

pub fn set_managed_headroom_proxy_resource_dir(path: PathBuf) {
    let bundle = path.join("headroom-proxy-bundle");
    let dirs = managed_headroom_proxy_resource_dirs();
    let mut dirs = dirs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !dirs.iter().any(|existing| same_path(existing, &bundle)) {
        dirs.push(bundle);
    }
}

pub fn bundled_proxy_lib_present() -> bool {
    headroom_proxy_bundle_roots()
        .into_iter()
        .any(|root| root.join("lib").join("headroom").is_dir())
}

pub fn resolve_headroom_proxy(app_handle: Option<&AppHandle>) -> Result<ResolvedHeadroomProxy, String> {
    resolve_headroom_proxy_from_env()
        .or_else(|| resolve_headroom_proxy_from_bundle(app_handle))
        .or_else(resolve_headroom_proxy_from_path)
        .ok_or_else(|| format!("headroom proxy CLI is not available. {HEADROOM_PROXY_BUNDLE_HINT}"))
}

pub fn proxy_port_from_base_url(base_url: &str) -> u16 {
    let trimmed = base_url.trim();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find("]:") {
            let port = &trimmed[end + 2..];
            if let Ok(value) = port.split('/').next().unwrap_or(port).parse::<u16>() {
                return value;
            }
        }
        return DEFAULT_PROXY_PORT;
    }
    if let Some((_, port)) = trimmed.rsplit_once(':') {
        let port = port.split('/').next().unwrap_or(port);
        if let Ok(value) = port.parse::<u16>() {
            return value;
        }
    }
    DEFAULT_PROXY_PORT
}

fn resolve_headroom_proxy_from_env() -> Option<ResolvedHeadroomProxy> {
    let raw = std::env::var("LOCUS_HEADROOM_CLI")
        .ok()
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())?;
    let path = PathBuf::from(&raw);
    if !path.is_file() {
        return None;
    }
    let working_dir = path.parent()?.to_path_buf();
    Some(ResolvedHeadroomProxy {
        program: path,
        prefix_args: vec!["proxy".to_string()],
        working_dir,
        using_bundled_runtime: false,
        bundle_version: None,
    })
}

fn resolve_headroom_proxy_from_path() -> Option<ResolvedHeadroomProxy> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in headroom_cli_names() {
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            return Some(ResolvedHeadroomProxy {
                program: candidate,
                prefix_args: vec!["proxy".to_string()],
                working_dir: dir,
                using_bundled_runtime: false,
                bundle_version: None,
            });
        }
    }
    None
}

fn resolve_headroom_proxy_from_bundle(app_handle: Option<&AppHandle>) -> Option<ResolvedHeadroomProxy> {
    for root in headroom_proxy_bundle_roots() {
        if let Some(resolved) = resolve_headroom_proxy_from_bundle_root(&root, app_handle) {
            return Some(resolved);
        }
    }
    None
}

fn resolve_headroom_proxy_from_bundle_root(
    root: &Path,
    app_handle: Option<&AppHandle>,
) -> Option<ResolvedHeadroomProxy> {
    let lib_dir = root.join("lib");
    let headroom_pkg = lib_dir.join("headroom");
    if !headroom_pkg.is_dir() {
        return None;
    }

    let python = resolve_python_for_headroom_proxy(app_handle)?;
    let port = proxy_port_from_base_url(&crate::headroom::settings::base_url());
    let bundle_version = read_bundle_version_from_root(root);

    Some(ResolvedHeadroomProxy {
        program: python,
        prefix_args: vec![
            "-m".to_string(),
            PROXY_MODULE.to_string(),
            "proxy".to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            port.to_string(),
        ],
        working_dir: root.to_path_buf(),
        using_bundled_runtime: true,
        bundle_version,
    })
}

pub fn read_bundle_version_from_root(root: &Path) -> Option<String> {
    let version_path = root.join("version.txt");
    if !version_path.is_file() {
        return None;
    }
    std::fs::read_to_string(&version_path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_python_for_headroom_proxy(app_handle: Option<&AppHandle>) -> Option<PathBuf> {
    if let Some(runtime) = crate::python_runtime::resolve_effective_python(app_handle) {
        if runtime.path.is_file() {
            return Some(runtime.path);
        }
    }

    for root in managed_python_roots() {
        let python = root.join("windows-x64").join("python.exe");
        if python.is_file() {
            return Some(python);
        }
    }

    find_on_path("python3").or_else(|| find_on_path("python"))
}

fn managed_python_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_unique_dir(&mut roots, &exe_dir.join("resources").join("managed-python"));
            push_unique_dir(&mut roots, &exe_dir.join("managed-python"));
        }
    }
    push_unique_dir(
        &mut roots,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("managed-python"),
    );
    roots
}

fn headroom_proxy_bundle_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(raw) = std::env::var("LOCUS_HEADROOM_PROXY_BUNDLE") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            push_unique_dir(&mut roots, &PathBuf::from(trimmed));
        }
    }

    if let Ok(registered) = managed_headroom_proxy_resource_dirs().lock() {
        for root in registered.iter() {
            push_unique_dir(&mut roots, root);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_unique_dir(
                &mut roots,
                &exe_dir.join("resources").join("headroom-proxy-bundle"),
            );
            push_unique_dir(&mut roots, &exe_dir.join("headroom-proxy-bundle"));
        }
    }

    push_unique_dir(
        &mut roots,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("headroom-proxy-bundle"),
    );

    roots
}

fn managed_headroom_proxy_resource_dirs() -> &'static ManagedHeadroomProxyDirs {
    static DIRS: OnceLock<ManagedHeadroomProxyDirs> = OnceLock::new();
    DIRS.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_unique_dir(roots: &mut Vec<PathBuf>, candidate: &Path) {
    if candidate.is_dir() && !roots.iter().any(|existing| same_path(existing, candidate)) {
        roots.push(candidate.to_path_buf());
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn same_path(left: &Path, right: &Path) -> bool {
    std::fs::canonicalize(left)
        .ok()
        .zip(std::fs::canonicalize(right).ok())
        .map(|(left, right)| left == right)
        .unwrap_or(false)
}

#[cfg(windows)]
fn headroom_cli_names() -> [&'static str; 2] {
    ["headroom.exe", "headroom"]
}

#[cfg(not(windows))]
fn headroom_cli_names() -> [&'static str; 1] {
    ["headroom"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_port_from_localhost_url() {
        assert_eq!(
            proxy_port_from_base_url("http://localhost:8787"),
            8787
        );
        assert_eq!(proxy_port_from_base_url("http://127.0.0.1:9999"), 9999);
    }

    #[test]
    fn bundled_proxy_root_requires_lib() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gen").join("headroom-proxy-bundle");
        if !root.join("lib").join("headroom").is_dir() {
            return;
        }
        let resolved = resolve_headroom_proxy_from_bundle_root(&root, None);
        assert!(resolved.is_some(), "expected bundled proxy when lib/headroom exists");
        let resolved = resolved.expect("resolved");
        assert!(resolved.using_bundled_runtime);
        assert_eq!(resolved.prefix_args[0], "-m");
        assert_eq!(resolved.prefix_args[1], PROXY_MODULE);
    }
}
