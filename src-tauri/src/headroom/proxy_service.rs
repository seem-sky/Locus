use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::AppHandle;

use super::proxy_client::{HeadroomProxyClient, HeadroomProxyHealth};
use super::resolve::{self, ResolvedHeadroomProxy};

const STARTUP_POLL_MS: u64 = 500;
const STARTUP_MAX_ATTEMPTS: u32 = 240;
const BUNDLED_STARTUP_MAX_ATTEMPTS: u32 = 360;

static GLOBAL_PROXY_STATE: OnceLock<std::sync::Arc<HeadroomProxyState>> = OnceLock::new();

pub struct HeadroomProxyState {
    service: HeadroomProxyService,
    client: HeadroomProxyClient,
    app_handle: Mutex<Option<AppHandle>>,
}

impl HeadroomProxyState {
    pub fn new() -> Self {
        Self {
            service: HeadroomProxyService::from_env(),
            client: HeadroomProxyClient::from_settings(),
            app_handle: Mutex::new(None),
        }
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        if let Ok(mut guard) = self.app_handle.lock() {
            *guard = Some(handle);
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
        let app_handle = self
            .app_handle
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        self.service.ensure_running(&client, app_handle.as_ref())
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

    pub fn ensure_running(
        &self,
        client: &HeadroomProxyClient,
        app_handle: Option<&AppHandle>,
    ) -> Result<(), String> {
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

        self.start(client, app_handle)?;
        self.wait_for_health(client, app_handle)
    }

    pub fn start(
        &self,
        client: &HeadroomProxyClient,
        app_handle: Option<&AppHandle>,
    ) -> Result<(), String> {
        self.reap_exited_child();
        if client.health().available && self.locus_managed_process_alive() {
            return Ok(());
        }

        self.stop()?;

        let resolved = resolve::resolve_headroom_proxy(app_handle)?;
        let mut guard = self.child.lock().map_err(|error| error.to_string())?;

        let mut cmd = build_proxy_command(&resolved)?;
        crate::process_util::suppress_command_window(&mut cmd);
        crate::network::apply_proxy_env_to_command(&mut cmd);
        crate::process_util::set_new_process_group(&mut cmd);
        cmd.current_dir(&resolved.working_dir);
        cmd.stdout(Stdio::null());

        if let Some(log_path) = self.service_log_path() {
            if let Some(parent) = log_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(log_file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                cmd.stderr(Stdio::from(log_file));
            }
            eprintln!(
                "[Locus] starting headroom proxy at {} (bundled={}, log: {})",
                client.base_url(),
                resolved.using_bundled_runtime,
                log_path.display()
            );
        } else {
            cmd.stderr(Stdio::null());
            eprintln!(
                "[Locus] starting headroom proxy at {} (bundled={})",
                client.base_url(),
                resolved.using_bundled_runtime
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

    fn wait_for_health(
        &self,
        client: &HeadroomProxyClient,
        app_handle: Option<&AppHandle>,
    ) -> Result<(), String> {
        let bundled = super::resolve::resolve_headroom_proxy(app_handle)
            .map(|resolved| resolved.using_bundled_runtime)
            .unwrap_or(false);
        let max_attempts = if bundled {
            BUNDLED_STARTUP_MAX_ATTEMPTS
        } else {
            STARTUP_MAX_ATTEMPTS
        };
        for attempt in 0..max_attempts {
            if client.health().available {
                eprintln!("[Locus] headroom proxy ready at {}", client.base_url());
                return Ok(());
            }
            if attempt == 0 || (attempt + 1) % 20 == 0 {
                self.reap_exited_child();
            }
            std::thread::sleep(Duration::from_millis(STARTUP_POLL_MS));
        }
        let log_hint = self
            .service_log_path()
            .filter(|path| path.is_file())
            .map(|path| format!(" See log: {}", path.display()))
            .unwrap_or_default();
        Err(format!(
            "headroom proxy failed to become healthy at {}{log_hint}. {}",
            client.base_url(),
            resolve::HEADROOM_PROXY_BUNDLE_HINT
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

fn build_proxy_command(resolved: &ResolvedHeadroomProxy) -> Result<std::process::Command, String> {
    #[cfg(windows)]
    let program = crate::process_util::windows_command_path(&resolved.program);
    #[cfg(not(windows))]
    let program = resolved.program.clone();

    let mut cmd = std::process::Command::new(program);
    cmd.args(&resolved.prefix_args);

    if resolved.using_bundled_runtime {
        let lib_dir = resolved.working_dir.join("lib");
        apply_bundled_python_env(&mut cmd, &resolved.program, &lib_dir)?;
    }

    Ok(cmd)
}

fn apply_bundled_python_env(
    cmd: &mut std::process::Command,
    python: &Path,
    lib_dir: &Path,
) -> Result<(), String> {
    cmd.env("PYTHONNOUSERSITE", "1");
    cmd.env("PYTHONPATH", lib_dir.as_os_str());

    if python
        .to_string_lossy()
        .replace('\\', "/")
        .contains("managed-python")
    {
        if let Some(home) = python.parent() {
            cmd.env("PYTHONHOME", home.as_os_str());
        }
    }

    if let Some(path) = prepend_path_env(lib_dir) {
        cmd.env("PATH", path);
    }

    Ok(())
}

fn prepend_path_env(extra: &Path) -> Option<std::ffi::OsString> {
    let mut paths = vec![extra.to_path_buf()];
    if let Ok(existing) = std::env::var("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).ok()
}
