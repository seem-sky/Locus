<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, onUnmounted, ref } from "vue";
import { t } from "../i18n";
import { useChatStore } from "../stores/chat";
import { acquireSelectionLock } from "../composables/useSelectionLock";

const AssetView = defineAsyncComponent(() => import("./AssetView.vue"));

const props = withDefaults(defineProps<{
  workingDir: string;
  layout?: "side" | "bottom";
  maxSideWidth?: number;
  storageScope?: string;
}>(), {
  layout: "side",
  storageScope: "",
});

const chatStore = useChatStore();

const STORAGE_KEY_PANEL_WIDTH = "locus:chatProjectViewWidth";
const STORAGE_KEY_PANEL_HEIGHT = "locus:chatProjectViewHeight";
const DEFAULT_PANEL_WIDTH = 300;
const DEFAULT_PANEL_HEIGHT = 420;
const MIN_PANEL_WIDTH = 260;
const MAX_PANEL_WIDTH = 520;
const MIN_PANEL_HEIGHT = 280;
const MAX_PANEL_HEIGHT = 560;

const shellRef = ref<HTMLElement | null>(null);
const panelWidth = ref(DEFAULT_PANEL_WIDTH);
const panelHeight = ref(DEFAULT_PANEL_HEIGHT);
const isDraggingPanel = ref(false);
let releasePanelSelectionLock: (() => void) | null = null;

const panelWidthStorageKey = computed(() => scopedStorageKey(STORAGE_KEY_PANEL_WIDTH));
const panelHeightStorageKey = computed(() => scopedStorageKey(STORAGE_KEY_PANEL_HEIGHT));
const effectiveMaxSideWidth = computed(() => {
  const maxWidth = props.maxSideWidth;
  if (typeof maxWidth !== "number" || !Number.isFinite(maxWidth)) {
    return MAX_PANEL_WIDTH;
  }
  return Math.max(MIN_PANEL_WIDTH, Math.min(MAX_PANEL_WIDTH, Math.floor(maxWidth)));
});
const effectivePanelWidth = computed(() =>
  clampPanelWidth(panelWidth.value, effectiveMaxSideWidth.value),
);

const panelStyle = computed(() => {
  if (props.layout === "bottom") {
    return {
      width: "100%",
      minWidth: "0",
      height: `${panelHeight.value}px`,
      minHeight: `${panelHeight.value}px`,
    };
  }
  const width = effectivePanelWidth.value;
  return {
    width: `${width}px`,
    minWidth: `${width}px`,
  };
});

function scopedStorageKey(baseKey: string) {
  const scope = props.storageScope.trim();
  if (!scope) return baseKey;
  return baseKey.replace("locus:", `locus:${scope}:`);
}

function clampPanelWidth(next: number, maxWidth = MAX_PANEL_WIDTH) {
  const normalizedNext = Number.isFinite(next) ? next : DEFAULT_PANEL_WIDTH;
  const normalizedMax = Number.isFinite(maxWidth) ? maxWidth : MAX_PANEL_WIDTH;
  const upperBound = Math.max(
    MIN_PANEL_WIDTH,
    Math.min(MAX_PANEL_WIDTH, Math.floor(normalizedMax)),
  );
  return Math.max(MIN_PANEL_WIDTH, Math.min(upperBound, normalizedNext));
}

function clampPanelHeight(next: number) {
  return Math.max(MIN_PANEL_HEIGHT, Math.min(MAX_PANEL_HEIGHT, next));
}

function closePanel() {
  chatStore.closeProjectViewPanel();
}

function onPanelResizeMouseDown(event: MouseEvent) {
  event.preventDefault();
  isDraggingPanel.value = true;
  releasePanelSelectionLock?.();
  releasePanelSelectionLock = acquireSelectionLock();
  document.addEventListener("mousemove", onPanelResizeMouseMove);
  document.addEventListener("mouseup", onPanelResizeMouseUp);
}

function onPanelResizeMouseMove(event: MouseEvent) {
  if (!isDraggingPanel.value || !shellRef.value) return;
  const rect = shellRef.value.getBoundingClientRect();
  if (props.layout === "bottom") {
    const nextHeight = clampPanelHeight(rect.bottom - event.clientY);
    panelHeight.value = nextHeight;
    return;
  }
  const nextWidth = clampPanelWidth(rect.right - event.clientX, effectiveMaxSideWidth.value);
  panelWidth.value = nextWidth;
}

function stopPanelResize(persist = true) {
  if (!isDraggingPanel.value) return;
  isDraggingPanel.value = false;
  document.removeEventListener("mousemove", onPanelResizeMouseMove);
  document.removeEventListener("mouseup", onPanelResizeMouseUp);
  releasePanelSelectionLock?.();
  releasePanelSelectionLock = null;
  if (!persist) return;
  try {
    if (props.layout === "bottom") {
      localStorage.setItem(panelHeightStorageKey.value, String(panelHeight.value));
    } else {
      localStorage.setItem(panelWidthStorageKey.value, String(panelWidth.value));
    }
  } catch {
    // ignore persistence failures
  }
}

function onPanelResizeMouseUp() {
  stopPanelResize(true);
}

function loadPanelSize() {
  try {
    const storedWidth = localStorage.getItem(panelWidthStorageKey.value);
    if (storedWidth) {
      const parsed = Number.parseInt(storedWidth, 10);
      if (Number.isFinite(parsed)) {
        panelWidth.value = clampPanelWidth(parsed, effectiveMaxSideWidth.value);
      }
    }
    const storedHeight = localStorage.getItem(panelHeightStorageKey.value);
    if (storedHeight) {
      const parsed = Number.parseInt(storedHeight, 10);
      if (Number.isFinite(parsed)) {
        panelHeight.value = clampPanelHeight(parsed);
      }
    }
  } catch {
    // ignore persistence failures
  }
}

function onWindowResize() {
  panelWidth.value = clampPanelWidth(panelWidth.value, effectiveMaxSideWidth.value);
}

onMounted(() => {
  loadPanelSize();
  window.addEventListener("resize", onWindowResize);
});

onUnmounted(() => {
  window.removeEventListener("resize", onWindowResize);
  stopPanelResize(false);
});
</script>

<template>
  <div
    ref="shellRef"
    class="chat-project-view-shell"
    :class="[
      `layout-${layout}`,
      { 'dragging-panel': isDraggingPanel },
    ]"
  >
    <div class="chat-project-view-resize-handle" @mousedown="onPanelResizeMouseDown"></div>

    <aside class="chat-project-view-panel" :style="panelStyle">
      <header class="chat-project-view-header">
        <span class="chat-project-view-title">{{ t("chat.projectView.title") }}</span>
        <button
          type="button"
          class="chat-project-view-close ui-select-none"
          :title="t('chat.projectView.close')"
          @click="closePanel"
        >
          &times;
        </button>
      </header>
      <div class="chat-project-view-body">
        <AssetView :working-dir="workingDir" embedded />
      </div>
    </aside>
  </div>
</template>

<style scoped>
.chat-project-view-shell {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-shrink: 0;
}

.chat-project-view-shell.layout-bottom {
  width: 100%;
  height: auto;
  min-width: 0;
  flex-direction: column;
}

.chat-project-view-resize-handle {
  width: 3px;
  flex-shrink: 0;
  cursor: col-resize;
  background: var(--border-color);
  transition: background 0.15s ease;
}

.chat-project-view-shell.layout-bottom .chat-project-view-resize-handle {
  width: 100%;
  height: 3px;
  cursor: row-resize;
}

.chat-project-view-resize-handle:hover,
.chat-project-view-shell.dragging-panel .chat-project-view-resize-handle {
  background: var(--text-secondary);
}

.chat-project-view-panel {
  width: 300px;
  min-width: 300px;
  height: 100%;
  min-height: 0;
  background: var(--msg-assistant-bg);
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  flex-shrink: 0;
  border-left: 1px solid var(--border-color);
}

.chat-project-view-shell.layout-bottom .chat-project-view-panel {
  width: 100%;
  min-width: 0;
  border-left: none;
  border-top: 1px solid var(--border-color);
}

.chat-project-view-header {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 36px;
  padding: 6px 10px 6px 12px;
  border-bottom: 1px solid var(--border-color);
}

.chat-project-view-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--text-secondary);
}

.chat-project-view-close {
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 18px;
  line-height: 1;
  cursor: pointer;
}

.chat-project-view-close:hover,
.chat-project-view-close:focus-visible {
  background: var(--hover-bg);
  color: var(--text-color);
}

.chat-project-view-body {
  flex: 1 1 0;
  min-height: 0;
  min-width: 0;
  display: flex;
  overflow: hidden;
}

.chat-project-view-body :deep(.asset-view) {
  background: transparent;
}
</style>
