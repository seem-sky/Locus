<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Brain } from "lucide";
import { t } from "../../i18n";
import { memoryList, memoryRetrieve } from "../../services/memory";
import { normalizeAppError } from "../../services/errors";
import type { MemoryCategory, MemoryRetrieveHit } from "../../types";

const props = defineProps<{
  workingDir?: string;
  queryText?: string;
}>();

const open = ref(false);
const loading = ref(false);
const error = ref("");
const hits = ref<MemoryRetrieveHit[]>([]);
let loadSeq = 0;

const hasWorkspace = computed(() => !!props.workingDir?.trim());
const enabled = computed(() => hasWorkspace.value);

const summaryLabel = computed(() => {
  if (!enabled.value) return t("memory.contextDisabled");
  if (loading.value && hits.value.length === 0) return t("memory.contextLoading");
  if (hits.value.length === 0) return t("memory.contextEmpty");
  return t("memory.contextEntries", hits.value.length);
});

function labelForCategory(category: MemoryCategory): string {
  return t(`memory.category.${category}`);
}

function groupedHits() {
  const groups = new Map<MemoryCategory, MemoryRetrieveHit[]>();
  for (const hit of hits.value) {
    const list = groups.get(hit.entry.category) ?? [];
    list.push(hit);
    groups.set(hit.entry.category, list);
  }
  return groups;
}

async function loadHits() {
  const seq = ++loadSeq;
  const workingDir = props.workingDir?.trim() ?? "";
  if (!workingDir) {
    hits.value = [];
    error.value = "";
    loading.value = false;
    return;
  }

  loading.value = true;
  error.value = "";
  try {
    const query = props.queryText?.trim() || "";
    const nextHits = query
      ? await memoryRetrieve(workingDir, query, { limit: 12, tokenBudget: 800 })
      : (await memoryList({ workingDir, limit: 12 })).map((entry) => ({
          entry,
          score: 1,
          keywordScore: 0,
          semanticScore: 0,
        }));
    if (seq !== loadSeq) return;
    hits.value = nextHits;
  } catch (cause) {
    if (seq !== loadSeq) return;
    hits.value = [];
    error.value = normalizeAppError(cause).message;
  } finally {
    if (seq === loadSeq) loading.value = false;
  }
}

function togglePopover() {
  if (!enabled.value) return;
  open.value = !open.value;
  if (open.value) {
    void loadHits();
  }
}

function closePopover() {
  open.value = false;
}

watch(
  () => `${props.workingDir ?? ""}::${props.queryText ?? ""}`,
  () => {
    if (open.value) void loadHits();
  },
);
</script>

<template>
  <div class="memory-context-indicator" @click.stop>
    <button
      type="button"
      class="memory-context-btn ui-select-none"
      :class="{ active: open, disabled: !enabled }"
      :disabled="!enabled"
      :title="enabled ? t('memory.contextHint') : t('memory.contextNoWorkspace')"
      :aria-expanded="open"
      @click="togglePopover"
    >
      <svg
        class="memory-context-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <component
          :is="tag"
          v-for="([tag, attrs], idx) in Brain"
          :key="idx"
          v-bind="attrs"
        />
      </svg>
      <span class="memory-context-label">{{ summaryLabel }}</span>
    </button>

    <Transition name="memory-popover">
      <div
        v-if="open"
        class="memory-context-popover"
        role="dialog"
        :aria-label="t('memory.contextPopoverTitle')"
        @click.stop
      >
        <div class="memory-context-popover-head">
          <div class="memory-context-popover-title">{{ t("memory.contextPopoverTitle") }}</div>
          <div class="memory-context-popover-subtitle">{{ t("memory.contextPopoverSubtitle") }}</div>
        </div>

        <div v-if="error" class="memory-context-error">{{ error }}</div>
        <div v-else-if="loading && hits.length === 0" class="memory-context-loading">
          {{ t("memory.contextLoading") }}
        </div>
        <div v-else-if="hits.length === 0" class="memory-context-empty">
          {{ t("memory.contextEmpty") }}
        </div>
        <div v-else class="memory-context-groups">
          <section
            v-for="[category, categoryHits] in groupedHits()"
            :key="category"
            class="memory-context-group"
          >
            <div class="memory-context-group-title">{{ labelForCategory(category) }}</div>
            <ul class="memory-context-list">
              <li v-for="hit in categoryHits" :key="hit.entry.id" class="memory-context-item">
                <span class="memory-context-item-content">{{ hit.entry.content }}</span>
                <span v-if="hit.entry.pinned" class="memory-context-pin">{{ t("memory.pinned") }}</span>
              </li>
            </ul>
          </section>
        </div>
      </div>
    </Transition>

    <div v-if="open" class="memory-context-backdrop" @click="closePopover" />
  </div>
</template>

<style scoped>
.memory-context-indicator {
  position: relative;
  display: inline-flex;
  align-items: center;
}

.memory-context-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 6px;
  border: 1px solid transparent;
  border-radius: 5px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
  transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
}

.memory-context-btn:hover:not(.disabled),
.memory-context-btn.active:not(.disabled) {
  background: var(--hover-bg);
  border-color: color-mix(in srgb, var(--accent-color) 22%, transparent);
  color: var(--accent-color);
}

.memory-context-btn.disabled {
  opacity: 0.5;
  cursor: default;
}

.memory-context-icon {
  width: 14px;
  height: 14px;
  flex: 0 0 auto;
}

.memory-context-label {
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.memory-context-backdrop {
  position: fixed;
  inset: 0;
  z-index: 20;
}

.memory-context-popover {
  position: absolute;
  left: 0;
  bottom: calc(100% + 8px);
  z-index: 30;
  width: min(360px, calc(100vw - 32px));
  max-height: min(320px, 50vh);
  overflow: auto;
  padding: 12px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--surface-elevated, var(--panel-bg));
  box-shadow: 0 12px 28px rgba(0, 0, 0, 0.18);
  color: var(--text-color);
}

.memory-context-popover-head {
  margin-bottom: 10px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-color);
}

.memory-context-popover-title {
  font-size: 13px;
  font-weight: 600;
}

.memory-context-popover-subtitle {
  margin-top: 4px;
  font-size: 12px;
  line-height: 1.45;
  color: var(--text-secondary);
}

.memory-context-error,
.memory-context-loading,
.memory-context-empty {
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-context-groups {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.memory-context-group-title {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--accent-color);
  margin-bottom: 6px;
}

.memory-context-list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.memory-context-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: color-mix(in srgb, var(--bg-color) 70%, transparent);
  font-size: 12px;
  line-height: 1.4;
}

.memory-context-item-content {
  flex: 1;
  min-width: 0;
  white-space: pre-wrap;
  word-break: break-word;
}

.memory-context-pin {
  flex: none;
  font-size: 10px;
  color: var(--accent-color);
}

.memory-popover-enter-active,
.memory-popover-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}

.memory-popover-enter-from,
.memory-popover-leave-to {
  opacity: 0;
  transform: translateY(4px);
}
</style>
