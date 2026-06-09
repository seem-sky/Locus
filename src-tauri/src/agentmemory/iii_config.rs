use std::path::{Path, PathBuf};

const III_CONFIG_FILENAME: &str = "iii-config.yaml";
const ENV_FILENAME: &str = ".env";
const LEGACY_GLOBAL_DIR_NAME: &str = ".agentmemory";

/// Default global agentmemory home (`~/.agentmemory`) used before Locus isolation.
pub fn legacy_global_data_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(LEGACY_GLOBAL_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(LEGACY_GLOBAL_DIR_NAME))
}

/// Write `iii-config.yaml` under Locus export root so iii KV stores live beside
/// `service.log` / Obsidian exports instead of inside the bundled package tree.
pub fn write_iii_config(export_root: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(export_root).map_err(|e| {
        format!(
            "Failed to create agentmemory export dir '{}': {}",
            export_root.display(),
            e
        )
    })?;

    let state_path = yaml_path(&export_root.join("state_store.db"));
    let stream_path = yaml_path(&export_root.join("stream_store"));

    let content = format!(
        r#"workers:
  - name: iii-http
    config:
      port: 3111
      host: 127.0.0.1
      default_timeout: 180000
      cors:
        allowed_origins: ["http://localhost:3111", "http://localhost:3113", "http://127.0.0.1:3111", "http://127.0.0.1:3113"]
        allowed_methods: [GET, POST, PUT, DELETE, OPTIONS]
  - name: iii-state
    config:
      adapter:
        name: kv
        config:
          store_method: file_based
          file_path: {state_path}
  - name: iii-queue
    config:
      adapter:
        name: builtin
  - name: iii-pubsub
    config:
      adapter:
        name: local
  - name: iii-cron
    config:
      adapter:
        name: kv
  - name: iii-stream
    config:
      port: 3112
      host: 127.0.0.1
      adapter:
        name: kv
        config:
          store_method: file_based
          file_path: {stream_path}
  - name: iii-observability
    config:
      enabled: true
      service_name: agentmemory
      exporter: memory
      sampling_ratio: 0.1
      metrics_enabled: true
      logs_enabled: true
      logs_console_output: false
  - name: iii-exec
    config:
      watch:
        - src/**/*.ts
      exec:
        - node dist/index.mjs
"#
    );

    let config_path = export_root.join(III_CONFIG_FILENAME);
    std::fs::write(&config_path, content).map_err(|e| {
        format!(
            "Failed to write agentmemory iii config '{}': {}",
            config_path.display(),
            e
        )
    })?;
    Ok(config_path)
}

/// Copy `.env` from `~/.agentmemory` when Locus export root does not have one yet.
pub fn maybe_migrate_legacy_env(export_root: &Path, legacy_data_dir: &Path) -> Result<(), String> {
    let target = export_root.join(ENV_FILENAME);
    if target.is_file() {
        return Ok(());
    }
    let source = legacy_data_dir.join(ENV_FILENAME);
    if !source.is_file() {
        return Ok(());
    }
    std::fs::copy(&source, &target).map_err(|e| {
        format!(
            "Failed to migrate agentmemory .env '{}' -> '{}': {}",
            source.display(),
            target.display(),
            e
        )
    })?;
    eprintln!(
        "[Locus] agentmemory: migrated legacy .env '{}' -> '{}'",
        source.display(),
        target.display()
    );
    Ok(())
}

/// Copy KV data from the bundled package `data/` tree when Locus export root is still empty.
pub fn maybe_migrate_legacy_kv(export_root: &Path, legacy_data_dir: &Path) -> Result<(), String> {
    let targets = [
        ("state_store.db", export_root.join("state_store.db")),
        ("stream_store", export_root.join("stream_store")),
    ];

    for (name, target) in targets {
        if dir_has_entries(&target) {
            continue;
        }
        let source = legacy_data_dir.join(name);
        if !dir_has_entries(&source) {
            continue;
        }
        copy_dir_recursive(&source, &target)?;
        eprintln!(
            "[Locus] agentmemory: migrated legacy KV '{}' -> '{}'",
            source.display(),
            target.display()
        );
    }
    Ok(())
}

fn yaml_path(path: &Path) -> String {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"");
    format!("\"{normalized}\"")
}

fn dir_has_entries(path: &Path) -> bool {
    path.is_dir()
        && std::fs::read_dir(path)
            .ok()
            .is_some_and(|mut entries| entries.next().is_some())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("Source is not a directory: {}", src.display()));
    }
    std::fs::create_dir_all(dst).map_err(|e| {
        format!(
            "Failed to create directory '{}': {}",
            dst.display(),
            e
        )
    })?;

    for entry in std::fs::read_dir(src).map_err(|e| {
        format!("Failed to read directory '{}': {}", src.display(), e)
    })? {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let file_type = entry.file_type().map_err(|e| {
            format!(
                "Failed to read file type for '{}': {}",
                entry.path().display(),
                e
            )
        })?;
        let target_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target_path)?;
        } else {
            std::fs::copy(entry.path(), &target_path).map_err(|e| {
                format!(
                    "Failed to copy '{}' -> '{}': {}",
                    entry.path().display(),
                    target_path.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_iii_config_uses_export_root_kv_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let export_root = temp.path().join("agentmemory");
        let config_path = write_iii_config(&export_root).expect("write config");
        assert!(config_path.is_file());

        let raw = std::fs::read_to_string(&config_path).expect("read config");
        let state = export_root.join("state_store.db");
        let stream = export_root.join("stream_store");
        assert!(raw.contains(&yaml_path(&state)));
        assert!(raw.contains(&yaml_path(&stream)));
        assert!(raw.contains("port: 3111"));
    }

    #[test]
    fn maybe_migrate_legacy_kv_skips_when_export_root_already_has_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let export_root = temp.path().join("export");
        let legacy = temp.path().join("legacy").join("data");

        std::fs::create_dir_all(legacy.join("state_store.db")).expect("legacy state");
        std::fs::write(legacy.join("state_store.db").join("mem.bin"), b"old").expect("legacy bin");

        std::fs::create_dir_all(export_root.join("state_store.db")).expect("export state");
        std::fs::write(
            export_root.join("state_store.db").join("mem.bin"),
            b"new",
        )
        .expect("export bin");

        maybe_migrate_legacy_kv(&export_root, &legacy).expect("migrate");
        let kept = std::fs::read(export_root.join("state_store.db").join("mem.bin")).expect("read");
        assert_eq!(kept, b"new");
    }

    #[test]
    fn maybe_migrate_legacy_env_copies_from_global_home() {
        let temp = tempfile::tempdir().expect("tempdir");
        let export_root = temp.path().join("export");
        let legacy = temp.path().join("legacy");
        std::fs::create_dir_all(&legacy).expect("legacy dir");
        std::fs::write(legacy.join(".env"), "OPENROUTER_API_KEY=from-legacy\n").expect("legacy env");

        maybe_migrate_legacy_env(&export_root, &legacy).expect("migrate env");
        let copied = std::fs::read_to_string(export_root.join(".env")).expect("copied env");
        assert!(copied.contains("from-legacy"));
    }

    #[test]
    fn maybe_migrate_legacy_env_skips_when_export_root_has_env() {
        let temp = tempfile::tempdir().expect("tempdir");
        let export_root = temp.path().join("export");
        let legacy = temp.path().join("legacy");
        std::fs::create_dir_all(&export_root).expect("export dir");
        std::fs::create_dir_all(&legacy).expect("legacy dir");
        std::fs::write(export_root.join(".env"), "KEEP=1\n").expect("export env");
        std::fs::write(legacy.join(".env"), "DROP=1\n").expect("legacy env");

        maybe_migrate_legacy_env(&export_root, &legacy).expect("migrate env");
        let kept = std::fs::read_to_string(export_root.join(".env")).expect("kept env");
        assert!(kept.contains("KEEP=1"));
        assert!(!kept.contains("DROP=1"));
    }

    #[test]
    fn maybe_migrate_legacy_kv_copies_from_bundle_data_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let export_root = temp.path().join("export");
        let legacy = temp.path().join("legacy").join("data");

        std::fs::create_dir_all(legacy.join("state_store.db")).expect("legacy state");
        std::fs::write(
            legacy.join("state_store.db").join("mem%3Amemories.bin"),
            b"legacy",
        )
        .expect("legacy bin");

        maybe_migrate_legacy_kv(&export_root, &legacy).expect("migrate");
        let copied = std::fs::read(
            export_root
                .join("state_store.db")
                .join("mem%3Amemories.bin"),
        )
        .expect("copied");
        assert_eq!(copied, b"legacy");
    }
}
