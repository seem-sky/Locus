export enum TaskStage {
  Read = "read",
  Implement = "implement",
  Review = "review",
}

export enum TaskStatus {
  Pending = "pending",
  InProgress = "in_progress",
  Completed = "completed",
  Failed = "failed",
}

export interface FileModification {
  path: string;
  old_content?: string | null;
  new_content: string;
  change_type: FileChangeType;
}

export enum FileChangeType {
  Create = "create",
  Modify = "modify",
  Delete = "delete",
  Rename = "rename",
}

export interface CodeChanges {
  files_created: string[];
  files_modified: FileModification[];
  files_deleted: string[];
  rationale: string;
  tests_added: string[];
}

export interface ReadAnalysis {
  task_description: string;
  target_files: string[];
  relevant_context: Record<string, string>;
  dependencies: string[];
  understanding_summary: string;
}

export interface TaskDescription {
  id: string;
  description: string;
  target_files: string[];
  context?: Record<string, string> | null;
}

export enum ReviewOutcome {
  Pass = "pass",
  NeedsRevision = "needs_revision",
  Fail = "fail",
}

export enum IssueSeverity {
  Critical = "critical",
  High = "high",
  Medium = "medium",
  Low = "low",
  Info = "info",
}

export enum IssueCategory {
  Quality = "quality",
  Security = "security",
  Performance = "performance",
  Logic = "logic",
}

export interface ReviewIssue {
  severity: IssueSeverity;
  category: IssueCategory;
  description: string;
  location?: string | null;
  suggestion?: string | null;
}

export interface ReviewSection {
  score: number;
  issues: ReviewIssue[];
  summary: string;
}

export interface ReviewReport {
  quality: ReviewSection;
  security: ReviewSection;
  performance: ReviewSection;
  logic: ReviewSection;
  overall: ReviewOutcome;
  suggestions: string[];
}

export interface TaskState {
  id: string;
  description: string;
  current_stage: TaskStage;
  status: TaskStatus;
  read_result?: ReadAnalysis | null;
  changes?: CodeChanges | null;
  review_report?: ReviewReport | null;
  error_message?: string | null;
  implementer_assigned?: boolean;
}
