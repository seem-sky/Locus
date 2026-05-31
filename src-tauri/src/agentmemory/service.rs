use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use super::client::{AgentMemoryClient, AgentMemoryHealthStatus};
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
        if client.health().available {
            self.reap_exited_child();
            return Ok(());
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
        if client.health().available {
            self.reap_exited_child();
            return Ok(());
        }
        self.start(client)?;
        self.wait_for_health(client)
    }

    pub fn start(&self, client: &AgentMemoryClient) -> Result<(), String> {
        if client.health().available {
            self.reap_exited_child();
            return Ok(());
        }

        self.stop()?;
        cleanup_stale_listener(client);

        let mut guard = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    *guard = None;
                }
                Ok(None) => return Ok(()),
                Err(e) => {
                    return Err(format!(
                        "Failed to inspect agentmemory child process: {}",
                        e
                    ));
                }
            }
        }

        let resolved = resolve::resolve_agentmemory()?;
        self.remember_runtime(&resolved);

        let export_root = self
            .export_root
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let (program, prefix_args, spawn_cwd) = windows_spawn_command(&resolved, export_root.as_deref());
        let mut cmd = crate::process_util::command(&program);
        crate::process_util::set_new_process_group(&mut cmd);
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
                cmd.env(
                    "AGENTMEMORY_SERVICE_LOG",
                    crate::process_util::windows_command_path(&log_path),
                );
            }
            cmd.env(
                "AGENTMEMORY_EXPORT_ROOT",
                crate::process_util::windows_command_path(&export_root),
            );
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
    let bin_dir = PathBuf::from(crate::process_util::windows_command_path(bin_dir));
    let mut paths = vec![bin_dir];
    if let Ok(existing) = std::env::var("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).ok()
}

/// Bundled codegraph `node.exe` often lives under `...\node_modules\...`; on Windows,
/// Node re-parses the full command line and `\n` inside `\node_modules` truncates argv.
/// Use a small launcher script plus system Node when that path shape is detected.
fn windows_spawn_command(
    resolved: &ResolvedAgentmemory,
    export_root: Option<&Path>,
) -> (String, Vec<String>, PathBuf) {
    let working_dir =
        PathBuf::from(crate::process_util::windows_command_path(&resolved.working_dir));

    #[cfg(windows)]
    {
        let bundled = resolved.program.to_string_lossy();
        if bundled.contains("node_modules") {
            let launcher = resolved.bundle_root.join("launcher.mjs");
            if launcher.is_file() {
                if let Some(system_node) = system_node_executable() {
                    return (
                        crate::process_util::windows_command_path(&system_node),
                        vec![crate::process_util::windows_command_path(&launcher)],
                        working_dir.clone(),
                    );
                }
            }
        }
        return (
            crate::process_util::windows_command_path(&resolved.program),
            resolved.prefix_args.clone(),
            working_dir,
        );
    }

    #[cfg(not(windows))]
    {
        let _ = export_root;
        (
            resolved.program.to_string_lossy().into_owned(),
            resolved.prefix_args.clone(),
            working_dir,
        )
    }
}

fn system_node_executable() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join("node.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    #[cfg(windows)]
    {
        for candidate in [
            PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
            PathBuf::from(r"C:\Program Files (x86)\nodejs\node.exe"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn service_log_path(export_root: &Path) -> Option<PathBuf> {
    Some(export_root.join("service.log"))
}

fn cleanup_stale_listener(client: &AgentMemoryClient) {
    let health = client.health();
    if health.available || !health.orphaned_listener {
        return;
    }
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

        let (program, prefix_args, spawn_cwd) = windows_spawn_command(&resolved, None);

        let mut cmd = crate::process_util::command(&program);
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
