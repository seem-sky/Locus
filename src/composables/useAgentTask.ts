import { ref, computed, readonly } from "vue";
import type { TaskState, ReadAnalysis, CodeChanges, ReviewReport } from "../types/task";
import * as taskService from "../services/task";

const activeTaskId = ref<string | null>(null);
const activeTask = ref<TaskState | null>(null);
const taskLoading = ref(false);
const taskError = ref<string | null>(null);
let pollInterval: ReturnType<typeof setInterval> | null = null;

export function useAgentTask() {
  const isTaskActive = computed(() => activeTask.value !== null);

  const currentStage = computed(() => activeTask.value?.current_stage ?? null);

  const currentStatus = computed(() => activeTask.value?.status ?? null);

  const stageLabel = computed(() => {
    if (!currentStage.value) return "";
    switch (currentStage.value) {
      case "read":
        return "Reading";
      case "implement":
        return "Implementing";
      case "review":
        return "Reviewing";
      default:
        return "";
    }
  });

  const stageIcon = computed(() => {
    if (!currentStage.value) return "";
    switch (currentStage.value) {
      case "read":
        return "📖";
      case "implement":
        return "✏️";
      case "review":
        return "🔍";
      default:
        return "";
    }
  });

  const implementerAssigned = computed(() => activeTask.value?.implementer_assigned ?? false);

  async function createTask(
    description: string,
    targetFiles: string[]
  ): Promise<TaskState | null> {
    taskLoading.value = true;
    taskError.value = null;
    try {
      const taskId = `task-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      const task = await taskService.createMultiStageTask(taskId, description, targetFiles);
      activeTaskId.value = taskId;
      activeTask.value = task;
      startPolling();
      return task;
    } catch (e) {
      taskError.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      taskLoading.value = false;
    }
  }

  async function updateReadResult(analysis: ReadAnalysis): Promise<TaskState | null> {
    if (!activeTaskId.value) return null;
    taskLoading.value = true;
    taskError.value = null;
    try {
      const task = await taskService.updateTaskReadResult(activeTaskId.value, analysis);
      activeTask.value = task;
      return task;
    } catch (e) {
      taskError.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      taskLoading.value = false;
    }
  }

  async function updateChanges(changes: CodeChanges): Promise<TaskState | null> {
    if (!activeTaskId.value) return null;
    taskLoading.value = true;
    taskError.value = null;
    try {
      const task = await taskService.updateTaskChanges(activeTaskId.value, changes);
      activeTask.value = task;
      return task;
    } catch (e) {
      taskError.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      taskLoading.value = false;
    }
  }

  async function updateReviewReport(report: ReviewReport): Promise<TaskState | null> {
    if (!activeTaskId.value) return null;
    taskLoading.value = true;
    taskError.value = null;
    try {
      const task = await taskService.updateTaskReviewReport(activeTaskId.value, report);
      activeTask.value = task;
      stopPolling();
      return task;
    } catch (e) {
      taskError.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      taskLoading.value = false;
    }
  }

  async function refreshTask(): Promise<void> {
    if (!activeTaskId.value) return;
    try {
      const task = await taskService.getTaskState(activeTaskId.value);
      if (task) {
        activeTask.value = task;
      } else {
        activeTask.value = null;
        activeTaskId.value = null;
        stopPolling();
      }
    } catch {
      // Ignore refresh errors
    }
  }

  function startPolling(): void {
    stopPolling();
    pollInterval = setInterval(() => {
      void refreshTask();
    }, 2000);
  }

  function stopPolling(): void {
    if (pollInterval !== null) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  function clearTask(): void {
    stopPolling();
    activeTaskId.value = null;
    activeTask.value = null;
    taskError.value = null;
  }

  return {
    activeTaskId: readonly(activeTaskId),
    activeTask: readonly(activeTask),
    taskLoading: readonly(taskLoading),
    taskError: readonly(taskError),
    isTaskActive,
    currentStage,
    currentStatus,
    stageLabel,
    stageIcon,
    implementerAssigned,
    createTask,
    updateReadResult,
    updateChanges,
    updateReviewReport,
    refreshTask,
    clearTask,
  };
}
