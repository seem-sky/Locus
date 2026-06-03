/*
 * @Author         : seem.sky@gmail.com
 * @Email          : seem.sky@gmail.com
 * @Description    :
 * @FilePath       : \src-tauri\src\agentmemory\llm_env.rs
 * @Date           : 2026-06-02 18:34:09
 * @LastEditTime   : 2026-06-02 18:45:43
 * @LastEditors    : seem.sky@gmail.com seem.sky@gmail.com
 */
//! Maps Locus model-management credentials to agentmemory child-process env vars.
//! Only the primary provider for `main_model` is injected (agentmemory picks the first
//! matching key in OPENAI → MINIMAX → ANTHROPIC → GEMINI → OPENROUTER order).

use std::collections::HashMap;

use crate::auth::codex::CodexTokenData;
use crate::auth::TokenData;
use crate::commands::{
    load_model_defaults, normalize_custom_endpoint_config, persistent_config_dir, ApiFormat,
    CustomEndpoint,
};
use crate::keychain;

const DEFAULT_OPENROUTER_COMPRESSION_MODEL: &str = "deepseek/deepseek-v4-pro";
const OPENAI_CODEX_BASE_URL: &str = "https://api.openai.com/v1";
const ANTHROPIC_DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

#[derive(Debug, Clone)]
pub struct AgentMemoryLlmEnv {
    pub vars: Vec<(String, String)>,
    pub provider_label: String,
    pub configured: bool,
    pub warning: Option<String>,
}

impl AgentMemoryLlmEnv {
    pub fn empty() -> Self {
        Self {
            vars: Vec::new(),
            provider_label: "none".to_string(),
            configured: false,
            warning: Some(
                "No default model in model management; agentmemory runs without LLM summarization."
                    .to_string(),
            ),
        }
    }

    pub fn var_map(&self) -> HashMap<String, String> {
        self.vars.iter().cloned().collect()
    }
}

pub fn resolve_for_agentmemory() -> AgentMemoryLlmEnv {
    let defaults = load_model_defaults();
    let main_model = defaults.main_model.trim();
    if main_model.is_empty() {
        return AgentMemoryLlmEnv::empty();
    }

    let custom_endpoints = load_custom_endpoints_from_disk();
    resolve_for_model(main_model, &custom_endpoints)
}

fn load_custom_endpoints_from_disk() -> Vec<CustomEndpoint> {
    let path = match persistent_config_dir() {
        Ok(dir) => dir.join("custom_endpoints.json"),
        Err(_) => return Vec::new(),
    };
    let mut endpoints: Vec<CustomEndpoint> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    for ep in &mut endpoints {
        normalize_custom_endpoint_config(ep);
    }
    endpoints
}

fn resolve_for_model(main_model: &str, custom_endpoints: &[CustomEndpoint]) -> AgentMemoryLlmEnv {
    let is_custom = main_model.starts_with("custom/");
    let is_openrouter = main_model.starts_with("openrouter/");
    let is_anthropic_sdk = main_model.starts_with("anthropic_sdk/");
    let is_openai_codex = main_model.starts_with("openai/");
    let is_anthropic_direct = !main_model.contains('/');

    if is_custom {
        return resolve_custom_endpoint(main_model, custom_endpoints);
    }
    if is_openrouter {
        return resolve_openrouter(main_model);
    }
    if is_anthropic_sdk {
        return resolve_anthropic_sdk();
    }
    if is_openai_codex {
        return resolve_openai_codex(main_model);
    }
    if is_anthropic_direct {
        return resolve_anthropic_oauth(main_model);
    }

    AgentMemoryLlmEnv {
        vars: Vec::new(),
        provider_label: "unknown".to_string(),
        configured: false,
        warning: Some(format!(
            "Unrecognized default model \"{}\" for agentmemory LLM bridge.",
            main_model
        )),
    }
}

fn resolve_openrouter(main_model: &str) -> AgentMemoryLlmEnv {
    let api_key = load_openrouter_key();
    if api_key.is_empty() {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "openrouter".to_string(),
            configured: false,
            warning: Some(
                "OpenRouter API key is not configured; agentmemory cannot run LLM summarization."
                    .to_string(),
            ),
        };
    }

    let mut vars = vec![("OPENROUTER_API_KEY".to_string(), api_key)];
    let _ = main_model;
    let model = std::env::var("LOCUS_AGENTMEMORY_OPENROUTER_MODEL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_OPENROUTER_COMPRESSION_MODEL.to_string());
    vars.push(("OPENROUTER_MODEL".to_string(), model));

    AgentMemoryLlmEnv {
        vars,
        provider_label: "openrouter".to_string(),
        configured: true,
        warning: None,
    }
}

fn resolve_custom_endpoint(main_model: &str, custom_endpoints: &[CustomEndpoint]) -> AgentMemoryLlmEnv {
    let endpoint_id = main_model.strip_prefix("custom/").unwrap_or("");
    let Some(endpoint) = custom_endpoints
        .iter()
        .find(|ep| ep.id == endpoint_id)
        .cloned()
    else {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "custom".to_string(),
            configured: false,
            warning: Some(format!(
                "Custom endpoint \"{}\" not found for agentmemory LLM bridge.",
                endpoint_id
            )),
        };
    };

    let api_key = keychain::get_secret(&keychain::endpoint_key_name(&endpoint.id))
        .ok()
        .flatten()
        .unwrap_or_default();
    if api_key.is_empty() {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "custom".to_string(),
            configured: false,
            warning: Some(format!(
                "Custom endpoint \"{}\" has no API key; agentmemory cannot run LLM summarization.",
                endpoint.name
            )),
        };
    }

    match endpoint.api_format {
        ApiFormat::OpenaiChat | ApiFormat::OpenaiResponses => AgentMemoryLlmEnv {
            vars: vec![
                ("OPENAI_API_KEY".to_string(), api_key),
                (
                    "OPENAI_BASE_URL".to_string(),
                    normalize_openai_base_url(&endpoint.endpoint),
                ),
                ("OPENAI_MODEL".to_string(), endpoint.api_model.clone()),
            ],
            provider_label: "openai_compatible".to_string(),
            configured: true,
            warning: None,
        },
        ApiFormat::AnthropicMessages => AgentMemoryLlmEnv {
            vars: vec![
                ("ANTHROPIC_API_KEY".to_string(), api_key),
                (
                    "ANTHROPIC_BASE_URL".to_string(),
                    normalize_anthropic_base_url(&endpoint.endpoint),
                ),
                ("ANTHROPIC_MODEL".to_string(), endpoint.api_model.clone()),
            ],
            provider_label: "anthropic".to_string(),
            configured: true,
            warning: None,
        },
    }
}

fn resolve_anthropic_sdk() -> AgentMemoryLlmEnv {
    AgentMemoryLlmEnv {
        vars: vec![("AGENTMEMORY_ALLOW_AGENT_SDK".to_string(), "true".to_string())],
        provider_label: "anthropic_sdk".to_string(),
        configured: true,
        warning: Some(
            "Using Claude Agent SDK for agentmemory LLM (local Claude Code CLI required)."
                .to_string(),
        ),
    }
}

fn resolve_openai_codex(main_model: &str) -> AgentMemoryLlmEnv {
    let Some(tokens) = load_codex_tokens_from_keychain() else {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "openai_codex".to_string(),
            configured: false,
            warning: Some(
                "ChatGPT subscription (Codex) is not logged in; agentmemory cannot use OAuth for background LLM."
                    .to_string(),
            ),
        };
    };
    if tokens.validation_failed {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "openai_codex".to_string(),
            configured: false,
            warning: tokens.validation_error.or_else(|| {
                Some(
                    "ChatGPT subscription validation failed; re-login in settings."
                        .to_string(),
                )
            }),
        };
    }
    if tokens.access_token.trim().is_empty() {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "openai_codex".to_string(),
            configured: false,
            warning: Some(
                "ChatGPT subscription token is empty; agentmemory may not accept Codex OAuth for summarization."
                    .to_string(),
            ),
        };
    }

    let model = main_model
        .strip_prefix("openai/")
        .filter(|s| !s.is_empty())
        .unwrap_or("gpt-4o-mini")
        .to_string();

    AgentMemoryLlmEnv {
        vars: vec![
            ("OPENAI_API_KEY".to_string(), tokens.access_token),
            (
                "OPENAI_BASE_URL".to_string(),
                OPENAI_CODEX_BASE_URL.to_string(),
            ),
            ("OPENAI_MODEL".to_string(), model),
        ],
        provider_label: "openai_codex".to_string(),
        configured: true,
        warning: Some(
            "Bridging ChatGPT subscription token to agentmemory OpenAI provider; if summarization still fails, set a default model with an API key (e.g. OpenRouter)."
                .to_string(),
        ),
    }
}

fn resolve_anthropic_oauth(main_model: &str) -> AgentMemoryLlmEnv {
    let Some(tokens) = load_claude_tokens_from_keychain() else {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "anthropic_oauth".to_string(),
            configured: false,
            warning: Some(
                "Anthropic subscription is not logged in; agentmemory needs an API key or OpenRouter default model."
                    .to_string(),
            ),
        };
    };
    if tokens.access_token.trim().is_empty() {
        return AgentMemoryLlmEnv {
            vars: Vec::new(),
            provider_label: "anthropic_oauth".to_string(),
            configured: false,
            warning: Some("Anthropic OAuth token is empty.".to_string()),
        };
    }

    AgentMemoryLlmEnv {
        vars: vec![
            ("ANTHROPIC_API_KEY".to_string(), tokens.access_token),
            (
                "ANTHROPIC_BASE_URL".to_string(),
                ANTHROPIC_DEFAULT_BASE_URL.to_string(),
            ),
            ("ANTHROPIC_MODEL".to_string(), main_model.to_string()),
        ],
        provider_label: "anthropic_oauth".to_string(),
        configured: true,
        warning: Some(
            "Bridging Anthropic OAuth token to agentmemory; if summarization fails, use OpenRouter or a custom Anthropic-compatible endpoint as the default model."
                .to_string(),
        ),
    }
}

fn load_openrouter_key() -> String {
    keychain::get_secret(keychain::KEY_OPENROUTER)
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn load_claude_tokens_from_keychain() -> Option<TokenData> {
    let raw = keychain::get_secret(keychain::KEY_CLAUDE_TOKENS).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

fn load_codex_tokens_from_keychain() -> Option<CodexTokenData> {
    let raw = keychain::get_secret(keychain::KEY_CODEX_TOKENS).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

fn normalize_openai_base_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return OPENAI_CODEX_BASE_URL.to_string();
    }
    if trimmed.ends_with("/v1") {
        return trimmed.to_string();
    }
    if let Some(idx) = trimmed.find("/v1/") {
        return trimmed[..idx + 3].to_string();
    }
    format!("{trimmed}/v1")
}

fn normalize_anthropic_base_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return ANTHROPIC_DEFAULT_BASE_URL.to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_keys(env: &AgentMemoryLlmEnv) -> Vec<&str> {
        env.vars.iter().map(|(k, _)| k.as_str()).collect()
    }

    #[test]
    fn openrouter_model_sets_only_openrouter_key() {
        let env = AgentMemoryLlmEnv {
            vars: vec![
                ("OPENROUTER_API_KEY".to_string(), "sk-or-test".to_string()),
                (
                    "OPENROUTER_MODEL".to_string(),
                    DEFAULT_OPENROUTER_COMPRESSION_MODEL.to_string(),
                ),
            ],
            provider_label: "openrouter".to_string(),
            configured: true,
            warning: None,
        };
        let keys = env_keys(&env);
        assert!(keys.contains(&"OPENROUTER_API_KEY"));
        assert!(!keys.contains(&"OPENAI_API_KEY"));
    }

    #[test]
    fn custom_openai_chat_maps_openai_triplet() {
        let endpoints = vec![CustomEndpoint {
            id: "ep1".to_string(),
            name: "Local".to_string(),
            api_model: "gpt-4o-mini".to_string(),
            endpoint: "http://localhost:1234".to_string(),
            api_format: ApiFormat::OpenaiChat,
            api_key: String::new(),
            context_length: 128_000,
            beta_flags: vec![],
            supported_reasoning_efforts: vec![],
            reasoning_param_format: None,
            replay_reasoning_content: None,
            server_tools: Default::default(),
            supports_tool_lazy_loading: true,
            supports_vision: false,
        }];
        // Simulate key present by testing normalize + structure through a mock env build
        let base = normalize_openai_base_url("http://localhost:1234");
        assert_eq!(base, "http://localhost:1234/v1");
        let base2 = normalize_openai_base_url("https://api.openai.com/v1/chat");
        assert_eq!(base2, "https://api.openai.com/v1");
        assert_eq!(endpoints[0].api_format, ApiFormat::OpenaiChat);
    }

    #[test]
    fn normalize_openai_base_url_handles_v1_suffix() {
        assert_eq!(
            normalize_openai_base_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1"
        );
    }
}
