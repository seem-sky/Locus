use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBrowseFiltersPayload {
    #[serde(default)]
    pub blocked_folder_names: Vec<String>,
    #[serde(default)]
    pub blocked_file_names: Vec<String>,
    #[serde(default)]
    pub blocked_extensions: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceBrowseFilters {
    pub blocked_folder_names: Vec<String>,
    pub blocked_file_names: Vec<String>,
    pub blocked_extensions: Vec<String>,
}

impl WorkspaceBrowseFilters {
    pub fn from_payload(payload: Option<WorkspaceBrowseFiltersPayload>) -> Self {
        let Some(payload) = payload else {
            return Self::default();
        };
        Self {
            blocked_folder_names: normalize_folder_rules(payload.blocked_folder_names),
            blocked_file_names: normalize_name_rules(payload.blocked_file_names),
            blocked_extensions: normalize_extension_rules(payload.blocked_extensions),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.blocked_folder_names.is_empty()
            || !self.blocked_file_names.is_empty()
            || !self.blocked_extensions.is_empty()
    }

    pub fn cache_key(&self) -> String {
        if !self.is_active() {
            return String::new();
        }
        format!(
            "f:{}|n:{}|x:{}",
            self.blocked_folder_names.join(","),
            self.blocked_file_names.join(","),
            self.blocked_extensions.join(","),
        )
    }
}

fn normalize_name_rules(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.replace('\\', "/");
        if out.iter().any(|existing: &String| existing.eq_ignore_ascii_case(&normalized))
        {
            continue;
        }
        out.push(normalized);
    }
    out
}

fn normalize_folder_rules(values: Vec<String>) -> Vec<String> {
    normalize_name_rules(values)
        .into_iter()
        .map(|rule| rule.trim_matches('/').to_string())
        .filter(|rule| !rule.is_empty())
        .collect()
}

fn normalize_extension_rules(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = if trimmed.starts_with('.') {
            trimmed
        } else {
            format!(".{trimmed}")
        };
        if out.iter().any(|existing| existing == &normalized) {
            continue;
        }
        out.push(normalized);
    }
    out
}

pub fn should_skip_with_browse_filters(
    file_name: &str,
    rel_path: &str,
    is_dir: bool,
    filters: Option<&WorkspaceBrowseFilters>,
) -> bool {
    let Some(filters) = filters else {
        return false;
    };
    if !filters.is_active() {
        return false;
    }

    let rel_normalized = rel_path.replace('\\', "/");
    let rel_lower = rel_normalized.to_ascii_lowercase();

    if is_dir {
        for rule in &filters.blocked_folder_names {
            let rule_lower = rule.to_ascii_lowercase();
            if rule.contains('/') {
                if rel_lower == rule_lower || rel_lower.starts_with(&format!("{rule_lower}/")) {
                    return true;
                }
                continue;
            }
            if file_name.eq_ignore_ascii_case(rule) {
                return true;
            }
            if rel_normalized
                .split('/')
                .any(|segment| segment.eq_ignore_ascii_case(rule))
            {
                return true;
            }
        }
        return false;
    }

    for rule in &filters.blocked_file_names {
        if file_name.eq_ignore_ascii_case(rule) {
            return true;
        }
    }

    let file_lower = file_name.to_ascii_lowercase();
    for ext in &filters.blocked_extensions {
        if file_lower.ends_with(ext) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filters(
        folders: &[&str],
        files: &[&str],
        extensions: &[&str],
    ) -> WorkspaceBrowseFilters {
        WorkspaceBrowseFilters::from_payload(Some(WorkspaceBrowseFiltersPayload {
            blocked_folder_names: folders.iter().map(|s| (*s).to_string()).collect(),
            blocked_file_names: files.iter().map(|s| (*s).to_string()).collect(),
            blocked_extensions: extensions.iter().map(|s| (*s).to_string()).collect(),
        }))
    }

    #[test]
    fn skips_folder_by_segment_name() {
        let rules = filters(&["Temp"], &[], &[]);
        assert!(should_skip_with_browse_filters(
            "Temp",
            "Assets/Temp",
            true,
            Some(&rules),
        ));
    }

    #[test]
    fn skips_folder_by_path_prefix() {
        let rules = filters(&["Assets/Generated"], &[], &[]);
        assert!(should_skip_with_browse_filters(
            "Generated",
            "Assets/Generated",
            true,
            Some(&rules),
        ));
        assert!(should_skip_with_browse_filters(
            "Nested",
            "Assets/Generated/Nested",
            true,
            Some(&rules),
        ));
    }

    #[test]
    fn skips_files_by_name_and_extension() {
        let rules = filters(&[], &["Thumbs.db"], &["dll"]);
        assert!(should_skip_with_browse_filters(
            "Thumbs.db",
            "Thumbs.db",
            false,
            Some(&rules),
        ));
        assert!(should_skip_with_browse_filters(
            "plugin.dll",
            "Plugins/plugin.dll",
            false,
            Some(&rules),
        ));
    }
}
