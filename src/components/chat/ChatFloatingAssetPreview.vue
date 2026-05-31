<script setup lang="ts">
import { computed, defineAsyncComponent, toRef } from "vue";
import { t } from "../../i18n";
import { useWorkspaceAssetPreview } from "../../composables/useWorkspaceAssetPreview";
import { useComposerAssetRefDropTarget } from "../../composables/useComposerAssetRefDrop";
import { useAssetRefPointerDragSource } from "../../composables/useAssetRefPointerDrag";
import { useChatStore } from "../../stores/chat";
import AssetChip from "../AssetChip.vue";

const AssetPreviewHost = defineAsyncComponent(
  () => import("../asset/AssetPreviewHost.vue"),
);

const props = defineProps<{
  workingDir: string;
  path: string;
  name?: string;
}>();

const chatStore = useChatStore();
const workingDirRef = toRef(() => props.workingDir);
const assetPathRef = toRef(() => props.path);

const {
  previewPayload,
  previewLoading,
  previewError,
  previewDisplayName,
  previewDisplayPath,
  activeTargetId,
  targetCache,
  targetLoading,
  loadTarget,
} = useWorkspaceAssetPreview(workingDirRef, assetPathRef);

const headerTitle = computed(() => props.name?.trim() || previewDisplayName.value);
const composerAssetRefDrop = useComposerAssetRefDropTarget();
const assetRefPointerDrag = useAssetRefPointerDragSource();

function closePanel() {
  chatStore.closeFloatingAssetPreview();
}

function onHeaderPointerDown(event: PointerEvent) {
  if (!previewDisplayPath.value) return;
  assetRefPointerDrag.onFileRowPointerDown(previewDisplayPath.value, event);
}

function onPreviewDragOver(event: DragEvent) {
  composerAssetRefDrop.acceptDragOver(event);
}

function onPreviewDrop(event: DragEvent) {
  composerAssetRefDrop.handleDrop(event);
}
</script>

<template>
  <aside
    class="chat-floating-asset-preview ui-select-none"
    role="complementary"
    data-composer-asset-ref-drop
    :aria-label="t('chat.floatingAssetPreview.title')"
    @mousedown.stop
    @dragover="onPreviewDragOver"
    @drop="onPreviewDrop"
  >
    <header class="chat-floating-asset-preview-header">
      <div
        class="chat-floating-asset-preview-drag"
        :title="t('chat.floatingAssetPreview.dragHint')"
        @pointerdown.stop="onHeaderPointerDown"
      >
        <AssetChip
          :path="previewDisplayPath"
          kind="asset"
        />
        <span class="chat-floating-asset-preview-name">{{ headerTitle }}</span>
      </div>
      <span class="chat-floating-asset-preview-hint">{{ t("chat.floatingAssetPreview.dragHint") }}</span>
      <button
        type="button"
        class="chat-floating-asset-preview-close"
        :title="t('chat.floatingAssetPreview.close')"
        :aria-label="t('chat.floatingAssetPreview.close')"
        @click="closePanel"
      >
        &times;
      </button>
    </header>
    <div class="chat-floating-asset-preview-body">
      <AssetPreviewHost
        :payload="previewPayload"
        :loading="previewLoading"
        :error="previewError"
        :selected-name="previewDisplayName"
        :selected-path="previewDisplayPath"
        :active-target-id="activeTargetId"
        :target-cache="targetCache"
        :target-loading="targetLoading"
        :load-target="loadTarget"
        @close="closePanel"
      />
    </div>
  </aside>
</template>

<style scoped>
.chat-floating-asset-preview {
  position: absolute;
  inset: 0;
  z-index: 12;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  border: none;
  border-radius: 0;
  background: var(--bg-color);
  pointer-events: auto;
}

.chat-floating-asset-preview-header {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 40px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border-color);
  background: color-mix(in srgb, var(--panel-bg) 92%, var(--bg-color) 8%);
}

.chat-floating-asset-preview-drag {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  flex: 1 1 auto;
  padding: 2px 4px;
  border-radius: 6px;
  cursor: grab;
}

.chat-floating-asset-preview-drag:active {
  cursor: grabbing;
}

.chat-floating-asset-preview-drag:hover {
  background: var(--hover-bg);
}

.chat-floating-asset-preview-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-family: var(--font-mono-identifier);
  color: var(--text-color);
}

.chat-floating-asset-preview-hint {
  flex-shrink: 0;
  max-width: 220px;
  font-size: 11px;
  line-height: 1.3;
  color: var(--text-secondary);
  text-align: right;
}

.chat-floating-asset-preview-close {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 20px;
  line-height: 1;
  cursor: pointer;
}

.chat-floating-asset-preview-close:hover,
.chat-floating-asset-preview-close:focus-visible {
  background: var(--hover-bg);
  color: var(--text-color);
}

.chat-floating-asset-preview-body {
  flex: 1 1 0;
  min-height: 0;
  min-width: 0;
  display: flex;
  overflow: hidden;
}

.chat-floating-asset-preview-body :deep(.aph-root) {
  height: 100%;
}
</style>
