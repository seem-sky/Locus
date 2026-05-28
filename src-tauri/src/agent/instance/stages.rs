use crate::agent::review::ReviewReport;
use crate::agent::task::{CodeChanges, ReadAnalysis, TaskContext, TaskDescription};
use std::collections::HashMap;

pub struct ReadStageOutput {
    pub analysis: ReadAnalysis,
}

pub struct ImplementStageOutput {
    pub changes: Option<CodeChanges>,
    pub needs_subagent_execution: bool,
}

pub struct ReviewStageOutput {
    pub report: ReviewReport,
}

pub struct MultiStageTask {
    pub task_id: String,
    pub description: String,
    pub target_files: Vec<String>,
    pub context: HashMap<String, String>,
}

impl MultiStageTask {
    pub fn new(task_id: String, description: String, target_files: Vec<String>) -> Self {
        Self {
            task_id,
            description,
            target_files,
            context: HashMap::new(),
        }
    }

    pub fn from_description(task_desc: &TaskDescription) -> Self {
        Self {
            task_id: task_desc.id.clone(),
            description: task_desc.description.clone(),
            target_files: task_desc.target_files.clone(),
            context: task_desc.context.clone().unwrap_or_default(),
        }
    }

    pub fn to_task_description(&self) -> TaskDescription {
        TaskDescription {
            id: self.task_id.clone(),
            description: self.description.clone(),
            target_files: self.target_files.clone(),
            context: Some(self.context.clone()),
        }
    }
}

pub struct StageExecutor {
    project_root: String,
}

impl StageExecutor {
    pub fn new(project_root: String) -> Self {
        Self { project_root }
    }

    pub fn execute_read_stage(&self, task: &MultiStageTask) -> ReadStageOutput {
        let mut relevant_context = HashMap::new();
        let mut dependencies = Vec::new();

        for file_path in &task.target_files {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                relevant_context.insert(
                    file_path.clone(),
                    content.chars().take(2000).collect(),
                );

                if let Some(parent) = std::path::Path::new(file_path).parent() {
                    if let Ok(entries) = std::fs::read_dir(parent) {
                        for entry in entries.flatten() {
                            if let Some(name) = entry.file_name().to_str() {
                                if name.ends_with(".rs") || name.ends_with(".ts") || name.ends_with(".vue") {
                                    dependencies.push(format!("{}/{}", parent.display(), name));
                                }
                            }
                        }
                    }
                }
            }
        }

        let understanding_summary = format!(
            "Task: {}\nTarget files: {} files\nDependencies: {} found",
            task.description,
            task.target_files.len(),
            dependencies.len()
        );

        ReadStageOutput {
            analysis: ReadAnalysis {
                task_description: task.description.clone(),
                target_files: task.target_files.clone(),
                relevant_context,
                dependencies,
                understanding_summary,
            },
        }
    }

    pub fn execute_implement_stage(
        &self,
        _analysis: &ReadAnalysis,
        changes: Option<CodeChanges>,
    ) -> ImplementStageOutput {
        let needs_subagent_execution = changes.is_none();
        ImplementStageOutput {
            changes,
            needs_subagent_execution,
        }
    }

    pub fn execute_review_stage(
        &self,
        changes: &CodeChanges,
    ) -> ReviewStageOutput {
        use crate::agent::instance::reviewer::CodeReviewer;

        let reviewer = CodeReviewer::new(self.project_root.clone());
        let report = reviewer.review_changes(changes);

        ReviewStageOutput { report }
    }
}

pub fn create_task_context(task: &MultiStageTask) -> TaskContext {
    TaskContext::new(task.task_id.clone())
}

pub fn advance_context_stage(context: &mut TaskContext) {
    context.advance_stage();
}

pub fn build_review_report_summary(report: &ReviewReport) -> String {
    let mut summary = String::new();
    summary.push_str(&format!("Quality: {} ({})\n", report.quality.score, report.quality.issues.len()));
    summary.push_str(&format!("Security: {} ({})\n", report.security.score, report.security.issues.len()));
    summary.push_str(&format!("Performance: {} ({})\n", report.performance.score, report.performance.issues.len()));
    summary.push_str(&format!("Logic: {} ({})\n", report.logic.score, report.logic.issues.len()));
    summary.push_str(&format!("Overall: {}\n", report.overall));

    if !report.suggestions.is_empty() {
        summary.push_str("\nSuggestions:\n");
        for suggestion in &report.suggestions {
            summary.push_str(&format!("- {}\n", suggestion));
        }
    }

    summary
}
