use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use super::proxy_client::{HeadroomProxyClient, HeadroomProxyHealth};

const STARTUP_POLL_MS: u64 = 400;
const STARTUP_MAX_ATTEMPTS: u32 = 75;

static GLOBAL_PROXY_STATE: OnceLock<std::sync::Arc<HeadroomProxyState>> = OnceLock::new();

pub struct HeadroomProxyState {
    service: HeadroomProxyService,
    client: HeadroomProxyClient,
}

impl HeadroomProxyState {
    pub fn new() -> Self {
        Self {
            service: HeadroomProxyService::from_env(),
            client: HeadroomProxyClient::from_settings(),
        }
    }

    pub fn install_global(state: std::sync::Arc<Self>) {
        let _ = GLOBAL_PROXY_STATE.set(state);
    }

    pub fn global() -> Option<std::sync::Arc<Self>> {
        GLOBAL_PROXY_STATE.get().cloned()
    }

    pub fn set_log_dir(&self, path: PathBuf) {
        self.service.set_log_dir(path);
    }

    pub fn health(&self) -> HeadroomProxyHealth {
        HeadroomProxyClient::from_settings().health()
    }

    pub fn ensure_ready(&self) -> Result<(), String> {
        if !crate::headroom::settings::proxy_autostart_wanted() {
            return Ok(());
        }
        let client = HeadroomProxyClient::from_settings();
        self.service.ensure_running(&client)
    }
}

pub fn ensure_proxy_ready() -> Result<(), String> {
    if let Some(state) = HeadroomProxyState::global() {
        return state.ensure_ready();
    }
    Ok(())
}

pub struct HeadroomProxyService {
    child: Mutex<Option<Child>>,
    autostart: bool,
    log_dir: Mutex<Option<PathBuf>>,
}

impl HeadroomProxyService {
    pub fn from_env() -> Self {
        let autostart = crate::headroom::settings::proxy_autostart_enabled();
        Self {
            child: Mutex::new(None),
            autostart,
            log_dir: Mutex::new(None),
        }
    }

    pub fn set_log_dir(&self, path: PathBuf) {
        if let Ok(mut guard) = self.log_dir.lock() {
            *guard = Some(path);
        }
    }

    fn locus_managed_process_alive(&self) -> bool {
        let Ok(mut guard) = self.child.lock() else {
            return false;
        };
        let Some(child) = guard.as_mut() else {
            return false;
        };
        matches!(child.try_wait(), Ok(None))
    }

    pub fn ensure_running(&self, client: &HeadroomProxyClient) -> Result<(), String> {
        let health = client.health();
        if health.available {
            if self.locus_managed_process_alive() {
                eprintln!(
                    "[Locus] headroom proxy ready (managed) at {}",
                    client.base_url()
                );
            } else {
                eprintln!(
                    "[Locus] headroom proxy already running at {}",
                    client.base_url()
                );
            }
            return Ok(());
        }

        if !self.autostart {
            return Err(format!(
                "headroom proxy is not running at {}. Start it with: headroom proxy",
                client.base_url()
            ));
        }

        self.start(client)?;
        self.wait_for_health(client)
    }

    pub fn start(&self, client: &HeadroomProxyClient) -> Result<(), String> {
        self.reap_exited_child();
        if client.health().available && self.locus_managed_process_alive() {
            return Ok(());
        }

        self.stop()?;

        let program = resolve_headroom_cli()?;
        let mut guard = self.child.lock().map_err(|error| error.to_string())?;

        let mut cmd = std::process::Command::new(&program);
        crate::process_util::suppress_command_window(&mut cmd);
        crate::network::apply_proxy_env_to_command(&mut cmd);
        crate::process_util::set_new_process_group(&mut cmd);
        cmd.arg("proxy").stdout(Stdio::null());

        if let Some(log_path) = self.service_log_path() {
            if let Ok(log_file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                cmd.stderr(Stdio::from(log_file));
            }
            eprintln!(
                "[Locus] starting headroom proxy at {} (log: {})",
                client.base_url(),
                log_path.display()
            );
        } else {
            cmd.stderr(Stdio::null());
            eprintln!(
                "[Locus] starting headroom proxy at {}",
                client.base_url()
            );
        }

        let child = cmd
            .spawn()
            .map_err(|error| format!("failed to start headroom proxy: {error}"))?;
        *guard = Some(child);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.child.lock().map_err(|error| error.to_string())?;
        if let Some(mut child) = guard.take() {
            crate::process_util::kill_process_tree(&mut child);
        }
        Ok(())
    }

    fn wait_for_health(&self, client: &HeadroomProxyClient) -> Result<(), String> {
        for _ in 0..STARTUP_MAX_ATTEMPTS {
            if client.health().available {
                eprintln!("[Locus] headroom proxy ready at {}", client.base_url());
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(STARTUP_POLL_MS));
        }
        let log_hint = self
            .service_log_path()
            .filter(|path| path.is_file())
            .map(|path| format!(" See log: {}", path.display()))
            .unwrap_or_default();
        Err(format!(
            "headroom proxy failed to become healthy at {}{log_hint}. Install CLI: pip install 'headroom-ai[proxy]'",
            client.base_url()
        ))
    }

    fn service_log_path(&self) -> Option<PathBuf> {
        self.log_dir
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .map(|dir| dir.join("service.log"))
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

impl Drop for HeadroomProxyService {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn resolve_headroom_cli() -> Result<PathBuf, String> {
    if let Ok(raw) = std::env::var("LOCUS_HEADROOM_CLI") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_file() {
                return Ok(path);
            }
            return Err(format!(
                "LOCUS_HEADROOM_CLI is not a file: {}",
                path.display()
            ));
        }
    }

    for name in headroom_cli_names() {
        if let Some(path) = find_on_path(name) {
            return Ok(path);
        }
    }

    Err(
        "headroom CLI not found on PATH (install: pip install 'headroom-ai[proxy]', or set LOCUS_HEADROOM_CLI)"
            .to_string(),
    )
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

#[cfg(windows)]
fn headroom_cli_names() -> [&'static str; 2] {
    ["headroom.exe", "headroom"]
}

#[cfg(not(windows))]
fn headroom_cli_names() -> [&'static str; 1] {
    ["headroom"]
}
