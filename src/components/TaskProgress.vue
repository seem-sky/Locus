<script setup lang="ts">
import { computed } from "vue";
import { t } from "../i18n";
import type { TaskState, TaskStage } from "../types/task";
import { TaskStage as TaskStageEnum, IssueSeverity } from "../types/task";

const props = defineProps<{
  task: TaskState;
}>();

const emit = defineEmits<{
  (e: "stage-click", stage: TaskStage): void;
  (e: "view-details"): void;
}>();

const stageOrder: TaskStage[] = [TaskStageEnum.Read, TaskStageEnum.Implement, TaskStageEnum.Review];

const currentStageIndex = computed(() => stageOrder.indexOf(props.task.current_stage));

function getStageStatus(stage: TaskStage): "completed" | "current" | "pending" {
  const stageIndex = stageOrder.indexOf(stage);
  const currentIndex = currentStageIndex.value;

  if (stageIndex < currentIndex) return "completed";
  if (stageIndex === currentIndex) return "current";
  return "pending";
}

function stageLabel(stage: TaskStage): string {
  switch (stage) {
    case TaskStageEnum.Read:
      return t("task.stage.read");
    case TaskStageEnum.Implement:
      return t("task.stage.implement");
    case TaskStageEnum.Review:
      return t("task.stage.review");
  }
}

function stageIcon(stage: TaskStage): string {
  switch (stage) {
    case TaskStageEnum.Read:
      return "📖";
    case TaskStageEnum.Implement:
      return "✏️";
    case TaskStageEnum.Review:
      return "🔍";
    default:
      return "📋";
  }
}

function severityColor(severity: IssueSeverity): string {
  switch (severity) {
    case IssueSeverity.Critical:
      return "var(--color-error)";
    case IssueSeverity.High:
      return "var(--color-warning)";
    case IssueSeverity.Medium:
      return "var(--color-info)";
    case IssueSeverity.Low:
      return "var(--color-success)";
    case IssueSeverity.Info:
      return "var(--color-secondary)";
    default:
      return "var(--text-secondary)";
  }
}

const reviewReport = computed(() => props.task.review_report);

function sectionScoreClass(score: number): string {
  if (score >= 80) return "score-high";
  if (score >= 60) return "score-medium";
  return "score-low";
}

function overallLabel(outcome: string): string {
  switch (outcome) {
    case "pass":
      return t("task.review.pass");
    case "needs_revision":
      return t("task.review.needs_revision");
    case "fail":
      return t("task.review.fail");
    default:
      return outcome;
  }
}

function overallClass(outcome: string): string {
  switch (outcome) {
    case "pass":
      return "overall-pass";
    case "needs_revision":
      return "overall-warning";
    case "fail":
      return "overall-fail";
    default:
      return "";
  }
}
</script>

<template>
  <div class="task-progress">
    <div class="task-header">
      <h3 class="task-title">{{ task.description }}</h3>
      <span class="task-id">#{{ task.id.slice(0, 8) }}</span>
    </div>

    <div class="stages">
      <div
        v-for="(stage, index) in stageOrder"
        :key="stage"
        class="stage"
        :class="getStageStatus(stage)"
        @click="emit('stage-click', stage)"
      >
        <div class="stage-icon">{{ stageIcon(stage) }}</div>
        <div class="stage-info">
          <div class="stage-label">{{ stageLabel(stage) }}</div>
          <div class="stage-status-dot" />
        </div>
        <div v-if="index < stageOrder.length - 1" class="stage-connector" />
      </div>
    </div>

    <div v-if="reviewReport" class="review-report">
      <div class="review-header">
        <h4>{{ t("task.review.title") }}</h4>
        <span class="overall-badge" :class="overallClass(reviewReport.overall)">
          {{ overallLabel(reviewReport.overall) }}
        </span>
      </div>

      <div class="review-sections">
        <div class="review-section">
          <div class="section-header">
            <span>{{ t("task.review.quality") }}</span>
            <span class="score" :class="sectionScoreClass(reviewReport.quality.score)">
              {{ reviewReport.quality.score }}
            </span>
          </div>
          <div v-if="reviewReport.quality.issues.length > 0" class="issues">
            <div
              v-for="(issue, i) in reviewReport.quality.issues.slice(0, 3)"
              :key="i"
              class="issue"
            >
              <span class="issue-dot" :style="{ background: severityColor(issue.severity) }" />
              <span class="issue-text">{{ issue.description }}</span>
            </div>
            <div v-if="reviewReport.quality.issues.length > 3" class="more-issues">
              +{{ reviewReport.quality.issues.length - 3 }} {{ t("task.review.more") }}
            </div>
          </div>
        </div>

        <div class="review-section">
          <div class="section-header">
            <span>{{ t("task.review.security") }}</span>
            <span class="score" :class="sectionScoreClass(reviewReport.security.score)">
              {{ reviewReport.security.score }}
            </span>
          </div>
          <div v-if="reviewReport.security.issues.length > 0" class="issues">
            <div
              v-for="(issue, i) in reviewReport.security.issues.slice(0, 3)"
              :key="i"
              class="issue"
            >
              <span class="issue-dot" :style="{ background: severityColor(issue.severity) }" />
              <span class="issue-text">{{ issue.description }}</span>
            </div>
            <div v-if="reviewReport.security.issues.length > 3" class="more-issues">
              +{{ reviewReport.security.issues.length - 3 }} {{ t("task.review.more") }}
            </div>
          </div>
        </div>

        <div class="review-section">
          <div class="section-header">
            <span>{{ t("task.review.performance") }}</span>
            <span class="score" :class="sectionScoreClass(reviewReport.performance.score)">
              {{ reviewReport.performance.score }}
            </span>
          </div>
          <div v-if="reviewReport.performance.issues.length > 0" class="issues">
            <div
              v-for="(issue, i) in reviewReport.performance.issues.slice(0, 3)"
              :key="i"
              class="issue"
            >
              <span class="issue-dot" :style="{ background: severityColor(issue.severity) }" />
              <span class="issue-text">{{ issue.description }}</span>
            </div>
            <div v-if="reviewReport.performance.issues.length > 3" class="more-issues">
              +{{ reviewReport.performance.issues.length - 3 }} {{ t("task.review.more") }}
            </div>
          </div>
        </div>

        <div class="review-section">
          <div class="section-header">
            <span>{{ t("task.review.logic") }}</span>
            <span class="score" :class="sectionScoreClass(reviewReport.logic.score)">
              {{ reviewReport.logic.score }}
            </span>
          </div>
          <div v-if="reviewReport.logic.issues.length > 0" class="issues">
            <div
              v-for="(issue, i) in reviewReport.logic.issues.slice(0, 3)"
              :key="i"
              class="issue"
            >
              <span class="issue-dot" :style="{ background: severityColor(issue.severity) }" />
              <span class="issue-text">{{ issue.description }}</span>
            </div>
            <div v-if="reviewReport.logic.issues.length > 3" class="more-issues">
              +{{ reviewReport.logic.issues.length - 3 }} {{ t("task.review.more") }}
            </div>
          </div>
        </div>
      </div>

      <div v-if="reviewReport.suggestions.length > 0" class="suggestions">
        <h5>{{ t("task.review.suggestions") }}</h5>
        <ul>
          <li v-for="(suggestion, i) in reviewReport.suggestions" :key="i">
            {{ suggestion }}
          </li>
        </ul>
      </div>
    </div>

    <div v-if="task.status === 'failed'" class="task-error">
      <span class="error-icon">⚠️</span>
      <span class="error-message">{{ task.error_message || t("task.error.unknown") }}</span>
    </div>

    <button class="view-details-btn" @click="emit('view-details')">
      {{ t("task.view_details") }}
    </button>
  </div>
</template>

<style scoped>
.task-progress {
  background: var(--bg-color);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 16px;
}

.task-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.task-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-color);
  margin: 0;
}

.task-id {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: monospace;
}

.stages {
  display: flex;
  align-items: center;
  gap: 0;
  margin-bottom: 20px;
}

.stage {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.15s;
  position: relative;
}

.stage:hover {
  background: var(--hover-bg);
}

.stage.completed {
  opacity: 0.7;
}

.stage.current {
  background: var(--active-bg);
}

.stage.pending {
  opacity: 0.4;
}

.stage-icon {
  font-size: 18px;
}

.stage-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.stage-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-color);
}

.stage-status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--border-color);
}

.stage.current .stage-status-dot {
  background: var(--accent-color);
  animation: pulse 1.5s infinite;
}

.stage.completed .stage-status-dot {
  background: var(--color-success);
}

.stage-connector {
  width: 24px;
  height: 2px;
  background: var(--border-color);
  margin: 0 4px;
}

.stage.completed + .stage-connector,
.stage.completed ~ .stage-connector {
  background: var(--color-success);
}

@keyframes pulse {
  0%, 100% {
    opacity: 1;
  }
  50% {
    opacity: 0.5;
  }
}

.review-report {
  background: var(--hover-bg);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 12px;
}

.review-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.review-header h4 {
  font-size: 13px;
  font-weight: 600;
  margin: 0;
  color: var(--text-color);
}

.overall-badge {
  font-size: 11px;
  font-weight: 500;
  padding: 2px 8px;
  border-radius: 4px;
}

.overall-pass {
  background: var(--color-success);
  color: white;
}

.overall-warning {
  background: var(--color-warning);
  color: white;
}

.overall-fail {
  background: var(--color-error);
  color: white;
}

.review-sections {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}

.review-section {
  background: var(--bg-color);
  border-radius: 6px;
  padding: 8px;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
  font-size: 11px;
  font-weight: 500;
  color: var(--text-secondary);
}

.score {
  font-weight: 700;
  font-size: 13px;
}

.score-high {
  color: var(--color-success);
}

.score-medium {
  color: var(--color-warning);
}

.score-low {
  color: var(--color-error);
}

.issues {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.issue {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: var(--text-secondary);
}

.issue-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
}

.issue-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.more-issues {
  font-size: 10px;
  color: var(--text-secondary);
  font-style: italic;
}

.suggestions {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-color);
}

.suggestions h5 {
  font-size: 11px;
  font-weight: 600;
  margin: 0 0 6px 0;
  color: var(--text-color);
}

.suggestions ul {
  margin: 0;
  padding-left: 16px;
  font-size: 11px;
  color: var(--text-secondary);
}

.suggestions li {
  margin-bottom: 2px;
}

.task-error {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--color-error);
  color: white;
  border-radius: 6px;
  margin-bottom: 12px;
  font-size: 12px;
}

.error-icon {
  font-size: 14px;
}

.view-details-btn {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.view-details-btn:hover {
  background: var(--hover-bg);
  color: var(--text-color);
}
</style>
