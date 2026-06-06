//! Persisted READ/PLAN ambiguous-tool whitelist (app storage).

use super::{
    bash_command_matches_whitelist_entry, bash_whitelist_storage_key, effective_tool_args,
    resolve_effective_tool_name,
};
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
        let mut list = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default();
        list.compact_bash_command_entries();
        list
    }

    /// Collapse stored bash lines to prefix keys where possible (e.g. long grep → `grep -rn`).
    fn compact_bash_command_entries(&mut self) {
        let keys: Vec<String> = self
            .bash_commands
            .drain()
            .map(|entry| bash_whitelist_storage_key(&entry))
            .collect();
        self.bash_commands = keys.into_iter().collect();
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
                .iter()
                .any(|entry| bash_command_matches_whitelist_entry(command, entry));
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
            return self.bash_commands.insert(bash_whitelist_storage_key(command));
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

    #[test]
    fn bash_whitelist_prefix_matches_grep_variants() {
        let mut list = WorkflowAmbiguousWhitelist::default();
        let long = r#"grep -rn "pb.decode/" Assets.Lua/ 2>/dev/null | head -10"#;
        list.add("bash", &serde_json::json!({"command": long}));
        assert!(list.bash_commands.iter().any(|e| e == "grep -rn"));
        let other = r#"grep -rn "string_unpack/" "H:/texas-game/Assets.Lua/Core/" 2>/dev/null | head -20"#;
        assert!(list.is_whitelisted("bash", &serde_json::json!({"command": other})));
        assert!(!list.is_whitelisted(
            "bash",
            &serde_json::json!({"command": "grep -r foo"})
        ));
    }
}
