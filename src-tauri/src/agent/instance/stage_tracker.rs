use crate::agent::task::{CodeChanges, FileChangeType, FileModification, ReadAnalysis, TaskStage};
use std::collections::HashSet;

pub struct StageTracker {
    read_files: HashSet<String>,
    modified_files: HashSet<String>,
    created_files: HashSet<String>,
    has_seen_write: bool,
    write_round_active: bool,
    has_seen_review_trigger: bool,
}

impl StageTracker {
    pub fn new() -> Self {
        Self {
            read_files: HashSet::new(),
            modified_files: HashSet::new(),
            created_files: HashSet::new(),
            has_seen_write: false,
            write_round_active: false,
            has_seen_review_trigger: false,
        }
    }

    pub fn track_read(&mut self, file_path: &str) {
        self.read_files.insert(file_path.to_string());
    }

    pub fn track_create(&mut self, file_path: &str) {
        self.has_seen_write = true;
        self.write_round_active = true;
        self.created_files.insert(file_path.to_string());
    }

    pub fn track_modify(&mut self, file_path: &str) {
        self.has_seen_write = true;
        self.write_round_active = true;
        self.modified_files.insert(file_path.to_string());
    }

    pub fn track_delete(&mut self, file_path: &str) {
        self.has_seen_write = true;
        self.write_round_active = true;
    }

    /// Track when TodoWrite or submit is called (indicates review phase)
    pub fn track_review_trigger(&mut self) {
        self.has_seen_review_trigger = true;
    }

    pub fn should_transition_to_implement(&self) -> bool {
        !self.read_files.is_empty() && self.has_seen_write
    }

    /// Check if we should transition to review phase
    /// This is called when TodoWrite or submit is detected
    pub fn should_transition_to_review(&self) -> bool {
        self.has_seen_write && self.has_seen_review_trigger
    }

    pub fn is_write_complete(&self) -> bool {
        !self.write_round_active && self.has_seen_write
    }

    pub fn begin_write_round(&mut self) {
        self.write_round_active = true;
    }

    pub fn end_write_round(&mut self) {
        self.write_round_active = false;
    }

    pub fn build_read_analysis(&self, task_description: &str) -> ReadAnalysis {
        let mut relevant_context = std::collections::HashMap::new();
        for path in &self.read_files {
            if let Ok(content) = std::fs::read_to_string(path) {
                relevant_context.insert(
                    path.clone(),
                    content.chars().take(2000).collect(),
                );
            }
        }

        let dependencies: Vec<String> = self
            .read_files
            .iter()
            .filter_map(|p| {
                let parent = std::path::Path::new(p).parent()?;
                if let Ok(entries) = std::fs::read_dir(parent) {
                    let deps: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| e.file_name().to_str().map(String::from))
                        .filter(|n| {
                            n.ends_with(".rs")
                                || n.ends_with(".ts")
                                || n.ends_with(".tsx")
                                || n.ends_with(".vue")
                                || n.ends_with(".py")
                                || n.ends_with(".go")
                                || n.ends_with(".java")
                        })
                        .collect();
                    if !deps.is_empty() {
                        return Some(format!("{}/{}", parent.display(), deps.join(",")));
                    }
                }
                None
            })
            .collect();

        ReadAnalysis {
            task_description: task_description.to_string(),
            target_files: self.read_files.iter().cloned().collect(),
            relevant_context,
            dependencies,
            understanding_summary: format!(
                "Read {} files. First edit operation detected, transitioning to Implement phase.",
                self.read_files.len()
            ),
        }
    }

    pub fn build_code_changes(&self, rationale: String) -> CodeChanges {
        let files_modified: Vec<FileModification> = self
            .modified_files
            .iter()
            .map(|p| FileModification {
                path: p.clone(),
                old_content: None,
                new_content: String::new(),
                change_type: FileChangeType::Modify,
            })
            .collect();

        CodeChanges {
            files_created: self.created_files.iter().cloned().collect(),
            files_modified,
            files_deleted: Vec::new(),
            rationale,
            tests_added: Vec::new(),
        }
    }

    pub fn reset_for_review(&mut self) {
        self.read_files.clear();
        self.modified_files.clear();
        self.created_files.clear();
        self.has_seen_write = false;
        self.write_round_active = false;
        self.has_seen_review_trigger = false;
    }

    pub fn read_files_count(&self) -> usize {
        self.read_files.len()
    }

    pub fn modified_files_count(&self) -> usize {
        self.modified_files.len()
    }

    pub fn created_files_count(&self) -> usize {
        self.created_files.len()
    }
}

impl Default for StageTracker {
    fn default() -> Self {
        Self::new()
    }
}

pub fn is_read_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read"
            | "read_file"
            | "read_multiple"
            | "grep"
            | "list"
            | "list_dir"
            | "glob"
            | "search"
            | "BatchRead"
            | "web_search"
            | "web_fetch"
    )
}

pub fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write"
            | "write_file"
            | "edit"
            | "insert"
            | "delete"
            | "move"
            | "rename"
            | "mkdir"
            | "rm"
            | "rmdir"
            | "create_directory"
            | "delete_file"
            | "mv"
    )
}

pub fn detect_stage_from_tool_name(tool_name: &str, current_stage: TaskStage) -> Option<TaskStage> {
    match current_stage {
        TaskStage::Read => {
            if is_write_tool(tool_name) {
                Some(TaskStage::Implement)
            } else {
                None
            }
        }
        TaskStage::Implement => {
            if tool_name == "TodoWrite" || tool_name == "submit" {
                Some(TaskStage::Review)
            } else {
                None
            }
        }
        TaskStage::Review => None,
    }
}
