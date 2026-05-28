import { ipcInvoke } from "./ipc";
import type {
  TaskState,
  ReadAnalysis,
  CodeChanges,
  ReviewReport,
} from "../types/task";

export function createMultiStageTask(
  taskId: string,
  description: string,
  targetFiles: string[],
): Promise<TaskState> {
  return ipcInvoke<TaskState>("create_multi_stage_task", {
    task_id: taskId,
    description,
    target_files: targetFiles,
  });
}

export function getTaskState(taskId: string): Promise<TaskState | null> {
  return ipcInvoke<TaskState | null>("get_task_state", { task_id: taskId });
}

export function updateTaskReadResult(
  taskId: string,
  analysis: ReadAnalysis,
): Promise<TaskState> {
  return ipcInvoke<TaskState>("update_task_read_result", {
    task_id: taskId,
    analysis,
  });
}

export function updateTaskChanges(
  taskId: string,
  changes: CodeChanges,
): Promise<TaskState> {
  return ipcInvoke<TaskState>("update_task_changes", {
    task_id: taskId,
    changes,
  });
}

export function updateTaskReviewReport(
  taskId: string,
  report: ReviewReport,
): Promise<TaskState> {
  return ipcInvoke<TaskState>("update_task_review_report", {
    task_id: taskId,
    report,
  });
}

export function listTasks(): Promise<TaskState[]> {
  return ipcInvoke<TaskState[]>("list_tasks");
}

export function deleteTask(taskId: string): Promise<boolean> {
  return ipcInvoke<boolean>("delete_task", { task_id: taskId });
}
