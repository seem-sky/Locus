<script setup lang="ts">
import BaseContextMenu from "../ui/BaseContextMenu.vue";
import type { CodePreviewSelectionMenuState } from "../../composables/useCodePreviewSelectionMenu";
import { t } from "../../i18n";

defineProps<{
  menu: CodePreviewSelectionMenuState;
}>();

const emit = defineEmits<{
  close: [];
  copy: [];
  sendToComposer: [];
}>();
</script>

<template>
  <BaseContextMenu
    :x="menu.x"
    :y="menu.y"
    class="code-preview-selection-menu"
    :aria-label="t('codePreview.selection.menuLabel')"
    @close="emit('close')"
  >
    <button type="button" @click="emit('copy')">
      {{ t("codePreview.selection.copy") }}
    </button>
    <button type="button" @click="emit('sendToComposer')">
      {{ t("codePreview.selection.sendToComposer") }}
    </button>
  </BaseContextMenu>
</template>
