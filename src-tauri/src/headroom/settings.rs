use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

const HEADROOM_CONFIG_FILE: &str = "headroom.json";
const DEFAULT_BASE_URL: &str = "http://localhost:8787";
const DEFAULT_MIN_COMPRESS_CHARS: u32 = 2000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadroomSettings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_context_compress_enabled")]
    pub context_compress_enabled: bool,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_rtk_path")]
    pub rtk_path: String,
    #[serde(default = "default_min_compress_chars")]
    pub min_compress_chars: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadroomSettingsStatus {
    pub settings: HeadroomSettings,
    pub library_available: bool,
    pub context_library_available: bool,
}

impl Default for HeadroomSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            context_compress_enabled: default_context_compress_enabled(),
            base_url: default_base_url(),
            api_key: String::new(),
            rtk_path: default_rtk_path(),
            min_compress_chars: default_min_compress_chars(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_context_compress_enabled() -> bool {
    true
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_min_compress_chars() -> u32 {
    DEFAULT_MIN_COMPRESS_CHARS
}

fn default_rtk_path() -> String {
    crate::rtk_runtime::default_rtk_path_for_settings()
}

static SETTINGS: OnceLock<RwLock<HeadroomSettings>> = OnceLock::new();

pub fn init(data_dir: &Path) {
    let loaded = load_from_disk(data_dir).unwrap_or_default();
    let _ = SETTINGS.set(RwLock::new(loaded));
    apply_process_env(current());
}

pub fn current() -> HeadroomSettings {
    settings_lock()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

pub fn save(settings: HeadroomSettings) -> Result<HeadroomSettings, String> {
    let sanitized = sanitize(settings);
    {
        let mut guard = settings_lock().write().map_err(|error| error.to_string())?;
        *guard = sanitized.clone();
    }
    persist_to_disk(&sanitized)?;
    apply_process_env(sanitized.clone());
    Ok(sanitized)
}

pub fn reset_to_defaults() {
    let defaults = HeadroomSettings::default();
    if let Ok(mut guard) = settings_lock().write() {
        *guard = defaults.clone();
    }
    apply_process_env(defaults);
}

pub fn status() -> HeadroomSettingsStatus {
    let settings = current();
    HeadroomSettingsStatus {
        library_available: super::library_available(),
        context_library_available: super::context_library_available(),
        settings,
    }
}

pub fn enabled() -> bool {
    if env_disabled() {
        return false;
    }
    current().enabled
}

pub fn proxy_autostart_enabled() -> bool {
    match std::env::var("LOCUS_HEADROOM_PROXY_AUTOSTART")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("0") | Some("false") | Some("no") => false,
        _ => true,
    }
}

/// Whether Locus should ensure a local `headroom proxy` is running (not Headroom Cloud).
pub fn proxy_autostart_wanted() -> bool {
    if !enabled() {
        return false;
    }
    if !proxy_autostart_enabled() {
        return false;
    }
    uses_local_proxy_endpoint()
}

pub fn uses_local_proxy_endpoint() -> bool {
    let url = base_url().to_ascii_lowercase();
    url.contains("127.0.0.1")
        || url.contains("localhost")
        || url.contains("[::1]")
}

pub fn context_compress_enabled() -> bool {
    if env_context_disabled() {
        return false;
    }
    enabled() && current().context_compress_enabled
}

pub fn base_url() -> String {
    std::env::var("HEADROOM_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| current().base_url.clone())
}

pub fn api_key() -> Option<String> {
    std::env::var("HEADROOM_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let key = current().api_key.trim().to_string();
            if key.is_empty() {
                None
            } else {
                Some(key)
            }
        })
}

pub fn rtk_path_override() -> Option<String> {
    std::env::var("LOCUS_HEADROOM_RTK_PATH")
        .ok()
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let path = current().rtk_path.trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        })
}

pub fn min_compress_chars() -> usize {
    std::env::var("LOCUS_HEADROOM_MIN_COMPRESS_CHARS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or_else(|| current().min_compress_chars.max(1) as usize)
}

fn env_disabled() -> bool {
    matches!(
        std::env::var("LOCUS_HEADROOM_DISABLED")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

fn env_context_disabled() -> bool {
    matches!(
        std::env::var("LOCUS_HEADROOM_CONTEXT_COMPRESS")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0") | Some("false") | Some("no")
    )
}

fn sanitize(mut settings: HeadroomSettings) -> HeadroomSettings {
    settings.base_url = settings.base_url.trim().to_string();
    if settings.base_url.is_empty() {
        settings.base_url = default_base_url();
    }
    settings.api_key = settings.api_key.trim().to_string();
    settings.rtk_path = settings.rtk_path.trim().to_string();
    if settings.rtk_path.is_empty() {
        settings.rtk_path = default_rtk_path();
    }
    settings.min_compress_chars = settings.min_compress_chars.max(1);
    settings
}

fn settings_lock() -> &'static RwLock<HeadroomSettings> {
    SETTINGS.get_or_init(|| RwLock::new(HeadroomSettings::default()))
}

fn config_path() -> Result<PathBuf, String> {
    crate::commands::persistent_config_dir()
        .map(|dir| dir.join(HEADROOM_CONFIG_FILE))
        .map_err(|error| format!("failed to resolve headroom config dir: {error}"))
}

fn load_from_disk(_data_dir: &Path) -> Option<HeadroomSettings> {
    let path = config_path().ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str::<HeadroomSettings>(&raw)
        .ok()
        .map(sanitize)
}

fn persist_to_disk(settings: &HeadroomSettings) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create headroom config dir: {error}"))?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("failed to serialize headroom config: {error}"))?;
    std::fs::write(&path, json)
        .map_err(|error| format!("failed to write headroom config '{}': {error}", path.display()))
}

fn apply_process_env(settings: HeadroomSettings) {
    std::env::set_var("HEADROOM_BASE_URL", &settings.base_url);
    if settings.api_key.is_empty() {
        std::env::remove_var("HEADROOM_API_KEY");
    } else {
        std::env::set_var("HEADROOM_API_KEY", &settings.api_key);
    }
    if settings.rtk_path.is_empty() {
        std::env::remove_var("LOCUS_HEADROOM_RTK_PATH");
    } else {
        std::env::set_var("LOCUS_HEADROOM_RTK_PATH", &settings.rtk_path);
    }
    std::env::set_var(
        "LOCUS_HEADROOM_MIN_COMPRESS_CHARS",
        settings.min_compress_chars.to_string(),
    );
    if settings.enabled {
        std::env::remove_var("LOCUS_HEADROOM_DISABLED");
    } else {
        std::env::set_var("LOCUS_HEADROOM_DISABLED", "1");
    }
    if settings.context_compress_enabled {
        std::env::remove_var("LOCUS_HEADROOM_CONTEXT_COMPRESS");
    } else {
        std::env::set_var("LOCUS_HEADROOM_CONTEXT_COMPRESS", "0");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_local_proxy_endpoint_detects_loopback() {
        assert!(uses_local_proxy_endpoint());
    }

    #[test]
    fn sanitize_fills_default_base_url() {
        let settings = sanitize(HeadroomSettings {
            base_url: "  ".to_string(),
            ..HeadroomSettings::default()
        });
        assert_eq!(settings.base_url, DEFAULT_BASE_URL);
    }
}
