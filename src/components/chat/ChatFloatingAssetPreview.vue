<script setup lang="ts">
import { defineAsyncComponent, toRef } from "vue";
import { t } from "../../i18n";
import { useWorkspaceAssetPreview } from "../../composables/useWorkspaceAssetPreview";
import { useComposerAssetRefDropTarget } from "../../composables/useComposerAssetRefDrop";
import { useChatStore } from "../../stores/chat";

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

const composerAssetRefDrop = useComposerAssetRefDropTarget();

function closePanel() {
  chatStore.closeFloatingAssetPreview();
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
  flex: 1 1 0;
  width: 100%;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: none;
  border-radius: 0;
  background: var(--bg-color);
  pointer-events: auto;
}

.chat-floating-asset-preview-body {
  flex: 1 1 0;
  min-height: 0;
  min-width: 0;
  display: flex;
  overflow: hidden;
}

.chat-floating-asset-preview-body :deep(.aph-root) {
  flex: 1 1 0;
  min-height: 0;
  width: 100%;
}
</style>
