<script setup lang="ts">
import { ref, computed } from "vue";
import { t } from "../../../i18n";
import {
  knowledgeImportWebpage,
  knowledgePreviewWebpage,
  type WebpageImportResult,
  type WebpagePreviewResult,
} from "../../../services/knowledge";
import { normalizeAppError } from "../../../services/errors";
import BaseButton from "../../ui/BaseButton.vue";
import BaseSegmented from "../../ui/BaseSegmented.vue";
import BaseMarkdownEditor from "../../ui/BaseMarkdownEditor.vue";
import { useMarkdownEditorViewMode } from "../../ui/markdownEditorViewMode";

const props = defineProps<{
  targetPath: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "imported", result: WebpageImportResult): void;
  (e: "import-error", error: string): void;
}>();

// Step: 'form' | 'preview'
const step = ref<"form" | "preview">("form");

const url = ref("");
const title = ref("");
const isLoading = ref(false);
const errorMessage = ref("");
const successMessage = ref("");

// Preview data
const previewData = ref<WebpagePreviewResult | null>(null);
const importTitle = ref("");
const editableContent = ref("");

// View mode for preview toggle
const { markdownEditorViewMode: viewMode } = useMarkdownEditorViewMode();

const viewModeOptions = computed(() => [
  { value: "rendered", label: t("knowledge.editor.view.rendered") },
  { value: "native", label: t("knowledge.editor.view.native") },
]);

const isValidUrl = computed(() => {
  try {
    const u = new URL(url.value);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
});

const canPreview = computed(() => {
  return isValidUrl.value && !isLoading.value;
});

async function previewWebpage() {
  if (!canPreview.value) return;

  isLoading.value = true;
  errorMessage.value = "";

  try {
    const result = await knowledgePreviewWebpage(url.value);
    previewData.value = result;
    importTitle.value = title.value.trim() || result.title;
    editableContent.value = result.markdown;
    step.value = "preview";
  } catch (err) {
    errorMessage.value = normalizeAppError(err).message;
  } finally {
    isLoading.value = false;
  }
}

async function importFromPreview() {
  if (!previewData.value) return;

  isLoading.value = true;
  errorMessage.value = "";
  successMessage.value = "";

  try {
    const result = await knowledgeImportWebpage({
      url: url.value,
      targetPath: props.targetPath,
      title: importTitle.value.trim() || undefined,
      previewContent: {
        markdown: editableContent.value,
        charCount: editableContent.value.length,
      },
    });

    if (result.success) {
      successMessage.value = t("knowledge.webpage.import.success",
        result.title,
        result.charCount,
      );
      emit("imported", result);
    } else {
      errorMessage.value = result.error || t("knowledge.webpage.import.unknownError");
      emit("import-error", errorMessage.value);
    }
  } catch (err) {
    errorMessage.value = normalizeAppError(err).message;
    emit("import-error", errorMessage.value);
  } finally {
    isLoading.value = false;
  }
}

function backToForm() {
  step.value = "form";
  previewData.value = null;
  errorMessage.value = "";
}

function close() {
  emit("close");
}

function onContentUpdate(value: string) {
  editableContent.value = value;
}
</script>

<template>
  <div class="reference-webpage-flow">
    <!-- Step 1: Form -->
    <template v-if="step === 'form'">
      <div class="reference-webpage-intro">
        {{ t("knowledge.webpage.import.intro") }}
      </div>

      <div class="reference-webpage-form">
        <label class="reference-webpage-field">
          <span class="reference-webpage-label">{{ t("knowledge.webpage.import.url") }} *</span>
          <input
            v-model="url"
            class="reference-webpage-input"
            type="url"
            placeholder="https://example.com/article"
            :disabled="isLoading"
            @keydown.enter="canPreview && previewWebpage()"
          />
        </label>

        <label class="reference-webpage-field">
          <span class="reference-webpage-label">{{ t("knowledge.webpage.import.title") }}</span>
          <input
            v-model="title"
            class="reference-webpage-input"
            type="text"
            :placeholder='t("knowledge.webpage.import.titlePlaceholder")'
            :disabled="isLoading"
          />
          <span class="reference-webpage-hint">{{ t("knowledge.webpage.import.titleHint") }}</span>
        </label>
      </div>

      <div v-if="errorMessage" class="reference-webpage-error">
        {{ errorMessage }}
      </div>

      <div class="reference-webpage-actions">
        <BaseButton
          variant="primary"
          :disabled="!canPreview"
          @click="previewWebpage"
        >
          {{ isLoading ? t("common.loading") : t("knowledge.webpage.import.preview") }}
        </BaseButton>
        <BaseButton @click="close">
          {{ t("common.close") }}
        </BaseButton>
      </div>
    </template>

    <!-- Step 2: Preview -->
    <template v-else-if="step === 'preview'">
      <div class="reference-webpage-preview-step">
        <div class="reference-webpage-preview-header">
          <div class="reference-webpage-preview-title-row">
            <span class="reference-webpage-label">{{ t("knowledge.webpage.import.previewTitle") }}</span>
            <span class="reference-webpage-char-count">{{ editableContent.length }} {{ t("knowledge.webpage.import.characters") }}</span>
          </div>
          <input
            v-model="importTitle"
            class="reference-webpage-input reference-webpage-title-input"
            type="text"
            :placeholder='t("knowledge.webpage.import.titlePlaceholder")'
            :disabled="isLoading"
          />
        </div>

        <div class="reference-webpage-preview-toolbar">
          <BaseButton size="sm" :disabled="isLoading" @click="backToForm">
            {{ t("knowledge.webpage.import.back") }}
          </BaseButton>
          <BaseSegmented
            v-model="viewMode"
            class="preview-view-segmented"
            size="sm"
            :options="viewModeOptions"
            :aria-label="t('knowledge.editor.viewMode')"
          />
        </div>

        <div class="reference-webpage-preview-content">
          <div class="reference-webpage-preview-editor">
            <BaseMarkdownEditor
              :model-value="editableContent"
              :view-mode="viewMode"
              :disabled="isLoading"
              :placeholder="t('knowledge.preview.bodyPlaceholder')"
              @update:model-value="onContentUpdate"
            />
          </div>
        </div>

        <div v-if="errorMessage" class="reference-webpage-error">
          {{ errorMessage }}
        </div>

        <div v-if="successMessage" class="reference-webpage-success">
          {{ successMessage }}
        </div>
      </div>

      <div class="reference-webpage-actions">
        <BaseButton
          variant="primary"
          :disabled="!importTitle.trim() || isLoading"
          @click="importFromPreview"
        >
          {{ isLoading ? t("common.loading") : t("knowledge.webpage.import.import") }}
        </BaseButton>
        <BaseButton @click="close">
          {{ t("common.close") }}
        </BaseButton>
      </div>
    </template>
  </div>
</template>

<style scoped>
.reference-webpage-flow {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  flex: 1;
  height: 100%;
}

.reference-webpage-flow > template {
  display: contents;
}

/* Preview step fills available space */
.reference-webpage-flow .reference-webpage-preview-step {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  flex: 1;
  height: 100%;
}

.reference-webpage-intro {
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-secondary);
}

.reference-webpage-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.reference-webpage-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.reference-webpage-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-color);
}

.reference-webpage-input {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--input-bg);
  color: var(--text-color);
  font-size: 13px;
  line-height: 1.4;
  transition: border-color 0.15s, box-shadow 0.15s;
}

.reference-webpage-input:focus {
  outline: none;
  border-color: var(--accent-color);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-color) 20%, transparent);
}

.reference-webpage-input:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.reference-webpage-hint {
  font-size: 11px;
  color: var(--text-tertiary);
}

.reference-webpage-error {
  padding: 8px 12px;
  border-radius: 6px;
  background: color-mix(in srgb, var(--status-danger-bg, #fee2e2) 50%, transparent);
  border: 1px solid var(--status-danger-border, #ef4444);
  color: var(--status-danger-fg, #dc2626);
  font-size: 12px;
  line-height: 1.5;
}

.reference-webpage-success {
  padding: 8px 12px;
  border-radius: 6px;
  background: color-mix(in srgb, var(--status-success-bg, #dcfce7) 50%, transparent);
  border: 1px solid var(--status-success-border, #22c55e);
  color: var(--status-success-fg, #16a34a);
  font-size: 12px;
  line-height: 1.5;
}

.reference-webpage-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

/* Preview step styles */
.reference-webpage-preview-header {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.reference-webpage-preview-title-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.reference-webpage-char-count {
  font-size: 11px;
  color: var(--text-tertiary);
}

.reference-webpage-title-input {
  font-weight: 600;
}

.reference-webpage-preview-toolbar {
  display: flex;
  gap: 12px;
  align-items: center;
}

.preview-view-segmented {
  margin-left: auto;
}

.reference-webpage-preview-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
  flex: 1;
  overflow: auto;
}

.reference-webpage-preview-editor {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  border: 1px solid var(--border-color);
  border-radius: 6px;
}

.reference-webpage-preview-editor :deep(.base-markdown-editor .base-markdown-editor-textarea) {
  height: 100%;
  min-height: 100%;
  box-sizing: border-box;
  overflow: auto;
  overscroll-behavior: contain;
}

.reference-webpage-preview-editor :deep(.base-markdown-editor .vditor-ir pre.vditor-reset) {
  height: 100%;
  min-height: 100%;
  box-sizing: border-box;
  overflow: auto;
  overscroll-behavior: contain;
}
</style>
