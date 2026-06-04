//! Persisted READ/PLAN ambiguous-tool whitelist (app storage).

use super::{effective_tool_args, normalize_bash_whitelist_key, resolve_effective_tool_name};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub const WORKFLOW_TOOL_WHITELIST_FILENAME: &str = "workflow_tool_whitelist.json";

/// User-approved ambiguous tools / bash commands for Dev workflow READ/PLAN phase.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowAmbiguousWhitelist {
    #[serde(default)]
    pub tools: HashSet<String>,
    #[serde(default)]
    pub bash_commands: HashSet<String>,
}

impl WorkflowAmbiguousWhitelist {
    pub fn load_from_dir(data_dir: &Path) -> Self {
        let path = data_dir.join(WORKFLOW_TOOL_WHITELIST_FILENAME);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_to_dir(&self, data_dir: &Path) -> Result<(), String> {
        let path = data_dir.join(WORKFLOW_TOOL_WHITELIST_FILENAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize workflow tool whitelist: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to save workflow tool whitelist: {}", e))
    }

    pub fn is_whitelisted(&self, tool_name: &str, args: &Value) -> bool {
        let effective = resolve_effective_tool_name(tool_name, args);
        let effective_args = effective_tool_args(tool_name, args);
        if effective == "bash" {
            let Some(command) = effective_args.get("command").and_then(|v| v.as_str()) else {
                return false;
            };
            return self
                .bash_commands
                .contains(&normalize_bash_whitelist_key(command));
        }
        self.tools.contains(&effective)
    }

    pub fn add(&mut self, tool_name: &str, args: &Value) -> bool {
        let effective = resolve_effective_tool_name(tool_name, args);
        let effective_args = effective_tool_args(tool_name, args);
        if effective == "bash" {
            let Some(command) = effective_args.get("command").and_then(|v| v.as_str()) else {
                return false;
            };
            return self
                .bash_commands
                .insert(normalize_bash_whitelist_key(command));
        }
        self.tools.insert(effective)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_whitelist_matches_normalized_command() {
        let mut list = WorkflowAmbiguousWhitelist::default();
        let args = serde_json::json!({"command": "  foo.sh  --bar  "});
        list.add("bash", &args);
        assert!(list.is_whitelisted(
            "bash",
            &serde_json::json!({"command": "foo.sh --bar"})
        ));
    }
}
