<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { storeToRefs } from "pinia";
import { t } from "../../i18n";
import { normalizeAppError } from "../../services/errors";
import { listArchivedSessions, loadSession, unarchiveSession } from "../../services/session";
import type { ChatMessage, SessionDetail, SessionSummary } from "../../types";
import { useAgentStore } from "../../stores/agent";
import { useNotificationStore } from "../../stores/notification";
import { useChatStore } from "../../stores/chat";
import {
  buildSessionTree,
  nodeContainsSession,
  type SessionTreeNode,
} from "../chat/sessionTree";
import MarkdownRenderer from "../MarkdownRenderer.vue";
import BaseButton from "../ui/BaseButton.vue";

type VisibleArchivedRow = {
  node: SessionTreeNode;
  depth: number;
  expanded: boolean;
  hasChildren: boolean;
};

const notificationStore = useNotificationStore();
const chatStore = useChatStore();
const agentStore = useAgentStore();
const { agents, subagents } = storeToRefs(agentStore);

const agentNameById = computed(() => {
  const map = new Map<string, string>();
  for (const agent of [...agents.value, ...subagents.value]) {
    map.set(agent.id, agent.name);
  }
  return map;
});

const archivedSessions = ref<SessionSummary[]>([]);
const selectedSessionId = ref<string | null>(null);
const selectedDetail = ref<SessionDetail | null>(null);
const listLoading = ref(false);
const detailLoading = ref(false);
const archivedLoadFailed = ref(false);
const unarchivingIds = ref<Set<string>>(new Set());
const expandedState = ref<Record<string, boolean>>({});

let refreshSeq = 0;
let detailSeq = 0;

const sessionTree = computed(() => buildSessionTree({ sessions: archivedSessions.value }));

const selectedSummary = computed(() =>
  archivedSessions.value.find((session) => session.id === selectedSessionId.value) ?? null,
);

function isNodeExpanded(node: SessionTreeNode): boolean {
  const stored = expandedState.value[node.key];
  if (stored !== undefined) return stored;
  return nodeContainsSession(node, selectedSessionId.value);
}

function setNodeExpanded(key: string, value: boolean) {
  expandedState.value = { ...expandedState.value, [key]: value };
}

function toggleNode(row: VisibleArchivedRow) {
  setNodeExpanded(row.node.key, !row.expanded);
}

const visibleRows = computed<VisibleArchivedRow[]>(() => {
  const rows: VisibleArchivedRow[] = [];
  const walk = (nodes: SessionTreeNode[], depth: number) => {
    for (const node of nodes) {
      const expanded = isNodeExpanded(node);
      const hasChildren = node.children.length > 0;
      rows.push({ node, depth, expanded, hasChildren });
      if (hasChildren && expanded) {
        walk(node.children, depth + 1);
      }
    }
  };
  walk(sessionTree.value, 0);
  return rows;
});

function rowLabel(node: SessionTreeNode): string {
  if (node.kind === "folder") return node.label;
  return node.title || t("chat.session.newSession");
}

function isSubagentNode(node: SessionTreeNode): boolean {
  return node.kind === "session" && node.sessionType === "chat" && !!node.parentSessionId;
}

function rowRoleClass(node: SessionTreeNode): string {
  if (node.kind === "folder") return "role-folder";
  if (isSubagentNode(node)) return "role-subagent";
  return `role-${node.sessionType}`;
}

function archivedRowSessionId(row: VisibleArchivedRow): string | null {
  if (row.node.kind !== "session" || !row.node.sessionId) return null;
  return row.node.sessionId;
}

function rowAgentId(row: VisibleArchivedRow): string | null {
  if (row.node.kind !== "session") return null;
  const agentId = row.node.agentId?.trim();
  return agentId || null;
}

function agentDisplayLabel(agentId: string | null | undefined): string {
  if (!agentId) return "";
  return agentNameById.value.get(agentId) ?? agentId;
}

function shouldShowAgentBadge(row: VisibleArchivedRow): boolean {
  return isSubagentNode(row.node) && !!rowAgentId(row);
}

function canUnarchiveSession(session: SessionSummary | null): boolean {
  return !!session && !session.parentSessionId;
}

function canUnarchiveRow(row: VisibleArchivedRow): boolean {
  const id = archivedRowSessionId(row);
  if (!id) return false;
  const summary = archivedSessions.value.find((session) => session.id === id);
  return canUnarchiveSession(summary ?? null);
}

const selectedAgentLabel = computed(() => {
  const agentId = selectedSummary.value?.agentId?.trim();
  if (!agentId || !selectedSummary.value?.parentSessionId) return null;
  return agentDisplayLabel(agentId);
});

function onArchivedRowClick(row: VisibleArchivedRow) {
  if (row.node.kind === "folder") {
    if (row.hasChildren) toggleNode(row);
    return;
  }
  const sessionId = archivedRowSessionId(row);
  if (!sessionId) return;
  void selectArchivedSession(sessionId);
}

function formatDateTime(timestamp: number): string {
  if (!timestamp) return "";
  return new Date(timestamp * 1000).toLocaleString();
}

function formatSessionTime(timestamp: number): string {
  const nowTs = Math.floor(Date.now() / 1000);
  const diff = Math.max(0, nowTs - timestamp);

  if (diff < 60) return t("common.timeJustNow");

  const units: Array<[number, string]> = [
    [60, "chat.session.time.minute"],
    [60 * 60, "chat.session.time.hour"],
    [60 * 60 * 24, "chat.session.time.day"],
    [60 * 60 * 24 * 7, "chat.session.time.week"],
    [60 * 60 * 24 * 30, "chat.session.time.month"],
    [60 * 60 * 24 * 365, "chat.session.time.year"],
  ];

  for (let i = units.length - 1; i >= 0; i--) {
    const [seconds, key] = units[i];
    if (diff >= seconds) {
      return t(key, Math.floor(diff / seconds));
    }
  }

  return t("common.timeJustNow");
}

function roleLabel(role: ChatMessage["role"]): string {
  switch (role) {
    case "user":
      return t("settings.archived.role.user");
    case "assistant":
      return t("settings.archived.role.assistant");
    case "tool":
      return t("settings.archived.role.tool");
  }
}

function isUnarchiving(sessionId: string): boolean {
  return unarchivingIds.value.has(sessionId);
}

async function loadArchivedDetail(sessionId: string | null) {
  if (!sessionId) {
    selectedDetail.value = null;
    return;
  }

  const seq = ++detailSeq;
  detailLoading.value = true;

  try {
    const detail = await loadSession(sessionId);
    if (seq !== detailSeq) return;
    selectedDetail.value = detail;
  } catch (e) {
    if (seq !== detailSeq) return;
    selectedDetail.value = null;
    const err = normalizeAppError(e);
    notificationStore.addNotice("error", t("settings.archived.loadFailed", err.message), {
      code: err.code,
      operation: "loadArchivedSessionDetail",
    });
  } finally {
    if (seq === detailSeq) {
      detailLoading.value = false;
    }
  }
}

async function refreshArchived(options?: { preserveSelection?: boolean }) {
  const preserveSelection = options?.preserveSelection ?? true;
  const seq = ++refreshSeq;
  listLoading.value = true;

  try {
    const sessions = await listArchivedSessions();
    if (seq !== refreshSeq) return;
    archivedLoadFailed.value = false;
    archivedSessions.value = sessions;

    const hasCurrent =
      preserveSelection &&
      !!selectedSessionId.value &&
      sessions.some((session) => session.id === selectedSessionId.value);
    const nextId = hasCurrent ? selectedSessionId.value : sessions[0]?.id ?? null;
    selectedSessionId.value = nextId;
    await loadArchivedDetail(nextId);
  } catch (e) {
    if (seq !== refreshSeq) return;
    archivedLoadFailed.value = true;
    const err = normalizeAppError(e);
    notificationStore.addNotice("error", t("settings.archived.loadFailed", err.message), {
      code: err.code,
      operation: "loadArchivedSessions",
    });
  } finally {
    if (seq === refreshSeq) {
      listLoading.value = false;
    }
  }
}

async function selectArchivedSession(sessionId: string) {
  if (sessionId === selectedSessionId.value) return;
  selectedSessionId.value = sessionId;
  await loadArchivedDetail(sessionId);
}

async function handleUnarchive(sessionId: string) {
  if (!sessionId || isUnarchiving(sessionId)) return;

  const summary = archivedSessions.value.find((session) => session.id === sessionId);
  if (!canUnarchiveSession(summary ?? null)) return;

  const next = new Set(unarchivingIds.value);
  next.add(sessionId);
  unarchivingIds.value = next;

  try {
    await unarchiveSession(sessionId);
    await Promise.all([
      refreshArchived(),
      chatStore.refreshSessions(),
    ]);
    notificationStore.addNotice("success", t("chat.session.unarchived"), {
      operation: "unarchiveSession",
    });
  } catch (e) {
    const err = normalizeAppError(e);
    const isChildUnarchive = err.code === "session.unarchive_child";
    notificationStore.addNotice(
      "error",
      isChildUnarchive
        ? t("chat.session.unarchiveChildBlocked")
        : t("settings.archived.unarchiveFailed", err.message),
      {
        code: err.code,
        operation: "unarchiveSession",
      },
    );
  } finally {
    const current = new Set(unarchivingIds.value);
    current.delete(sessionId);
    unarchivingIds.value = current;
  }
}

onMounted(() => {
  void agentStore.loadAgents();
  void refreshArchived({ preserveSelection: false });
});
</script>

<template>
  <div class="settings-section">
    <div class="section-label">{{ t("settings.archived.title") }}</div>
    <p class="section-desc">{{ t("settings.archived.desc") }}</p>

    <div class="archived-shell">
      <section class="archived-panel archived-list-panel">
        <div class="archived-panel-header">
          <span class="archived-panel-title">{{ t("settings.archived.listTitle") }}</span>
          <BaseButton class="archived-refresh-btn" :disabled="listLoading" @click="refreshArchived()">
            {{ t("settings.archived.refresh") }}
          </BaseButton>
        </div>

        <div v-if="listLoading && archivedSessions.length === 0" class="archived-empty">
          {{ t("common.loading") }}
        </div>
        <div v-else-if="archivedLoadFailed && archivedSessions.length === 0" class="archived-empty">
          <div class="archived-empty-title">{{ t("settings.archived.loadFailedTitle") }}</div>
          <div class="archived-empty-desc">{{ t("settings.archived.loadFailedDesc") }}</div>
          <BaseButton class="archived-refresh-btn" :disabled="listLoading" @click="refreshArchived({ preserveSelection: false })">
            {{ t("common.refresh") }}
          </BaseButton>
        </div>
        <div v-else-if="archivedSessions.length === 0" class="archived-empty">
          <div class="archived-empty-title">{{ t("settings.archived.empty") }}</div>
          <div class="archived-empty-desc">{{ t("settings.archived.emptyDesc") }}</div>
        </div>
        <div v-else class="archived-list">
          <button
            v-for="row in visibleRows"
            :key="row.node.key"
            type="button"
            class="archived-tree-row"
            :class="[
              rowRoleClass(row.node),
              {
                active: archivedRowSessionId(row) === selectedSessionId,
                child: row.depth > 0,
                folder: row.node.kind === 'folder',
                expandable: row.hasChildren,
              },
            ]"
            :style="{ paddingLeft: `${6 + row.depth * 12}px` }"
            @click="onArchivedRowClick(row)"
          >
            <button
              v-if="row.hasChildren"
              type="button"
              class="archived-expand-btn"
              :class="{ open: row.expanded }"
              :title="row.expanded ? t('chat.session.collapse') : t('chat.session.expand')"
              @click.stop="toggleNode(row)"
            >
              <svg viewBox="0 0 12 12" width="10" height="10" fill="currentColor" aria-hidden="true">
                <path d="M4 2.5 8 6 4 9.5z" />
              </svg>
            </button>
            <span v-else class="archived-row-spacer" aria-hidden="true">
              <span class="archived-row-dot"></span>
            </span>

            <div class="archived-session-info">
              <div class="archived-session-main">
                <span class="archived-session-title">{{ rowLabel(row.node) }}</span>
                <span
                  v-if="shouldShowAgentBadge(row)"
                  class="archived-agent-badge"
                  :title="t('chat.session.subagentBadgeTitle', rowAgentId(row)!)"
                >{{ agentDisplayLabel(rowAgentId(row)) }}</span>
                <div class="archived-session-meta">
                  <span class="archived-session-time">{{ formatSessionTime(row.node.updatedAt) }}</span>
                  <button
                    v-if="canUnarchiveRow(row)"
                    type="button"
                    class="archived-row-unarchive-btn"
                    :title="t('settings.archived.unarchive')"
                    :disabled="isUnarchiving(archivedRowSessionId(row)!)"
                    @click.stop="handleUnarchive(archivedRowSessionId(row)!)"
                  >
                    <svg viewBox="0 0 16 16" width="12" height="12" fill="none" aria-hidden="true">
                      <path d="M4 5.25h6.5a2 2 0 0 1 0 4H6.6m0 0 1.8-1.8m-1.8 1.8 1.8 1.8M2.75 3.5V7h3.5" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                  </button>
                </div>
              </div>
            </div>
          </button>
        </div>
      </section>

      <section class="archived-panel archived-preview-panel">
        <div class="archived-panel-header">
          <span class="archived-panel-title">{{ t("settings.archived.previewTitle") }}</span>
          <div class="archived-preview-toolbar">
            <span v-if="selectedDetail" class="archived-panel-meta">
              {{ t("settings.archived.messageCount", selectedDetail.messages.length) }}
            </span>
            <BaseButton
              v-if="selectedSummary && canUnarchiveSession(selectedSummary)"
              class="archived-preview-action"
              :disabled="isUnarchiving(selectedSummary.id)"
              @click="handleUnarchive(selectedSummary.id)"
            >
              {{ t("settings.archived.unarchive") }}
            </BaseButton>
          </div>
        </div>

        <div v-if="detailLoading" class="archived-preview-empty">{{ t("common.loading") }}</div>
        <template v-else-if="selectedDetail">
          <div class="archived-preview-header">
            <div class="archived-preview-title-row">
              <div class="archived-preview-title">{{ selectedDetail.title || t("chat.session.newSession") }}</div>
              <span
                v-if="selectedAgentLabel"
                class="archived-agent-badge archived-agent-badge--preview"
                :title="t('chat.session.subagentBadgeTitle', selectedSummary?.agentId ?? '')"
              >{{ selectedAgentLabel }}</span>
            </div>
            <div class="archived-preview-meta">
              {{ t("settings.archived.archivedAt", formatDateTime(selectedSummary?.updatedAt ?? selectedDetail.updatedAt)) }}
            </div>
          </div>

          <div v-if="selectedDetail.messages.length === 0" class="archived-preview-empty">
            {{ t("settings.archived.emptyPreview") }}
          </div>
          <div v-else class="archived-message-list">
            <article
              v-for="message in selectedDetail.messages"
              :key="message.id"
              class="archived-message"
              :class="`role-${message.role}`"
            >
              <header class="archived-message-header">
                <span class="archived-message-role">{{ roleLabel(message.role) }}</span>
                <span class="archived-message-time">{{ formatDateTime(message.createdAt) }}</span>
              </header>
              <div class="archived-message-body">
                <pre v-if="message.role === 'tool'" class="archived-tool-output">{{ message.content }}</pre>
                <MarkdownRenderer v-else :content="message.content" />
              </div>
            </article>
          </div>
        </template>
        <div v-else class="archived-preview-empty">{{ t("settings.archived.emptyPreview") }}</div>

      </section>
    </div>
  </div>
</template>

<style scoped>
.archived-shell {
  display: grid;
  grid-template-columns: minmax(248px, 280px) minmax(0, 1fr);
  gap: 12px;
  min-height: clamp(540px, calc(100vh - 220px), 720px);
  align-items: stretch;
}

.archived-panel {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  overflow: hidden;
  min-height: 0;
}

.archived-list-panel {
  background: var(--sidebar-bg);
}

.archived-preview-panel {
  background: var(--panel-bg);
}

.archived-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 40px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border-color);
}

.archived-panel-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-color);
}

.archived-panel-meta {
  font-size: 11px;
  color: var(--text-secondary);
}

.archived-preview-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
}

.archived-refresh-btn,
.archived-preview-action {
  min-height: 26px;
}

.archived-list {
  flex: 1 1 0;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 2px 6px 10px;
}

.archived-tree-row {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 0;
  padding: 4px 6px;
  border: 1px solid transparent;
  border-radius: 4px;
  background: transparent;
  box-shadow: none;
  color: inherit;
  text-align: left;
  cursor: pointer;
  position: relative;
  overflow: hidden;
  transition: background 0.12s ease, border-color 0.12s ease;
}

@supports (content-visibility: auto) {
  .archived-tree-row {
    content-visibility: auto;
    contain-intrinsic-size: auto 34px;
  }
}

.archived-tree-row + .archived-tree-row {
  margin-top: 2px;
}

.archived-tree-row:hover {
  background: var(--hover-bg);
}

.archived-tree-row.active {
  background: color-mix(in srgb, var(--active-bg) 78%, var(--sidebar-bg));
  border-color: color-mix(in srgb, var(--accent-color) 18%, transparent);
}

.archived-tree-row.child {
  position: relative;
}

.archived-tree-row.folder .archived-session-title {
  font-weight: 600;
}

.archived-tree-row.role-subagent .archived-session-title {
  font-weight: 500;
  color: color-mix(in srgb, var(--text-color) 88%, var(--text-secondary));
}

.archived-expand-btn,
.archived-row-spacer {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.archived-expand-btn {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  border-radius: 3px;
  cursor: pointer;
  padding: 0;
  box-shadow: none;
  opacity: 0.5;
  margin-right: 2px;
}

.archived-tree-row:hover .archived-expand-btn {
  opacity: 1;
}

.archived-expand-btn:hover {
  background: var(--hover-bg);
  color: var(--text-color);
}

.archived-expand-btn svg {
  transition: transform 0.15s ease;
}

.archived-expand-btn.open svg {
  transform: rotate(90deg);
}

.archived-row-spacer {
  margin-right: 0;
}

.archived-row-dot {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--text-secondary) 36%, transparent);
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--text-secondary) 20%, transparent);
}

.archived-session-info {
  min-width: 0;
  flex: 1;
}

.archived-session-main {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  width: 100%;
}

.archived-session-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-color);
  line-height: 1.35;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1 1 auto;
  min-width: 0;
}

.archived-agent-badge {
  flex-shrink: 0;
  max-width: 96px;
  padding: 1px 6px;
  border-radius: 999px;
  border: 1px solid color-mix(in srgb, var(--accent-color) 24%, var(--border-color));
  background: color-mix(in srgb, var(--accent-color) 10%, var(--sidebar-bg));
  color: color-mix(in srgb, var(--accent-color) 72%, var(--text-color));
  font-size: 10px;
  font-weight: 600;
  line-height: 1.35;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.archived-agent-badge--preview {
  max-width: 140px;
  font-size: 11px;
}

.archived-preview-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.archived-preview-title-row .archived-preview-title {
  flex: 1 1 auto;
  min-width: 0;
}

.archived-session-meta {
  margin-left: auto;
  min-width: 0;
  display: flex;
  align-items: center;
  position: relative;
  flex-shrink: 0;
}

.archived-session-time {
  font-size: 11px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  padding-left: 8px;
  opacity: 0.68;
  transition: opacity 0.12s ease;
}

.archived-row-unarchive-btn {
  position: absolute;
  right: 0;
  top: 50%;
  z-index: 2;
  width: 18px;
  height: 18px;
  min-width: 18px;
  padding: 0;
  border: 1px solid color-mix(in srgb, var(--border-color) 75%, transparent);
  border-radius: 4px;
  background: color-mix(in srgb, var(--sidebar-bg) 92%, var(--hover-bg));
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  pointer-events: none;
  box-shadow: none;
  transform: translateY(-50%) scale(0.92);
  transition: opacity 0.12s ease, transform 0.12s ease, background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}

.archived-tree-row.active .archived-row-unarchive-btn,
.archived-tree-row:hover .archived-row-unarchive-btn,
.archived-row-unarchive-btn:focus-visible {
  opacity: 1;
  pointer-events: auto;
  transform: translateY(-50%) scale(1);
}

.archived-tree-row.active .archived-session-time,
.archived-tree-row:hover .archived-session-time {
  opacity: 0;
}

.archived-row-unarchive-btn:hover:not(:disabled) {
  background: var(--hover-bg);
  color: var(--text-color);
}

.archived-row-unarchive-btn:disabled {
  opacity: 0.55;
  pointer-events: none;
}

.archived-preview-header {
  padding: 12px 16px 0;
}

.archived-preview-title {
  font-size: 15px;
  font-weight: 600;
  line-height: 1.35;
  color: var(--text-color);
}

.archived-preview-meta {
  margin-top: 3px;
  font-size: 11px;
  color: var(--text-secondary);
}

.archived-message-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 12px 16px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.archived-message {
  border: 1px solid color-mix(in srgb, var(--border-color) 86%, transparent);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel-bg) 90%, var(--bg-color));
  overflow: hidden;
}

.archived-message-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 12px;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 86%, transparent);
}

.archived-message-role {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-color);
}

.archived-message-time {
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
}

.archived-message-body {
  padding: 10px 12px 12px;
}

.archived-tool-output {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 12px;
  line-height: 1.55;
  color: var(--text-color);
  font-family: var(--font-mono-block);
}

.archived-empty,
.archived-preview-empty {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 24px;
  text-align: center;
  color: var(--text-secondary);
  font-size: 12px;
}

.archived-empty-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-color);
}

.archived-empty-desc {
  margin-top: 4px;
  max-width: 260px;
  line-height: 1.5;
}

@media (max-width: 1040px) {
  .archived-shell {
    grid-template-columns: 1fr;
    min-height: auto;
  }

  .archived-list-panel {
    min-height: 240px;
  }
}
</style>
