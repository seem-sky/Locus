use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use super::client::{AgentMemoryClient, AgentMemoryHealthStatus};
use super::llm_env;
use super::resolve::{self, ResolvedAgentmemory};

const STARTUP_POLL_MS: u64 = 400;
const STARTUP_MAX_ATTEMPTS: u32 = 60;

pub struct AgentMemoryService {
    child: Mutex<Option<Child>>,
    autostart: bool,
    export_root: Mutex<Option<PathBuf>>,
    last_runtime: Mutex<Option<RuntimeSnapshot>>,
}

#[derive(Debug, Clone)]
struct RuntimeSnapshot {
    bundle_version: Option<String>,
    using_bundled_runtime: bool,
}

impl AgentMemoryService {
    pub fn from_env() -> Self {
        let autostart = match std::env::var("LOCUS_AGENTMEMORY_AUTOSTART")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("0") | Some("false") | Some("no") => false,
            _ => true,
        };
        Self {
            child: Mutex::new(None),
            autostart,
            export_root: Mutex::new(None),
            last_runtime: Mutex::new(None),
        }
    }

    pub fn set_export_root(&self, path: PathBuf) {
        if let Ok(mut guard) = self.export_root.lock() {
            *guard = Some(path);
        }
    }

    pub fn autostart_enabled(&self) -> bool {
        self.autostart
    }

    pub fn bundle_version(&self) -> Option<String> {
        self.last_runtime
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().and_then(|snapshot| snapshot.bundle_version.clone()))
    }

    pub fn using_bundled_runtime(&self) -> bool {
        self.last_runtime
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|snapshot| snapshot.using_bundled_runtime))
            .unwrap_or(false)
    }

    pub fn health(&self, client: &AgentMemoryClient) -> AgentMemoryHealthStatus {
        client.health()
    }

    pub fn ensure_running(&self, client: &AgentMemoryClient) -> Result<(), String> {
        let health = client.health();
        if self.locus_managed_process_alive() && health.available && health.worker_count <= 1 {
            return Ok(());
        }
        if self.locus_managed_process_alive() && health.available && health.worker_count > 1 {
            eprintln!(
                "[Locus] agentmemory: {} workers connected (expected 1), restarting sidecar",
                health.worker_count
            );
        }
        if !self.autostart {
            return Err(format!(
                "agentmemory server is not running. {}",
                resolve::AGENTMEMORY_BUNDLE_HINT
            ));
        }
        self.start(client)?;
        self.wait_for_health(client)
    }

    pub fn start_and_wait(&self, client: &AgentMemoryClient) -> Result<(), String> {
        self.start(client)?;
        self.wait_for_health(client)
    }

    /// True when this service spawned a child that is still running.
    fn locus_managed_process_alive(&self) -> bool {
        let Ok(mut guard) = self.child.lock() else {
            return false;
        };
        let Some(child) = guard.as_mut() else {
            return false;
        };
        matches!(child.try_wait(), Ok(None))
    }

    pub fn start(&self, client: &AgentMemoryClient) -> Result<(), String> {
        self.reap_exited_child();
        let health = client.health();
        if health.available && self.locus_managed_process_alive() && health.worker_count <= 1 {
            return Ok(());
        }

        self.stop()?;
        let resolved = resolve::resolve_agentmemory()?;
        if client.health().available {
            reclaim_listener_port(client);
        } else {
            cleanup_stale_listener(client);
        }
        if resolved.using_bundled_runtime {
            reclaim_all_agentmemory_workers();
            reclaim_foreign_agentmemory_workers(&resolved.bundle_root);
        }

        let mut guard = self.child.lock().map_err(|e| e.to_string())?;

        self.remember_runtime(&resolved);

        let export_root = self
            .export_root
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let (program, prefix_args, spawn_cwd) = windows_spawn_command(&resolved);
        #[cfg(windows)]
        let program = crate::process_util::windows_command_path(Path::new(&program));
        #[cfg(not(windows))]
        let program = program.to_string_lossy().into_owned();
        let mut cmd = std::process::Command::new(&program);
        crate::process_util::suppress_command_window(&mut cmd);
        crate::network::apply_proxy_env_to_command(&mut cmd);
        crate::process_util::set_new_process_group(&mut cmd);
        if let Some(path) = prepend_path_env(&resolved.iii_bin_dir) {
            cmd.env("PATH", path);
        }
        cmd.args(prefix_args)
            .current_dir(&spawn_cwd)
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if let Some(export_root) = export_root {
            std::fs::create_dir_all(&export_root).map_err(|e| e.to_string())?;
            if let Some(log_path) = service_log_path(&export_root) {
                if let Ok(log_file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    cmd.stderr(Stdio::from(log_file));
                }
                cmd.env("AGENTMEMORY_SERVICE_LOG", log_path.display().to_string());
            }
            cmd.env(
                "AGENTMEMORY_EXPORT_ROOT",
                export_root.display().to_string(),
            );
        }

        let llm_env = llm_env::resolve_for_agentmemory();
        if llm_env.configured {
            eprintln!(
                "[Locus] agentmemory LLM bridge: provider={}",
                llm_env.provider_label
            );
        } else if let Some(warning) = &llm_env.warning {
            eprintln!("[Locus] agentmemory LLM bridge: {}", warning);
        }
        for (key, value) in llm_env.vars {
            cmd.env(key, value);
        }

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start agentmemory: {}", e))?;
        *guard = Some(child);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = guard.take() {
            crate::process_util::kill_process_tree(&mut child);
        }
        Ok(())
    }

    fn remember_runtime(&self, resolved: &ResolvedAgentmemory) {
        if let Ok(mut guard) = self.last_runtime.lock() {
            *guard = Some(RuntimeSnapshot {
                bundle_version: resolved.bundle_version.clone(),
                using_bundled_runtime: resolved.using_bundled_runtime,
            });
        }
    }

    fn wait_for_health(&self, client: &AgentMemoryClient) -> Result<(), String> {
        for _ in 0..STARTUP_MAX_ATTEMPTS {
            if client.health().available {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(STARTUP_POLL_MS));
        }
        Err(self.start_failure_message())
    }

    fn start_failure_message(&self) -> String {
        let log_hint = self
            .export_root
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .map(|export_root| export_root.join("service.log"))
            .filter(|path| path.is_file())
            .map(|path| format!(" See log: {}", path.display()))
            .unwrap_or_default();
        format!(
            "agentmemory server failed to become healthy after start.{log_hint} {}",
            resolve::AGENTMEMORY_BUNDLE_HINT
        )
    }

    fn reap_exited_child(&self) {
        let Ok(mut guard) = self.child.lock() else {
            return;
        };
        let Some(child) = guard.as_mut() else {
            return;
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            *guard = None;
        }
    }
}

impl Drop for AgentMemoryService {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn prepend_path_env(bin_dir: &Path) -> Option<std::ffi::OsString> {
    let mut paths = vec![bin_dir.to_path_buf()];
    if let Ok(existing) = std::env::var("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).ok()
}

/// Spawn bundled codegraph Node with `--liftoff-only` and a forward-slash CLI path.
/// `prefix_args` is built in resolve.rs; `iii` is discovered via PATH prepended in `start()`.
fn windows_spawn_command(resolved: &ResolvedAgentmemory) -> (PathBuf, Vec<String>, PathBuf) {
    (
        resolved.program.clone(),
        resolved.prefix_args.clone(),
        resolved.working_dir.clone(),
    )
}

fn service_log_path(export_root: &Path) -> Option<PathBuf> {
    Some(export_root.join("service.log"))
}

fn cleanup_stale_listener(client: &AgentMemoryClient) {
    let health = client.health();
    if health.available || !health.orphaned_listener {
        return;
    }
    reclaim_listener_port(client);
}

/// Stop all agentmemory worker processes so a fresh bundled runtime is the sole handler.
fn reclaim_all_agentmemory_workers() {
    for pid in agentmemory_worker_pids() {
        crate::process_util::kill_pid_tree(pid);
    }
}

/// Stop foreign agentmemory worker processes (e.g. global npm installs) so the bundled
/// runtime is the sole handler on the shared iii engine.
fn reclaim_foreign_agentmemory_workers(bundle_root: &Path) {
    let bundle_cli = bundle_root
        .join("node_modules")
        .join("@agentmemory")
        .join("agentmemory")
        .join("dist")
        .join("cli.mjs");
    let bundle_cli = dunce::canonicalize(&bundle_cli).unwrap_or(bundle_cli);
    let bundle_marker = bundle_cli
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();

    for pid in agentmemory_worker_pids() {
        let cmdline = process_command_line(pid).unwrap_or_default();
        let normalized = cmdline.replace('\\', "/").to_ascii_lowercase();
        if !normalized.contains("@agentmemory/agentmemory") {
            continue;
        }
        if normalized.contains(&bundle_marker) {
            continue;
        }
        crate::process_util::kill_pid_tree(pid);
    }
}

fn agentmemory_worker_pids() -> Vec<u32> {
    #[cfg(windows)]
    {
        let script = r#"$ErrorActionPreference = 'SilentlyContinue'; Get-CimInstance Win32_Process -Filter "Name = 'node.exe'" | Where-Object { $_.CommandLine -match '@agentmemory/agentmemory' } | ForEach-Object { $_.ProcessId }"#;
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::process_util::suppress_command_window(&mut cmd);
        let output = match cmd.output() {
            Ok(value) => value,
            Err(_) => return Vec::new(),
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .collect()
    }

    #[cfg(unix)]
    {
        let mut cmd = std::process::Command::new("pgrep");
        cmd.args(["-f", "@agentmemory/agentmemory"]);
        crate::process_util::suppress_command_window(&mut cmd);
        let output = match cmd.output() {
            Ok(value) => value,
            Err(_) => return Vec::new(),
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .collect()
    }
}

fn process_command_line(pid: u32) -> Option<String> {
    #[cfg(windows)]
    {
        let script = format!(
            r#"$ErrorActionPreference = 'SilentlyContinue'; (Get-CimInstance Win32_Process -Filter "ProcessId = {pid}").CommandLine"#
        );
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", &script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::process_util::suppress_command_window(&mut cmd);
        let output = cmd.output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    }

    #[cfg(unix)]
    {
        let mut cmd = std::process::Command::new("ps");
        cmd.args(["-p", &pid.to_string(), "-o", "args="]);
        let output = cmd.output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    }
}

/// Stop any agentmemory listener on the REST port so Locus can spawn a managed child with fresh env.
fn reclaim_listener_port(client: &AgentMemoryClient) {
    if let Some(port) = rest_port_from_base_url(client.base_url()) {
        if let Some(pid) = find_listening_pid(port) {
            if is_agentmemory_listener_pid(pid) {
                crate::process_util::kill_pid_tree(pid);
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

fn rest_port_from_base_url(base_url: &str) -> Option<u16> {
    reqwest::Url::parse(base_url)
        .ok()
        .and_then(|url| url.port_or_known_default())
}

fn find_listening_pid(port: u16) -> Option<u32> {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("netstat");
        cmd.args(["-ano"]).stdout(Stdio::piped()).stderr(Stdio::null());
        crate::process_util::suppress_command_window(&mut cmd);
        let output = cmd.output().ok()?;
        let needle = format!(":{port}");
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if !line.contains("LISTENING") || !line.contains(&needle) {
                continue;
            }
            return line.split_whitespace().last()?.parse().ok();
        }
        None
    }

    #[cfg(unix)]
    {
        let mut cmd = std::process::Command::new("lsof");
        cmd.args([
            "-nP",
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
            "-t",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
        let output = cmd.output().ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .and_then(|line| line.trim().parse().ok())
    }
}

fn is_agentmemory_listener_pid(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("tasklist");
        cmd.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::process_util::suppress_command_window(&mut cmd);
        let Some(output) = cmd.output().ok() else {
            return false;
        };
        let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        text.contains("iii.exe") || text.contains("node.exe")
    }

    #[cfg(unix)]
    {
        let mut cmd = std::process::Command::new("ps");
        cmd.args(["-p", &pid.to_string(), "-o", "comm="])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let Some(output) = cmd.output().ok() else {
            return false;
        };
        let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        text.contains("iii") || text.contains("node")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepend_path_env_keeps_existing_entries() {
        std::env::set_var("PATH", "/usr/bin");
        let joined = prepend_path_env(Path::new("/opt/iii/bin")).expect("join paths");
        let joined = joined.to_string_lossy();
        assert!(joined.contains("/opt/iii/bin"));
        assert!(joined.contains("/usr/bin"));
    }

    #[test]
    fn reap_exited_child_clears_exited_process() {
        let service = AgentMemoryService::from_env();
        {
            let mut guard = service.child.lock().expect("lock");
            let mut cmd = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" });
            if cfg!(windows) {
                cmd.args(["/C", "exit 0"]);
            }
            *guard = Some(cmd.spawn().expect("spawn"));
            guard.as_mut().unwrap().wait().expect("wait");
        }
        service.reap_exited_child();
        assert!(service.child.lock().expect("lock").is_none());
    }

    #[test]
    fn resolve_agentmemory_spawn_args_use_forward_slashes() {
        let resolved = match resolve::resolve_agentmemory() {
            Ok(value) => value,
            Err(error) => {
                eprintln!("resolve_agentmemory unavailable in test env: {error}");
                return;
            }
        };
        let program = crate::process_util::windows_command_path(&resolved.program);
        assert!(
            !program.contains('\\'),
            "program path must use forward slashes on Windows: {program}"
        );
        for arg in &resolved.prefix_args {
            if arg.contains('\\') {
                panic!("spawn arg contains backslashes: {arg:?}");
            }
        }
    }

    #[test]
    fn bundled_agentmemory_spawn_survives_startup() {
        use std::path::PathBuf;

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

        let resolved = resolve::resolve_agentmemory_from_bundle_root_for_test(&root)
            .expect("expected prepared agentmemory bundle to resolve");

        let temp = tempfile::tempdir().expect("tempdir");
        let log_path = temp.path().join("stderr.log");
        let log_file = std::fs::File::create(&log_path).expect("log file");

        let (program, prefix_args, spawn_cwd) = windows_spawn_command(&resolved);

        #[cfg(windows)]
        let program = crate::process_util::windows_command_path(Path::new(&program));
        #[cfg(not(windows))]
        let program = program.to_string_lossy().into_owned();
        let mut cmd = std::process::Command::new(&program);
        crate::process_util::suppress_command_window(&mut cmd);
        crate::process_util::set_new_process_group(&mut cmd);
        cmd.args(prefix_args.clone())
            .current_dir(&spawn_cwd)
            .stdout(Stdio::null())
            .stderr(Stdio::from(log_file));
        if let Some(path) = prepend_path_env(&resolved.iii_bin_dir) {
            cmd.env("PATH", path);
        }

        let mut child = cmd.spawn().expect("spawn bundled agentmemory");
        std::thread::sleep(Duration::from_millis(2500));
        let exited = child.try_wait().expect("try_wait");
        let stderr = std::fs::read_to_string(&log_path).unwrap_or_default();
        if let Some(status) = exited {
            panic!(
                "agentmemory child exited early with {status}; program={program:?} args={prefix_args:?} stderr={stderr}",
            );
        }
        let _ = child.kill();
        let _ = child.wait();
        assert!(
            !stderr.contains("lstat 'G:'"),
            "node argv corruption detected; args={prefix_args:?} stderr={stderr}",
        );
    }
}
