mod client;
mod context;
mod messages;
mod proxy_client;
mod proxy_service;
pub mod resolve;
mod rewrite;
mod settings;
mod tool_output;

pub use client::{
    compress_bash_output, compress_chat_messages, compress_tool_output, context_library_available,
    library_available,
};
pub use proxy_service::{ensure_proxy_ready, HeadroomProxyState};
pub use context::{compress_prepared_messages, log_round_compress_summary};
pub use settings::{
    always_compress_context_enabled, context_compress_enabled, init as init_headroom_settings,
    min_compress_chars, proxy_autostart_wanted, reset_to_defaults as reset_headroom_settings,
    save as save_headroom_settings, status as headroom_settings_status, uses_local_proxy_endpoint,
    HeadroomProxyRuntimeStatus, HeadroomProxySource, HeadroomSettings, HeadroomSettingsStatus,
};
pub use rewrite::{
    augment_path_with_headroom_rtk, grep_tool_native_meta, read_tool_native_meta,
    rewrite_bash_with_meta, rewrite_with_meta, try_execute_rtk_grep, HeadroomRewriteMeta,
};
pub use tool_output::{
    finalize_success_output, maybe_compress_tool_output, record_execution_meta, tool_native_meta,
};

use serde::Serialize;
use serde_json::Value;

const SETUP_HINT: &str =
    "RTK compresses supported CLI commands; bundled headroom proxy autostarts for Library compress before auto-compact";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeadroomCompressMeta {
    pub enabled: bool,
    pub available: bool,
    pub compressed: bool,
    pub original_chars: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_before: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_after: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_saved: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transforms_applied: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ccr_hashes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeadroomExecutionMeta {
    pub rewrite: HeadroomRewriteMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<HeadroomCompressMeta>,
}

impl HeadroomExecutionMeta {
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

pub fn enabled() -> bool {
    settings::enabled()
}

pub fn progress_info(meta: &HeadroomRewriteMeta) -> String {
    serde_json::to_string(meta).unwrap_or_else(|_| "{}".to_string())
}

pub fn execution_meta_json(rewrite: HeadroomRewriteMeta, compress: Option<HeadroomCompressMeta>) -> Value {
    serde_json::json!({
        "headroom": HeadroomExecutionMeta { rewrite, compress }
    })
}

pub fn setup_hint() -> &'static str {
    SETUP_HINT
}
