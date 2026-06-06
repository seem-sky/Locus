<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { t } from "../../i18n";
import {
  agentmemoryActionList,
  agentmemoryActionUpdate,
  agentmemoryConsolidate,
  agentmemoryInsights,
  agentmemoryStart,
  agentmemoryStatus,
  agentmemoryStop,
  isAgentMemoryPatternNoise,
  memoryCreate,
  memoryDelete,
  memoryList,
  memoryPin,
  memoryUpdate,
  parseSessionRows,
  type AgentMemoryAction,
  type AgentMemoryInsights,
  type AgentMemorySessionRow,
  type AgentMemoryStatus,
} from "../../services/memory";
import { openUrl } from "@tauri-apps/plugin-opener";
import { normalizeAppError } from "../../services/errors";
import { useNotificationStore } from "../../stores/notification";
import { useProjectStore } from "../../stores/project";
import type { MemoryCategory, MemoryEntry, MemoryScope } from "../../types";
import WorkspaceRequiredState from "../WorkspaceRequiredState.vue";
import BaseButton from "../ui/BaseButton.vue";
import BaseSegmented from "../ui/BaseSegmented.vue";

const project = useProjectStore();
const notificationStore = useNotificationStore();

const hasWorkspace = computed(() => !!project.workingDir.trim());
const loading = ref(false);
const error = ref("");
const entries = ref<MemoryEntry[]>([]);
const activeCategory = ref<MemoryCategory | "all">("all");
const activeTag = ref<string | null>(null);
const searchQuery = ref("");

const editingEntry = ref<MemoryEntry | null>(null);
const editModalRef = ref<HTMLElement | null>(null);
const editContent = ref("");
const editTags = ref("");
const editSaving = ref(false);
const deletingId = ref<string | null>(null);
const serviceStatus = ref<AgentMemoryStatus | null>(null);
const serviceLoading = ref(false);
const actions = ref<AgentMemoryAction[]>([]);
const actionsLoading = ref(false);
const actionUpdatingId = ref<string | null>(null);
const insights = ref<AgentMemoryInsights | null>(null);
const insightsLoading = ref(false);
const insightsExpanded = ref(false);
const consolidating = ref(false);
let loadEntriesSeq = 0;

const viewerUrl = computed(() => {
  const port = serviceStatus.value?.viewerPort ?? 3113;
  return `http://127.0.0.1:${port}`;
});

async function loadServiceStatus() {
  serviceLoading.value = true;
  try {
    serviceStatus.value = await agentmemoryStatus();
    error.value = "";
  } catch (cause) {
    serviceStatus.value = null;
    const normalized = normalizeAppError(cause);
    if (normalized.message.includes("only available inside the Locus desktop app")) {
      error.value = t("memory.agentmemory.runtimeUnavailable");
    } else {
      error.value = normalized.message;
    }
  } finally {
    serviceLoading.value = false;
  }
}

async function sleep(ms: number) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function pollServiceUntilReady(timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  let lastStatus: AgentMemoryStatus | null = null;
  while (Date.now() < deadline) {
    lastStatus = await agentmemoryStatus();
    serviceStatus.value = lastStatus;
    if (lastStatus.available) {
      return lastStatus;
    }
    await sleep(500);
  }
  throw new Error(
    lastStatus?.error?.trim()
      || t("memory.agentmemory.startTimeout"),
  );
}

async function startService() {
  serviceLoading.value = true;
  error.value = "";
  try {
    serviceStatus.value = await agentmemoryStart();
    const ready = await pollServiceUntilReady();
    serviceStatus.value = ready;
    notificationStore.addNotice("success", t("memory.agentmemory.available"));
    await loadEntries();
  } catch (cause) {
    const normalized = normalizeAppError(cause);
    error.value = normalized.message;
    notificationStore.addNotice("error", normalized.message);
    try {
      serviceStatus.value = await agentmemoryStatus();
    } catch {
      // keep last known status
    }
  } finally {
    serviceLoading.value = false;
  }
}

async function stopService() {
  serviceLoading.value = true;
  try {
    serviceStatus.value = await agentmemoryStop();
    notificationStore.addNotice("success", t("memory.agentmemory.unavailable"));
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  } finally {
    serviceLoading.value = false;
  }
}

async function openViewer() {
  try {
    await openUrl(viewerUrl.value);
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  }
}

const categoryOptions = computed(() => [
  { value: "all", label: t("memory.filter.all") },
  { value: "user", label: t("memory.category.user") },
  { value: "feedback", label: t("memory.category.feedback") },
  { value: "topic", label: t("memory.category.topic") },
  { value: "reference", label: t("memory.category.reference") },
]);

const allTags = computed(() => {
  const tags = new Set<string>();
  for (const entry of entries.value) {
    for (const tag of entry.tags) tags.add(tag);
  }
  return Array.from(tags).sort((a, b) => a.localeCompare(b));
});

const filteredEntries = computed(() => {
  let list = entries.value;
  if (activeCategory.value !== "all") {
    list = list.filter((entry) => entry.category === activeCategory.value);
  }
  if (activeTag.value) {
    list = list.filter((entry) => entry.tags.includes(activeTag.value!));
  }
  const query = searchQuery.value.trim().toLowerCase();
  if (query) {
    list = list.filter((entry) =>
      entry.content.toLowerCase().includes(query)
      || entry.tags.some((tag) => tag.toLowerCase().includes(query)),
    );
  }
  return [...list].sort((left, right) => {
    if (left.pinned !== right.pinned) return left.pinned ? -1 : 1;
    return right.updatedAt - left.updatedAt;
  });
});

function labelForCategory(category: MemoryCategory): string {
  return t(`memory.category.${category}`);
}

function labelForScope(scope: MemoryScope): string {
  return scope === "user" ? t("memory.scope.user") : t("memory.scope.project");
}

function formatTime(ms: number): string {
  if (!ms) return "-";
  return new Date(ms).toLocaleString();
}

async function loadEntries(options?: { force?: boolean }) {
  const seq = ++loadEntriesSeq;
  if (!hasWorkspace.value) {
    entries.value = [];
    error.value = "";
    loading.value = false;
    return;
  }
  if (
    !options?.force
    && serviceStatus.value
    && !serviceStatus.value.available
  ) {
    entries.value = [];
    error.value = serviceStatus.value.error?.trim() || t("memory.agentmemory.unavailable");
    loading.value = false;
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    const nextEntries = await memoryList({ workingDir: project.workingDir });
    if (seq !== loadEntriesSeq) return;
    entries.value = nextEntries;
  } catch (cause) {
    if (seq !== loadEntriesSeq) return;
    entries.value = [];
    error.value = normalizeAppError(cause).message;
  } finally {
    if (seq === loadEntriesSeq) loading.value = false;
  }
}

async function loadActions() {
  if (!hasWorkspace.value || !serviceStatus.value?.available) {
    actions.value = [];
    return;
  }
  actionsLoading.value = true;
  try {
    actions.value = await agentmemoryActionList(project.workingDir);
  } catch (cause) {
    actions.value = [];
    console.warn("[memory-settings] agentmemory_action_list failed:", cause);
  } finally {
    actionsLoading.value = false;
  }
}

async function markActionDone(action: AgentMemoryAction) {
  if (!hasWorkspace.value || actionUpdatingId.value) return;
  actionUpdatingId.value = action.id;
  try {
    const updated = await agentmemoryActionUpdate({
      workingDir: project.workingDir,
      actionId: action.id,
      status: "done",
    });
    actions.value = actions.value.map((item) => (item.id === updated.id ? updated : item));
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  } finally {
    actionUpdatingId.value = null;
  }
}

const pendingActions = computed(() =>
  actions.value.filter((action) => action.status === "pending" || action.status === "active"),
);

const sessionRows = computed<AgentMemorySessionRow[]>(() =>
  parseSessionRows(insights.value?.sessions),
);

const profileSummary = computed(() => {
  const profile = insights.value?.profile;
  if (!profile || typeof profile !== "object") return null;
  const record = profile as Record<string, unknown>;
  const concepts = Array.isArray(record.concepts)
    ? record.concepts.filter((item): item is string => typeof item === "string")
    : [];
  const topFiles = Array.isArray(record.topFiles)
    ? record.topFiles.filter((item): item is string => typeof item === "string")
    : Array.isArray(record.files)
      ? record.files.filter((item): item is string => typeof item === "string")
      : [];
  const summary =
    typeof record.summary === "string"
      ? record.summary
      : typeof record.narrative === "string"
        ? record.narrative
        : null;
  if (!summary && concepts.length === 0 && topFiles.length === 0) {
    return null;
  }
  return { concepts, topFiles, summary };
});

const featureFlagRows = computed(() => {
  const flags = insights.value?.featureFlags ?? [];
  const keys = [
    "CONSOLIDATION_ENABLED",
    "GRAPH_EXTRACTION_ENABLED",
    "AGENTMEMORY_AUTO_COMPRESS",
  ];
  return keys.map((key) => {
    const flag = flags.find((item) => item.key === key);
    return {
      key,
      label: flag?.label ?? key,
      enabled: flag?.enabled ?? false,
      needsLlm: flag?.needsLlm ?? true,
    };
  });
});

const patternItems = computed(() => {
  const patterns = insights.value?.patterns;
  if (!patterns || typeof patterns !== "object") return [] as string[];
  const record = patterns as Record<string, unknown>;
  const list = Array.isArray(record.patterns)
    ? record.patterns
    : Array.isArray(patterns)
      ? patterns
      : [];
  return list
    .map((item) => {
      if (typeof item === "string") return item;
      if (item && typeof item === "object") {
        const row = item as Record<string, unknown>;
        if (typeof row.description === "string") return row.description;
        if (typeof row.title === "string") return row.title;
        if (typeof row.pattern === "string") return row.pattern;
      }
      return null;
    })
    .filter((item): item is string => !!item && item.trim().length > 0)
    .filter((item) => !isAgentMemoryPatternNoise(item));
});

const graphSummary = computed(() => {
  const stats = insights.value?.graphStats;
  if (!stats || typeof stats !== "object") return null;
  const record = stats as Record<string, unknown>;
  const nodes =
    typeof record.nodeCount === "number"
      ? record.nodeCount
      : typeof record.nodes === "number"
        ? record.nodes
        : null;
  const edges =
    typeof record.edgeCount === "number"
      ? record.edgeCount
      : typeof record.edges === "number"
        ? record.edges
        : null;
  const healthy =
    typeof record.healthy === "boolean"
      ? record.healthy
      : typeof record.enabled === "boolean"
        ? record.enabled
        : null;
  if ((nodes ?? 0) === 0 && (edges ?? 0) === 0 && healthy !== true) {
    return null;
  }
  return { nodes: nodes ?? 0, edges: edges ?? 0, healthy };
});

async function loadAdvancedInsights() {
  if (!hasWorkspace.value || !serviceStatus.value?.available) {
    insights.value = null;
    return;
  }
  insightsLoading.value = true;
  try {
    insights.value = await agentmemoryInsights(project.workingDir);
  } catch (cause) {
    insights.value = null;
    console.warn("[memory-settings] agentmemory_insights failed:", cause);
  } finally {
    insightsLoading.value = false;
  }
}

async function runConsolidation() {
  if (consolidating.value || !serviceStatus.value?.available) return;
  consolidating.value = true;
  try {
    await agentmemoryConsolidate({ tier: "all", force: true });
    notificationStore.addNotice("success", t("memory.advanced.consolidateDone"));
    await loadAdvancedInsights();
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  } finally {
    consolidating.value = false;
  }
}

async function reloadEntries() {
  await loadServiceStatus();
  await Promise.all([loadEntries({ force: true }), loadActions(), loadAdvancedInsights()]);
}

function openEdit(entry: MemoryEntry) {
  editingEntry.value = entry;
  editContent.value = entry.content;
  editTags.value = entry.tags.join(", ");
  void nextTick(() => editModalRef.value?.focus());
}

function isEditDirty(): boolean {
  const entry = editingEntry.value;
  if (!entry) return false;
  const tags = editTags.value
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);
  return editContent.value.trim() !== entry.content.trim()
    || tags.join("\0") !== entry.tags.join("\0");
}

function closeEdit(force = false) {
  if (!force && isEditDirty() && !window.confirm(t("memory.editor.discardConfirm"))) {
    return;
  }
  editingEntry.value = null;
  editContent.value = "";
  editTags.value = "";
}

function onEditKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  closeEdit();
}

async function saveEdit() {
  const entry = editingEntry.value;
  if (!entry || !hasWorkspace.value) return;
  editSaving.value = true;
  try {
    const tags = editTags.value
      .split(",")
      .map((tag) => tag.trim())
      .filter(Boolean);
    const updated = await memoryUpdate({
      workingDir: project.workingDir,
      scope: entry.scope,
      id: entry.id,
      content: editContent.value.trim(),
      tags,
    });
    entries.value = entries.value.map((item) => (item.id === updated.id ? updated : item));
    notificationStore.addNotice("success", t("memory.saved"));
    closeEdit(true);
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  } finally {
    editSaving.value = false;
  }
}

async function togglePin(entry: MemoryEntry) {
  if (!hasWorkspace.value) return;
  try {
    const updated = await memoryPin(
      project.workingDir,
      entry.scope,
      entry.id,
      !entry.pinned,
    );
    entries.value = entries.value.map((item) => (item.id === updated.id ? updated : item));
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  }
}

async function confirmDelete(entry: MemoryEntry) {
  if (!hasWorkspace.value) return;
  if (!window.confirm(t("memory.deleteConfirm"))) return;
  deletingId.value = entry.id;
  try {
    await memoryDelete(project.workingDir, entry.scope, entry.id);
    entries.value = entries.value.filter((item) => item.id !== entry.id);
    notificationStore.addNotice("success", t("memory.deleted"));
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  } finally {
    deletingId.value = null;
  }
}

async function createSampleEntry() {
  if (!hasWorkspace.value) return;
  try {
    const created = await memoryCreate({
      workingDir: project.workingDir,
      category: "user",
      content: t("memory.sampleEntry"),
      tags: ["manual"],
    });
    entries.value = [created, ...entries.value];
    notificationStore.addNotice("success", t("memory.saved"));
  } catch (cause) {
    notificationStore.addNotice("error", normalizeAppError(cause).message);
  }
}

watch(() => project.workingDir, () => void reloadEntries(), { immediate: false });
watch(
  () => serviceStatus.value?.available,
  (available, previous) => {
    if (available && previous === false && hasWorkspace.value) {
      void loadEntries({ force: true });
      void loadAdvancedInsights();
    }
  },
);
onMounted(async () => {
  await loadServiceStatus();
  await loadEntries();
  await loadActions();
  await loadAdvancedInsights();
});
</script>

<template>
  <div class="settings-section memory-settings">
    <div class="settings-section-header">
      <div>
        <h2>{{ t("memory.title") }}</h2>
        <p class="settings-section-desc">{{ t("memory.description") }}</p>
      </div>
      <BaseButton variant="neutral" size="sm" :disabled="loading || !hasWorkspace" @click="reloadEntries">
        {{ t("memory.reload") }}
      </BaseButton>
    </div>

    <div class="memory-service-panel">
      <div class="memory-service-main">
        <div class="memory-service-title">{{ t("memory.agentmemory.title") }}</div>
        <div class="memory-service-meta">
          <span
            class="memory-service-badge"
            :class="serviceStatus?.available ? 'ok' : 'warn'"
          >
            {{ serviceStatus?.available ? t("memory.agentmemory.available") : t("memory.agentmemory.unavailable") }}
          </span>
          <span v-if="serviceStatus?.version" class="memory-service-detail">
            {{ t("memory.agentmemory.version", serviceStatus.version) }}
          </span>
          <span v-if="serviceStatus?.baseUrl" class="memory-service-detail">
            {{ t("memory.agentmemory.baseUrl", serviceStatus.baseUrl) }}
          </span>
          <span v-if="serviceStatus?.usingBundledRuntime" class="memory-service-detail">
            {{ t("memory.agentmemory.bundledRuntime") }}
          </span>
          <span v-if="serviceStatus?.bundleVersion" class="memory-service-detail">
            {{ t("memory.agentmemory.bundleVersion", serviceStatus.bundleVersion) }}
          </span>
          <span v-if="serviceStatus?.llmConfigured" class="memory-service-detail">
            {{
              t(
                "memory.agentmemory.llmConfigured",
                serviceStatus.llmProvider || "unknown",
              )
            }}
          </span>
          <span v-else class="memory-service-detail memory-service-warn">
            {{ t("memory.agentmemory.llmNotConfigured") }}
          </span>
          <span
            v-if="serviceStatus?.llmWarning"
            class="memory-service-detail memory-service-warn"
          >
            {{ serviceStatus.llmWarning }}
          </span>
          <span v-if="serviceStatus?.error" class="memory-service-detail memory-service-error">
            {{ serviceStatus.error }}
          </span>
          <span v-else-if="error" class="memory-service-detail memory-service-error">
            {{ error }}
          </span>
        </div>
      </div>
      <div class="memory-service-actions">
        <BaseButton
          variant="neutral"
          size="sm"
          :disabled="serviceLoading || serviceStatus?.available"
          @click="startService"
        >
          {{ t("memory.agentmemory.start") }}
        </BaseButton>
        <BaseButton
          variant="neutral"
          size="sm"
          :disabled="serviceLoading || !serviceStatus?.available"
          @click="stopService"
        >
          {{ t("memory.agentmemory.stop") }}
        </BaseButton>
        <BaseButton
          variant="neutral"
          size="sm"
          :disabled="!serviceStatus?.available"
          @click="openViewer"
        >
          {{ t("memory.agentmemory.openViewer") }}
        </BaseButton>
      </div>
    </div>

    <div
      v-if="hasWorkspace && serviceStatus?.available"
      class="memory-actions-panel"
    >
      <div class="memory-actions-header">
        <div class="memory-service-title">{{ t("memory.actions.title") }}</div>
        <span class="memory-service-detail">
          {{ t("memory.actions.summary", pendingActions.length, actions.length) }}
        </span>
      </div>
      <div v-if="actionsLoading" class="memory-service-detail">{{ t("memory.actions.loading") }}</div>
      <div v-else-if="actions.length === 0" class="memory-service-detail">{{ t("memory.actions.empty") }}</div>
      <ul v-else class="memory-actions-list">
        <li v-for="action in actions" :key="action.id" class="memory-action-item">
          <div class="memory-action-main">
            <div class="memory-action-title">{{ action.title }}</div>
            <div v-if="action.description" class="memory-action-desc">{{ action.description }}</div>
            <div class="memory-action-meta">
              <span class="memory-action-status">{{ action.status }}</span>
            </div>
          </div>
          <BaseButton
            v-if="action.status === 'pending' || action.status === 'active'"
            variant="neutral"
            size="sm"
            :disabled="actionUpdatingId === action.id"
            @click="markActionDone(action)"
          >
            {{ t("memory.actions.markDone") }}
          </BaseButton>
        </li>
      </ul>
    </div>

    <div
      v-if="hasWorkspace && serviceStatus?.available"
      class="memory-advanced-panel"
    >
      <div class="memory-advanced-header">
        <button
          type="button"
          class="memory-advanced-toggle"
          @click="insightsExpanded = !insightsExpanded"
        >
          <span class="memory-advanced-toggle-main">
            <span class="memory-service-title">{{ t("memory.advanced.title") }}</span>
            <span class="memory-advanced-chevron">{{ insightsExpanded ? "▾" : "▸" }}</span>
          </span>
        </button>
        <div class="memory-advanced-toolbar">
          <BaseButton
            variant="neutral"
            size="sm"
            :disabled="insightsLoading"
            @click="loadAdvancedInsights"
          >
            {{ t("memory.advanced.refresh") }}
          </BaseButton>
          <BaseButton
            variant="neutral"
            size="sm"
            :disabled="consolidating"
            @click="runConsolidation"
          >
            {{ consolidating ? t("memory.advanced.consolidating") : t("memory.advanced.consolidate") }}
          </BaseButton>
        </div>
      </div>

      <div v-if="insightsExpanded">
        <div v-if="insightsLoading" class="memory-service-detail">{{ t("memory.advanced.loading") }}</div>
        <template v-else>
          <div
            v-if="insights?.errors?.length"
            class="memory-service-detail memory-service-warn"
          >
            {{ insights.errors.join(" · ") }}
          </div>

          <div v-if="featureFlagRows.length" class="memory-advanced-flags">
            <span
              v-for="flag in featureFlagRows"
              :key="flag.key"
              class="memory-tag-chip"
              :class="flag.enabled ? 'is-on' : 'is-off'"
              :title="flag.key"
            >
              {{ flag.label }}: {{ flag.enabled ? "ON" : "OFF" }}
            </span>
          </div>

          <div class="memory-advanced-grid">
            <section class="memory-advanced-card sessions-card">
              <h3>{{ t("memory.advanced.sessions") }}</h3>
              <div v-if="sessionRows.length === 0" class="memory-service-detail">
                {{ t("memory.advanced.sessionsEmpty") }}
              </div>
              <ul v-else class="memory-advanced-list">
                <li v-for="session in sessionRows.slice(0, 8)" :key="session.id">
                  <div class="memory-advanced-row-title">
                    {{ session.title || session.id }}
                  </div>
                  <div class="memory-service-detail">
                    <span>{{ session.status || "-" }}</span>
                    <span v-if="session.observationCount != null">
                      · {{ t("memory.advanced.observations", session.observationCount) }}
                    </span>
                  </div>
                </li>
              </ul>
            </section>

            <section class="memory-advanced-card profile-card">
              <h3>{{ t("memory.advanced.profile") }}</h3>
              <div v-if="!profileSummary" class="memory-service-detail">
                {{ t("memory.advanced.profileEmpty") }}
              </div>
              <template v-else>
                <p v-if="profileSummary.summary" class="memory-advanced-text">
                  {{ profileSummary.summary }}
                </p>
                <div v-if="profileSummary.concepts.length" class="memory-advanced-tags">
                  <span
                    v-for="concept in profileSummary.concepts.slice(0, 12)"
                    :key="concept"
                    class="memory-tag-chip"
                  >
                    {{ concept }}
                  </span>
                </div>
                <ul v-if="profileSummary.topFiles.length" class="memory-advanced-list compact">
                  <li v-for="file in profileSummary.topFiles.slice(0, 6)" :key="file">{{ file }}</li>
                </ul>
              </template>
            </section>

            <section class="memory-advanced-card graph-card">
              <h3>{{ t("memory.advanced.graph") }}</h3>
              <div v-if="!graphSummary" class="memory-service-detail">
                {{ t("memory.advanced.graphEmpty") }}
              </div>
              <div v-else class="memory-advanced-metrics">
                <div v-if="graphSummary.nodes != null">
                  {{ t("memory.advanced.nodes", graphSummary.nodes) }}
                </div>
                <div v-if="graphSummary.edges != null">
                  {{ t("memory.advanced.edges", graphSummary.edges) }}
                </div>
                <div v-if="graphSummary.healthy != null">
                  {{
                    graphSummary.healthy
                      ? t("memory.advanced.graphHealthy")
                      : t("memory.advanced.graphDisabled")
                  }}
                </div>
              </div>
            </section>

            <section v-if="patternItems.length" class="memory-advanced-card wide">
              <h3>{{ t("memory.advanced.patterns") }}</h3>
              <ul class="memory-advanced-list">
                <li v-for="(pattern, index) in patternItems.slice(0, 6)" :key="index">
                  {{ pattern }}
                </li>
              </ul>
            </section>
          </div>
        </template>
      </div>
    </div>

    <WorkspaceRequiredState v-if="!hasWorkspace" />

    <template v-else>
      <div v-if="error" class="memory-settings-error">{{ error }}</div>

      <div class="memory-settings-toolbar">
        <BaseSegmented
          v-model="activeCategory"
          size="sm"
          :options="categoryOptions"
        />
        <input
          v-model="searchQuery"
          class="memory-settings-search"
          type="search"
          :placeholder="t('memory.searchPlaceholder')"
        />
      </div>

      <div v-if="allTags.length > 0" class="memory-settings-tags">
        <button
          type="button"
          class="memory-tag-chip"
          :class="{ active: !activeTag }"
          @click="activeTag = null"
        >
          {{ t("memory.filter.allTags") }}
        </button>
        <button
          v-for="tag in allTags"
          :key="tag"
          type="button"
          class="memory-tag-chip"
          :class="{ active: activeTag === tag }"
          @click="activeTag = activeTag === tag ? null : tag"
        >
          {{ tag }}
        </button>
      </div>

      <div v-if="loading && entries.length === 0" class="memory-settings-empty">
        {{ t("memory.loading") }}
      </div>
      <div v-else-if="filteredEntries.length === 0" class="memory-settings-empty">
        <p>{{ t("memory.emptyEntries") }}</p>
        <BaseButton variant="neutral" size="sm" @click="createSampleEntry">
          {{ t("memory.createFirst") }}
        </BaseButton>
      </div>

      <div v-else class="memory-entry-list">
        <article v-for="entry in filteredEntries" :key="`${entry.scope}:${entry.id}`" class="memory-entry-row">
          <div class="memory-entry-main">
            <div class="memory-entry-meta">
              <span class="memory-entry-category">{{ labelForCategory(entry.category) }}</span>
              <span class="memory-entry-scope">{{ labelForScope(entry.scope) }}</span>
              <span v-if="entry.pinned" class="memory-entry-pinned">{{ t("memory.pinned") }}</span>
            </div>
            <div class="memory-entry-content">{{ entry.content }}</div>
            <div v-if="entry.tags.length > 0" class="memory-entry-tags">
              <span v-for="tag in entry.tags" :key="tag" class="memory-entry-tag">{{ tag }}</span>
            </div>
            <div class="memory-entry-stats">
              <span>{{ t("memory.accessCount", entry.accessCount) }}</span>
              <span>{{ t("memory.updatedAt", formatTime(entry.updatedAt)) }}</span>
              <span v-if="entry.linkedDocPath" class="memory-entry-link">{{ entry.linkedDocPath }}</span>
            </div>
          </div>
          <div class="memory-entry-actions">
            <BaseButton variant="neutral" size="sm" @click="togglePin(entry)">
              {{ entry.pinned ? t("memory.unpin") : t("memory.pin") }}
            </BaseButton>
            <BaseButton variant="neutral" size="sm" @click="openEdit(entry)">
              {{ t("memory.edit") }}
            </BaseButton>
            <BaseButton
              variant="danger"
              size="sm"
              :disabled="deletingId === entry.id"
              @click="confirmDelete(entry)"
            >
              {{ t("memory.delete") }}
            </BaseButton>
          </div>
        </article>
      </div>
    </template>

    <div v-if="editingEntry" class="memory-edit-modal-backdrop">
      <div
        ref="editModalRef"
        class="memory-edit-modal"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
        :aria-label="t('memory.editTitle')"
        @keydown="onEditKeydown"
      >
        <h3>{{ t("memory.editTitle") }}</h3>
        <textarea v-model="editContent" class="memory-edit-textarea" rows="8" />
        <label class="memory-edit-label">{{ t("memory.tagsLabel") }}</label>
        <input v-model="editTags" class="memory-edit-input" type="text" />
        <div class="memory-edit-actions">
          <BaseButton variant="neutral" @click="closeEdit">{{ t("common.cancel") }}</BaseButton>
          <BaseButton variant="primary" :disabled="editSaving || !editContent.trim()" @click="saveEdit">
            {{ editSaving ? t("memory.editor.saving") : t("memory.save") }}
          </BaseButton>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.memory-settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
  max-width: 100%;
}

.settings-section-header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
}

.settings-section-desc {
  margin: 6px 0 0;
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.45;
}

.memory-settings-error {
  padding: 10px 12px;
  border: 1px solid var(--status-danger-border, var(--border-color));
  border-radius: 6px;
  color: var(--status-danger-fg);
  font-size: 13px;
}

.memory-actions-panel {
  margin-bottom: 16px;
  padding: 12px 14px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel-bg) 92%, var(--bg-color) 8%);
}

.memory-advanced-panel {
  margin-bottom: 16px;
  padding: 12px 14px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel-bg) 94%, var(--bg-color) 6%);
  min-width: 0;
  overflow: hidden;
}

.memory-advanced-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.memory-advanced-toggle {
  flex: 1 1 160px;
  min-width: 0;
  display: flex;
  align-items: center;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  text-align: left;
}

.memory-advanced-toggle-main {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  max-width: 100%;
}

.memory-advanced-chevron {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-advanced-toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  flex: 0 1 auto;
  justify-content: flex-end;
}

.memory-advanced-flags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}

.memory-advanced-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  margin-top: 12px;
  width: 100%;
  min-width: 0;
  align-items: start;
}

.memory-advanced-card {
  padding: 10px 12px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: color-mix(in srgb, var(--panel-bg) 90%, var(--bg-color) 10%);
  min-width: 0;
  width: 100%;
  box-sizing: border-box;
  overflow: hidden;
}

.memory-advanced-card.wide {
  grid-column: 1 / -1;
}

.memory-advanced-card h3 {
  margin: 0 0 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.memory-advanced-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.memory-advanced-list > li {
  min-width: 0;
  word-break: break-word;
}

.memory-advanced-list.compact {
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-advanced-list.compact > li {
  word-break: break-all;
}

.memory-advanced-row-title {
  font-size: 13px;
  font-weight: 600;
  word-break: break-word;
}

.memory-advanced-text {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: pre-wrap;
}

.memory-advanced-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
}

.memory-advanced-metrics {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 14px;
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-advanced-card.graph-card .memory-advanced-metrics {
  row-gap: 4px;
}

.memory-actions-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
}

.memory-actions-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.memory-action-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
}

.memory-action-main {
  min-width: 0;
  flex: 1;
}

.memory-action-title {
  font-size: 13px;
  font-weight: 600;
}

.memory-action-desc {
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
}

.memory-action-meta {
  margin-top: 6px;
  font-size: 11px;
  color: var(--text-secondary);
}

.memory-service-panel {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
  margin-bottom: 16px;
  padding: 12px 14px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel-bg) 92%, var(--bg-color) 8%);
}

.memory-service-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.memory-service-title {
  font-size: 13px;
  font-weight: 600;
}

.memory-service-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.memory-service-badge {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid var(--border-color);
}

.memory-service-badge.ok {
  color: var(--accent-color);
  border-color: color-mix(in srgb, var(--accent-color) 35%, var(--border-color));
}

.memory-service-badge.warn {
  color: var(--text-secondary);
}

.memory-service-detail {
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-service-warn {
  color: var(--warning-fg, #b8860b);
}

.memory-service-error {
  color: var(--danger-color, #e06c75);
  max-width: 100%;
  word-break: break-word;
}

.memory-service-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.memory-settings-toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
}

.memory-settings-search {
  flex: 1 1 180px;
  min-width: 0;
  padding: 6px 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-color);
  color: var(--text-color);
  font-size: 13px;
}

.memory-settings-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.memory-tag-chip {
  padding: 4px 8px;
  border: 1px solid var(--border-color);
  border-radius: 999px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.memory-tag-chip.active {
  border-color: color-mix(in srgb, var(--accent-color) 40%, transparent);
  color: var(--accent-color);
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
}

.memory-tag-chip.is-on {
  border-color: color-mix(in srgb, var(--status-good-fg, #3dd68c) 35%, var(--border-color));
  color: var(--status-good-fg, #3dd68c);
  background: color-mix(in srgb, var(--status-good-fg, #3dd68c) 8%, transparent);
  cursor: default;
}

.memory-tag-chip.is-off {
  border-color: var(--border-color);
  color: var(--text-tertiary, var(--text-secondary));
  opacity: 0.85;
  cursor: default;
}

.memory-settings-empty {
  padding: 24px;
  text-align: center;
  color: var(--text-secondary);
  font-size: 13px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.memory-entry-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.memory-entry-row {
  display: flex;
  gap: 12px;
  justify-content: space-between;
  padding: 12px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel-bg) 90%, var(--bg-color) 10%);
}

.memory-entry-main {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.memory-entry-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  font-size: 11px;
}

.memory-entry-category {
  color: var(--accent-color);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.memory-entry-scope,
.memory-entry-pinned {
  color: var(--text-secondary);
}

.memory-entry-pinned {
  color: var(--accent-color);
}

.memory-entry-content {
  font-size: 13px;
  line-height: 1.45;
  white-space: pre-wrap;
  word-break: break-word;
}

.memory-entry-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.memory-entry-tag {
  padding: 2px 6px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--bg-color) 80%, transparent);
  font-size: 11px;
  color: var(--text-secondary);
}

.memory-entry-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 11px;
  color: var(--text-secondary);
}

.memory-entry-link {
  font-family: var(--font-mono-identifier);
}

.memory-entry-actions {
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.memory-edit-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(0, 0, 0, 0.35);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.memory-edit-modal {
  width: min(520px, 100%);
  padding: 16px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--panel-bg);
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.memory-edit-modal:focus {
  outline: none;
}

.memory-edit-modal h3 {
  margin: 0;
  font-size: 15px;
}

.memory-edit-textarea,
.memory-edit-input {
  width: 100%;
  padding: 8px 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-color);
  color: var(--text-color);
  font-size: 13px;
  font-family: inherit;
}

.memory-edit-label {
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-edit-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

@media (max-width: 860px) {
  .memory-service-panel {
    flex-direction: column;
    align-items: stretch;
  }

  .memory-service-actions {
    width: 100%;
  }

  .memory-advanced-header {
    flex-direction: column;
    align-items: stretch;
  }

  .memory-advanced-toggle {
    flex: none;
    width: 100%;
  }

  .memory-advanced-toolbar {
    width: 100%;
    justify-content: flex-start;
  }

  .memory-action-item {
    flex-direction: column;
    align-items: stretch;
  }
}

/* 高级洞察三栏：宽屏三等分，中屏 2+1，窄屏单列 */
@media (max-width: 1080px) {
  .memory-advanced-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .memory-advanced-card.graph-card {
    grid-column: 1 / -1;
  }
}

@media (max-width: 640px) {
  .memory-advanced-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .memory-advanced-card.graph-card {
    grid-column: auto;
  }
}
</style>
