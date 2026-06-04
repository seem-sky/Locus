use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::tool::builtins::codegraph;
use crate::workspace::Workspace;

const WATCHER_BATCH_WINDOW_MS: u64 = 2_000;
const WATCHER_IDLE_POLL_MS: u64 = 250;
const PERIODIC_SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);

const IGNORED_DIR_NAMES: &[&str] = &[
    ".git",
    ".codegraph",
    ".cursor",
    ".cache",
    ".rtk",
    "node_modules",
    "target",
    "library",
    "temp",
    "logs",
    "obj",
    "bin",
    "dist",
    "build",
    "coverage",
    "vendor",
    "__pycache__",
];

const IGNORED_FILE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "pdb", "db", "db-shm", "db-wal", "png", "jpg", "jpeg", "gif", "webp", "ico",
    "zip", "tar", "gz", "7z", "mp3", "mp4", "wav", "pdf", "woff", "woff2", "ttf", "otf",
];

pub struct CodegraphWatcher {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    _os_watcher: RecommendedWatcher,
}

impl CodegraphWatcher {
    pub fn start(working_dir: String, workspace: Arc<Workspace>) -> Result<Self, String> {
        let root = PathBuf::from(&working_dir);
        if !root.is_dir() {
            return Err(format!("Working directory not found: {}", working_dir));
        }

        let (tx, rx) = mpsc::channel();
        let mut os_watcher = RecommendedWatcher::new(tx, Config::default())
            .map_err(|error| format!("Failed to create CodeGraph watcher: {}", error))?;
        os_watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| {
                format!(
                    "Failed to watch workspace for CodeGraph sync: {}",
                    error
                )
            })?;

        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_root = root.clone();
        let worker = thread::Builder::new()
            .name("codegraph-fs-watcher".to_string())
            .spawn(move || {
                watcher_loop(
                    rx,
                    worker_stop,
                    worker_root,
                    workspace,
                );
            })
            .map_err(|error| format!("Failed to spawn CodeGraph watcher thread: {}", error))?;

        eprintln!(
            "[CodeGraphWatcher] started for {} (debounce={}ms, periodic={}s)",
            working_dir,
            WATCHER_BATCH_WINDOW_MS,
            PERIODIC_SYNC_INTERVAL.as_secs()
        );

        Ok(Self {
            stop,
            worker: Some(worker),
            _os_watcher: os_watcher,
        })
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn watcher_loop(
    rx: mpsc::Receiver<notify::Result<Event>>,
    stop: Arc<AtomicBool>,
    workspace_root: PathBuf,
    workspace: Arc<Workspace>,
) {
    let generation = workspace.generation();
    let mut dirty = false;
    let mut last_periodic_sync = Instant::now();
    let sync_in_progress = Arc::new(AtomicBool::new(false));
    let sync_pending = Arc::new(AtomicBool::new(false));

    spawn_sync(
        workspace_root.clone(),
        workspace.clone(),
        generation,
        sync_in_progress.clone(),
        sync_pending.clone(),
        "startup",
    );

    while !stop.load(Ordering::Relaxed) {
        if workspace.generation() != generation {
            eprintln!("[CodeGraphWatcher] stopping (workspace generation changed)");
            break;
        }

        if last_periodic_sync.elapsed() >= PERIODIC_SYNC_INTERVAL
            && !sync_in_progress.load(Ordering::Relaxed)
        {
            spawn_sync(
                workspace_root.clone(),
                workspace.clone(),
                generation,
                sync_in_progress.clone(),
                sync_pending.clone(),
                "periodic",
            );
            last_periodic_sync = Instant::now();
        }

        let first = match rx.recv_timeout(Duration::from_millis(WATCHER_IDLE_POLL_MS)) {
            Ok(event) => Some(event),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if let Some(first) = first {
            let mut batch = vec![first];
            let deadline = Instant::now() + Duration::from_millis(WATCHER_BATCH_WINDOW_MS);
            while Instant::now() < deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match rx.recv_timeout(remaining) {
                    Ok(event) => batch.push(event),
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }

            if batch_has_relevant_changes(&batch, &workspace_root) {
                dirty = true;
            }
        }

        if !dirty {
            continue;
        }

        if sync_in_progress.load(Ordering::Relaxed) {
            sync_pending.store(true, Ordering::Relaxed);
            continue;
        }

        dirty = false;
        spawn_sync(
            workspace_root.clone(),
            workspace.clone(),
            generation,
            sync_in_progress.clone(),
            sync_pending.clone(),
            "filesystem",
        );
    }
}

fn spawn_sync(
    workspace_root: PathBuf,
    workspace: Arc<Workspace>,
    generation: u64,
    sync_in_progress: Arc<AtomicBool>,
    sync_pending: Arc<AtomicBool>,
    reason: &'static str,
) {
    sync_in_progress.store(true, Ordering::Relaxed);
    let sync_in_progress_for_task = sync_in_progress.clone();
    let sync_pending_for_task = sync_pending.clone();
    tauri::async_runtime::spawn(async move {
        let started_at = Instant::now();
        let result = codegraph::sync_project_index(&workspace_root).await;
        sync_in_progress_for_task.store(false, Ordering::Relaxed);

        if workspace.generation() != generation {
            return;
        }

        match result {
            Ok(()) => {
                tracing::info!(
                    log_module = "CodeGraphWatcher",
                    "sync complete reason={} elapsed_ms={}",
                    reason,
                    started_at.elapsed().as_millis()
                );
            }
            Err(error) => {
                eprintln!(
                    "[CodeGraphWatcher] sync failed (reason={}): {}",
                    reason, error
                );
            }
        }

        if sync_pending_for_task.swap(false, Ordering::Relaxed) && workspace.generation() == generation
        {
            spawn_sync(
                workspace_root,
                workspace,
                generation,
                sync_in_progress,
                sync_pending,
                "coalesced",
            );
        }
    });
}

fn batch_has_relevant_changes(batch: &[notify::Result<Event>], workspace_root: &Path) -> bool {
    for entry in batch {
        let event = match entry {
            Ok(event) => event,
            Err(error) => {
                eprintln!("[CodeGraphWatcher] notify error: {}", error);
                continue;
            }
        };
        if !is_relevant_event_kind(&event.kind) {
            continue;
        }
        for path in &event.paths {
            if should_ignore_watch_path(path, workspace_root) {
                continue;
            }
            return true;
        }
    }
    false
}

fn is_relevant_event_kind(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(_) => false,
        EventKind::Create(CreateKind::Any)
        | EventKind::Create(CreateKind::File)
        | EventKind::Create(CreateKind::Folder)
        | EventKind::Create(CreateKind::Other)
        | EventKind::Remove(RemoveKind::Any)
        | EventKind::Remove(RemoveKind::File)
        | EventKind::Remove(RemoveKind::Folder)
        | EventKind::Remove(RemoveKind::Other)
        | EventKind::Modify(ModifyKind::Any)
        | EventKind::Modify(ModifyKind::Data(_))
        | EventKind::Modify(ModifyKind::Metadata(_))
        | EventKind::Modify(ModifyKind::Name(_)) => true,
        _ => false,
    }
}

fn should_ignore_watch_path(path: &Path, workspace_root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(workspace_root) else {
        return true;
    };

    for component in relative.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        if IGNORED_DIR_NAMES
            .iter()
            .any(|ignored| name.eq_ignore_ascii_case(ignored))
        {
            return true;
        }
    }

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IGNORED_FILE_EXTENSIONS
                .iter()
                .any(|ignored| ext.eq_ignore_ascii_case(ignored))
        })
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{should_ignore_watch_path, IGNORED_DIR_NAMES};

    #[test]
    fn ignores_codegraph_and_vendor_directories() {
        let root = PathBuf::from(r"G:\AI\Locus");
        assert!(should_ignore_watch_path(
            &root.join(".codegraph/codegraph.db"),
            &root
        ));
        assert!(should_ignore_watch_path(
            &root.join("node_modules/pkg/index.ts"),
            &root
        ));
        assert!(!should_ignore_watch_path(
            &root.join("src-tauri/src/lib.rs"),
            &root
        ));
    }

    #[test]
    fn ignored_dir_names_are_lowercase_ascii() {
        for name in IGNORED_DIR_NAMES {
            assert!(
                name.chars()
                    .all(|ch| ch.is_ascii_lowercase() || matches!(ch, '.' | '_')),
                "ignored dir name should be stable: {name}"
            );
        }
    }
}
