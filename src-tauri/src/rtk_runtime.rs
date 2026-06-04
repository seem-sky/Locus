use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

type ManagedRtkDirs = Mutex<Vec<PathBuf>>;

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

/// Absolute path to the bundled `rtk` binary, if present.
pub fn resolve_bundled_rtk() -> Option<PathBuf> {
    for root in rtk_bundle_roots() {
        if let Some(binary) = rtk_binary_in_dir(&root) {
            return Some(binary);
        }
    }
    None
}

/// Default value for Headroom settings (`rtk_path`): bundled binary path, else bundle directory.
pub fn default_rtk_path_for_settings() -> String {
    if let Some(binary) = resolve_bundled_rtk() {
        return binary.display().to_string();
    }
    rtk_bundle_roots()
        .into_iter()
        .next()
        .map(|dir| dir.display().to_string())
        .unwrap_or_default()
}

pub fn rtk_binary_in_dir(dir: &Path) -> Option<PathBuf> {
    for name in rtk_binary_names() {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
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

fn managed_rtk_resource_dirs() -> &'static ManagedRtkDirs {
    static DIRS: OnceLock<ManagedRtkDirs> = OnceLock::new();
    DIRS.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_unique_bundle_root(roots: &mut Vec<PathBuf>, candidate: &Path) {
    if candidate.is_dir() && !roots.iter().any(|existing| same_path(existing, candidate)) {
        roots.push(candidate.to_path_buf());
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .map(|(left, right)| left == right)
            .unwrap_or(false)
}

#[cfg(windows)]
pub fn rtk_binary_names() -> [&'static str; 1] {
    ["rtk.exe"]
}

#[cfg(not(windows))]
pub fn rtk_binary_names() -> [&'static str; 1] {
    ["rtk"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rtk_path_uses_dev_bundle_when_present() {
        let dev_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gen").join("rtk");
        if !dev_root.is_dir() {
            return;
        }
        let default_path = default_rtk_path_for_settings();
        assert!(!default_path.is_empty());
        assert!(PathBuf::from(default_path).exists());
    }
}
