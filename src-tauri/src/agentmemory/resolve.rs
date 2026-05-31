use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

pub const AGENTMEMORY_BUNDLE_HINT: &str =
    "Run `bun run codegraph:bundle && bun run agentmemory:bundle`, set `LOCUS_AGENTMEMORY_PATH`, \
     or install `agentmemory` on PATH.";

#[derive(Debug, Clone)]
pub struct ResolvedAgentmemory {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
    pub working_dir: PathBuf,
    pub iii_bin_dir: PathBuf,
    pub bundle_root: PathBuf,
    pub using_bundled_runtime: bool,
    pub bundle_version: Option<String>,
}

type ManagedAgentmemoryDirs = Mutex<Vec<PathBuf>>;

#[derive(Debug, Deserialize)]
struct BundleManifest {
    #[serde(rename = "agentmemoryVersion")]
    agentmemory_version: Option<String>,
}

pub fn set_managed_agentmemory_resource_dir(path: PathBuf) {
    let bundle = path.join("agentmemory-bundle");
    let dirs = managed_agentmemory_resource_dirs();
    let mut dirs = dirs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !dirs.iter().any(|existing| same_path(existing, &bundle)) {
        dirs.push(bundle);
    }
}

pub fn resolve_agentmemory() -> Result<ResolvedAgentmemory, String> {
    resolve_agentmemory_from_env()
        .or_else(resolve_agentmemory_from_path)
        .or_else(resolve_agentmemory_from_bundle)
        .ok_or_else(|| format!("agentmemory is not available. {}", AGENTMEMORY_BUNDLE_HINT))
}

pub fn read_bundle_version_from_root(root: &Path) -> Option<String> {
    let manifest_path = root.join("manifest.json");
    if manifest_path.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<BundleManifest>(&raw) {
                if let Some(version) = manifest.agentmemory_version.filter(|v| !v.is_empty()) {
                    return Some(version);
                }
            }
        }
    }
    let version_path = root.join("version.txt");
    if version_path.is_file() {
        return std::fs::read_to_string(&version_path)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    None
}

fn resolve_agentmemory_from_env() -> Option<ResolvedAgentmemory> {
    let raw = std::env::var("LOCUS_AGENTMEMORY_PATH")
        .ok()
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())?;
    let path = PathBuf::from(&raw);
    if path.is_file() {
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mjs")) {
            let working_dir = path
                .parent()
                .and_then(|parent| parent.parent())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.parent().unwrap_or(&path).to_path_buf());
            if let Some((program, prefix_args)) =
                crate::tool::builtins::codegraph::resolve_codegraph_node_for_script(&path)
            {
                return Some(ResolvedAgentmemory {
                    program,
                    prefix_args,
                    working_dir: working_dir.clone(),
                    iii_bin_dir: working_dir.clone(),
                    bundle_root: working_dir,
                    using_bundled_runtime: false,
                    bundle_version: None,
                });
            }
        }
        return resolve_agentmemory_from_cli_entry(&path, path.parent()?.to_path_buf(), false, None);
    }
    if path.is_dir() {
        return resolve_agentmemory_from_bundle_root(&path);
    }
    None
}

fn resolve_agentmemory_from_path() -> Option<ResolvedAgentmemory> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in agentmemory_path_binary_names() {
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            #[cfg(windows)]
            if candidate.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cmd")) {
                return Some(ResolvedAgentmemory {
                    program: PathBuf::from(
                        std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string()),
                    ),
                    prefix_args: vec!["/C".to_string(), cli_arg_path(&candidate)],
                    working_dir: dir.clone(),
                    iii_bin_dir: dir.clone(),
                    bundle_root: dir,
                    using_bundled_runtime: false,
                    bundle_version: None,
                });
            }
            return Some(ResolvedAgentmemory {
                program: candidate.clone(),
                prefix_args: Vec::new(),
                working_dir: dir.clone(),
                iii_bin_dir: dir.clone(),
                bundle_root: dir,
                using_bundled_runtime: false,
                bundle_version: None,
            });
        }
    }
    None
}

fn resolve_agentmemory_from_bundle() -> Option<ResolvedAgentmemory> {
    for root in agentmemory_bundle_roots() {
        if let Some(resolved) = resolve_agentmemory_from_bundle_root(&root) {
            return Some(resolved);
        }
    }
    None
}

fn resolve_agentmemory_from_bundle_root(root: &Path) -> Option<ResolvedAgentmemory> {
    let package_root = root.join("node_modules").join("@agentmemory").join("agentmemory");
    let cli_entry = package_root.join("dist").join("cli.mjs");
    if !cli_entry.is_file() {
        return None;
    }
    let iii_bin_dir = root.join("bin");
    let iii_program = {
        #[cfg(windows)]
        {
            iii_bin_dir.join("iii.exe")
        }
        #[cfg(not(windows))]
        {
            iii_bin_dir.join("iii")
        }
    };
    if !iii_program.is_file() {
        return None;
    }

    let bundle_version = read_bundle_version_from_root(root);
    resolve_agentmemory_from_cli_entry(
        &cli_entry,
        package_root,
        true,
        bundle_version.clone(),
    )
    .map(|mut resolved| {
        resolved.iii_bin_dir = iii_bin_dir;
        resolved.bundle_root = root.to_path_buf();
        resolved.using_bundled_runtime = true;
        resolved.bundle_version = bundle_version;
        resolved
    })
}

fn resolve_agentmemory_from_cli_entry(
    cli_entry: &Path,
    working_dir: PathBuf,
    using_bundled_runtime: bool,
    bundle_version: Option<String>,
) -> Option<ResolvedAgentmemory> {
    if using_bundled_runtime {
        let (program, prefix_args) =
            crate::tool::builtins::codegraph::resolve_codegraph_node_for_script(cli_entry)?;
        return Some(ResolvedAgentmemory {
            program,
            prefix_args,
            working_dir: working_dir.clone(),
            iii_bin_dir: working_dir.clone(),
            bundle_root: working_dir,
            using_bundled_runtime: true,
            bundle_version,
        });
    }

    Some(ResolvedAgentmemory {
        program: cli_entry.to_path_buf(),
        prefix_args: Vec::new(),
        working_dir,
        iii_bin_dir: cli_entry
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cli_entry.to_path_buf()),
        bundle_root: cli_entry
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cli_entry.to_path_buf()),
        using_bundled_runtime: false,
        bundle_version,
    })
}

fn agentmemory_bundle_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(registered) = managed_agentmemory_resource_dirs().lock() {
        for root in registered.iter() {
            push_unique_bundle_root(&mut roots, root);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_unique_bundle_root(
                &mut roots,
                &exe_dir.join("resources").join("agentmemory-bundle"),
            );
            push_unique_bundle_root(&mut roots, &exe_dir.join("agentmemory-bundle"));
        }
    }

    push_unique_bundle_root(
        &mut roots,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("agentmemory-bundle"),
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

fn managed_agentmemory_resource_dirs() -> &'static ManagedAgentmemoryDirs {
    static DIRS: OnceLock<ManagedAgentmemoryDirs> = OnceLock::new();
    DIRS.get_or_init(|| Mutex::new(Vec::new()))
}

fn agentmemory_path_binary_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["agentmemory.exe", "agentmemory.cmd", "agentmemory"]
    }
    #[cfg(not(windows))]
    {
        &["agentmemory"]
    }
}

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
pub(crate) fn resolve_agentmemory_from_bundle_root_for_test(
    root: &Path,
) -> Option<ResolvedAgentmemory> {
    resolve_agentmemory_from_bundle_root(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_agentmemory_requires_cli_and_iii_in_bundle_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let package_root = root
            .join("node_modules")
            .join("@agentmemory")
            .join("agentmemory");
        fs::create_dir_all(package_root.join("dist")).expect("package dirs");
        fs::write(package_root.join("dist").join("cli.mjs"), "// stub").expect("cli");

        assert!(resolve_agentmemory_from_bundle_root(root).is_none());

        fs::create_dir_all(root.join("bin")).expect("bin dir");
        #[cfg(windows)]
        fs::write(root.join("bin").join("iii.exe"), "stub").expect("iii");
        #[cfg(not(windows))]
        fs::write(root.join("bin").join("iii"), "stub").expect("iii");

        let resolved = resolve_agentmemory_from_bundle_root(root);
        if crate::tool::builtins::codegraph::resolve_codegraph_node_for_script(
            &package_root.join("dist").join("cli.mjs"),
        )
        .is_some()
        {
            assert!(resolved.is_some());
            assert!(resolved.expect("resolved").using_bundled_runtime);
        } else {
            assert!(resolved.is_none());
        }
    }

    #[test]
    fn resolve_agentmemory_from_repo_bundle_when_prepared() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("agentmemory-bundle");
        #[cfg(windows)]
        let iii = root.join("bin").join("iii.exe");
        #[cfg(not(windows))]
        let iii = root.join("bin").join("iii");
        if !iii.is_file() {
            return;
        }
        let resolved = resolve_agentmemory_from_bundle_root(&root);
        assert!(resolved.is_some(), "expected prepared agentmemory bundle to resolve");
        let resolved = resolved.expect("resolved");
        assert!(resolved.using_bundled_runtime);
        assert_eq!(resolved.bundle_version.as_deref(), Some("0.9.24"));
    }

    #[test]
    fn bundled_spawn_args_avoid_windows_backslash_escapes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gen")
            .join("agentmemory-bundle");
        #[cfg(windows)]
        let iii = root.join("bin").join("iii.exe");
        #[cfg(not(windows))]
        let iii = root.join("bin").join("iii");
        if !iii.is_file() {
            return;
        }
        let resolved = resolve_agentmemory_from_bundle_root(&root).expect("resolved");
        for arg in &resolved.prefix_args {
            if arg.ends_with(".mjs") && arg.contains('\\') {
                panic!("cli entry still contains backslashes on Windows: {arg}");
            }
        }
    }

    #[test]
    fn read_bundle_version_prefers_manifest() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::write(root.join("version.txt"), "0.9.24\n").expect("version");
        fs::write(
            root.join("manifest.json"),
            r#"{"agentmemoryVersion":"0.9.24"}"#,
        )
        .expect("manifest");

        assert_eq!(
            read_bundle_version_from_root(root).as_deref(),
            Some("0.9.24")
        );
    }
}
