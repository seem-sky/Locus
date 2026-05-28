use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStage {
    Read,
    Implement,
    Review,
}

impl std::fmt::Display for TaskStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStage::Read => write!(f, "read"),
            TaskStage::Implement => write!(f, "implement"),
            TaskStage::Review => write!(f, "review"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModification {
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: String,
    pub change_type: FileChangeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeType {
    Create,
    Modify,
    Delete,
    Rename,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChanges {
    pub files_created: Vec<String>,
    pub files_modified: Vec<FileModification>,
    pub files_deleted: Vec<String>,
    pub rationale: String,
    pub tests_added: Vec<String>,
}

impl Default for CodeChanges {
    fn default() -> Self {
        Self {
            files_created: Vec::new(),
            files_modified: Vec::new(),
            files_deleted: Vec::new(),
            rationale: String::new(),
            tests_added: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadAnalysis {
    pub task_description: String,
    pub target_files: Vec<String>,
    pub relevant_context: HashMap<String, String>,
    pub dependencies: Vec<String>,
    pub understanding_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDescription {
    pub id: String,
    pub description: String,
    pub target_files: Vec<String>,
    pub context: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: String,
    pub current_stage: TaskStage,
    pub read_analysis: Option<ReadAnalysis>,
    pub code_changes: Option<CodeChanges>,
    pub metadata: HashMap<String, String>,
    pub implementer_assigned: bool,
}

impl TaskContext {
    pub fn new(task_id: String) -> Self {
        Self {
            task_id,
            current_stage: TaskStage::Read,
            read_analysis: None,
            code_changes: None,
            metadata: HashMap::new(),
            implementer_assigned: false,
        }
    }

    pub fn advance_stage(&mut self) {
        self.current_stage = match self.current_stage {
            TaskStage::Read => TaskStage::Implement,
            TaskStage::Implement => TaskStage::Review,
            TaskStage::Review => TaskStage::Review,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub id: String,
    pub description: String,
    pub current_stage: TaskStage,
    pub status: TaskStatus,
    pub read_result: Option<ReadAnalysis>,
    pub changes: Option<CodeChanges>,
    #[serde(skip)]
    pub review_report: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

impl TaskState {
    pub fn new(id: String, description: String) -> Self {
        Self {
            id,
            description,
            current_stage: TaskStage::Read,
            status: TaskStatus::Pending,
            read_result: None,
            changes: None,
            review_report: None,
            error_message: None,
        }
    }
}

pub struct TaskManager {
    tasks: HashMap<String, TaskState>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    pub fn create_task(&mut self, id: String, description: String) -> TaskState {
        let state = TaskState::new(id.clone(), description);
        self.tasks.insert(id.clone(), state.clone());
        state
    }

    pub fn get_task(&self, id: &str) -> Option<&TaskState> {
        self.tasks.get(id)
    }

    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut TaskState> {
        self.tasks.get_mut(id)
    }

    pub fn update_read_result(&mut self, task_id: &str, analysis: ReadAnalysis) -> Option<()> {
        let task = self.tasks.get_mut(task_id)?;
        task.read_result = Some(analysis);
        task.current_stage = TaskStage::Implement;
        task.status = TaskStatus::InProgress;
        Some(())
    }

    pub fn update_changes(&mut self, task_id: &str, changes: CodeChanges) -> Option<()> {
        let task = self.tasks.get_mut(task_id)?;
        task.changes = Some(changes);
        task.current_stage = TaskStage::Review;
        Some(())
    }

    pub fn update_review_report(&mut self, task_id: &str, report: serde_json::Value) -> Option<()> {
        let task = self.tasks.get_mut(task_id)?;
        task.review_report = Some(report);
        task.status = TaskStatus::Completed;
        Some(())
    }

    pub fn set_failed(&mut self, task_id: &str, error: String) -> Option<()> {
        let task = self.tasks.get_mut(task_id)?;
        task.status = TaskStatus::Failed;
        task.error_message = Some(error);
        Some(())
    }

    pub fn list_tasks(&self) -> Vec<&TaskState> {
        self.tasks.values().collect()
    }

    pub fn remove_task(&mut self, task_id: &str) -> Option<TaskState> {
        self.tasks.remove(task_id)
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}
