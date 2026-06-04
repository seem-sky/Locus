<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { t } from "../../i18n";
import { getHeadroomSettingsStatus, saveHeadroomSettings } from "../../services/system";
import type { HeadroomSettings, HeadroomSettingsStatus } from "../../types";
import BaseButton from "../ui/BaseButton.vue";
import BaseSwitch from "../ui/BaseSwitch.vue";

const status = ref<HeadroomSettingsStatus | null>(null);
const draft = ref<HeadroomSettings>(defaultSettings());
const loading = ref(false);
const saving = ref(false);
const error = ref<string | null>(null);
const saveError = ref<string | null>(null);

function defaultSettings(): HeadroomSettings {
  return {
    enabled: true,
    contextCompressEnabled: true,
    baseUrl: "http://localhost:8787",
    apiKey: "",
    rtkPath: "",
    minCompressChars: 2000,
  };
}

function cloneSettings(settings: HeadroomSettings): HeadroomSettings {
  return { ...settings };
}

function applyStatus(next: HeadroomSettingsStatus) {
  status.value = next;
  draft.value = cloneSettings(next.settings);
}

async function loadStatus() {
  loading.value = true;
  error.value = null;
  saveError.value = null;
  try {
    applyStatus(await getHeadroomSettingsStatus());
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function persistDraft() {
  saving.value = true;
  saveError.value = null;
  try {
    applyStatus(await saveHeadroomSettings(draft.value));
  } catch (err) {
    saveError.value = err instanceof Error ? err.message : String(err);
  } finally {
    saving.value = false;
  }
}

async function updateEnabled(value: boolean) {
  if (draft.value.enabled === value || saving.value) return;
  draft.value = { ...draft.value, enabled: value };
  await persistDraft();
}

async function updateContextCompress(value: boolean) {
  if (draft.value.contextCompressEnabled === value || saving.value) return;
  draft.value = { ...draft.value, contextCompressEnabled: value };
  await persistDraft();
}

async function saveAdvancedFields() {
  await persistDraft();
}

const libraryStatusLabel = computed(() => {
  if (!status.value) return "";
  if (status.value.libraryAvailable) {
    return t("settings.headroom.statusLibraryReady");
  }
  return t("settings.headroom.statusLibraryUnavailable");
});

const contextLibraryStatusLabel = computed(() => {
  if (!status.value) return "";
  if (status.value.contextLibraryAvailable) {
    return t("settings.headroom.statusContextReady");
  }
  return t("settings.headroom.statusContextUnavailable");
});

const advancedDirty = computed(() => {
  if (!status.value) return false;
  const saved = status.value.settings;
  return (
    draft.value.baseUrl !== saved.baseUrl
    || draft.value.apiKey !== saved.apiKey
    || draft.value.rtkPath !== saved.rtkPath
    || draft.value.minCompressChars !== saved.minCompressChars
  );
});

onMounted(() => {
  void loadStatus();
});
</script>

<template>
  <div class="settings-section headroom-settings">
    <div class="headroom-header">
      <div>
        <div class="section-label">{{ t("settings.headroom.title") }}</div>
        <p class="section-desc">{{ t("settings.headroom.desc") }}</p>
      </div>
      <BaseButton size="sm" :disabled="loading || saving" @click="loadStatus">
        {{ loading ? t("common.loading") : t("settings.headroom.refresh") }}
      </BaseButton>
    </div>

    <div v-if="error" class="headroom-error">
      {{ t("settings.headroom.loadFailed", error) }}
    </div>

    <div v-else-if="status" class="headroom-body">
      <section class="headroom-block">
        <div class="headroom-block-title">{{ t("settings.headroom.availability") }}</div>
        <dl class="headroom-grid">
          <dt>{{ t("settings.headroom.libraryStatus") }}</dt>
          <dd>{{ libraryStatusLabel }}</dd>
          <dt>{{ t("settings.headroom.contextStatus") }}</dt>
          <dd>{{ contextLibraryStatusLabel }}</dd>
        </dl>
      </section>

      <section class="headroom-block">
        <div class="headroom-toggle-row">
          <div>
            <div class="headroom-toggle-label">{{ t("settings.headroom.enabled") }}</div>
            <p class="headroom-toggle-hint">{{ t("settings.headroom.enabledHint") }}</p>
          </div>
          <BaseSwitch
            :model-value="draft.enabled"
            :disabled="loading || saving"
            @update:model-value="updateEnabled"
          />
        </div>
        <div class="headroom-toggle-row">
          <div>
            <div class="headroom-toggle-label">{{ t("settings.headroom.contextCompress") }}</div>
            <p class="headroom-toggle-hint">{{ t("settings.headroom.contextCompressHint") }}</p>
          </div>
          <BaseSwitch
            :model-value="draft.contextCompressEnabled"
            :disabled="loading || saving || !draft.enabled"
            @update:model-value="updateContextCompress"
          />
        </div>
      </section>

      <section class="headroom-block">
        <div class="headroom-block-title">{{ t("settings.headroom.advanced") }}</div>
        <div class="headroom-form">
          <label class="headroom-field">
            <span class="headroom-field-key">{{ t("settings.headroom.baseUrl") }}</span>
            <input
              v-model="draft.baseUrl"
              class="headroom-input"
              type="url"
              :placeholder="t('settings.headroom.baseUrlPlaceholder')"
              :disabled="loading || saving"
            />
          </label>
          <label class="headroom-field">
            <span class="headroom-field-key">{{ t("settings.headroom.apiKey") }}</span>
            <input
              v-model="draft.apiKey"
              class="headroom-input"
              type="password"
              autocomplete="off"
              :placeholder="t('settings.headroom.apiKeyPlaceholder')"
              :disabled="loading || saving"
            />
          </label>
          <label class="headroom-field">
            <span class="headroom-field-key">{{ t("settings.headroom.rtkPath") }}</span>
            <input
              v-model="draft.rtkPath"
              class="headroom-input"
              type="text"
              :placeholder="t('settings.headroom.rtkPathPlaceholder')"
              :disabled="loading || saving"
            />
          </label>
          <label class="headroom-field">
            <span class="headroom-field-key">{{ t("settings.headroom.minCompressChars") }}</span>
            <input
              v-model.number="draft.minCompressChars"
              class="headroom-input"
              type="number"
              min="1"
              step="100"
              :disabled="loading || saving"
            />
          </label>
        </div>
        <div class="headroom-actions">
          <BaseButton
            size="sm"
            :disabled="loading || saving || !advancedDirty"
            @click="saveAdvancedFields"
          >
            {{ saving ? t("common.loading") : t("settings.headroom.save") }}
          </BaseButton>
        </div>
      </section>

      <p class="headroom-note">{{ t("settings.headroom.envNote") }}</p>

      <div v-if="saveError" class="headroom-error">
        {{ t("settings.headroom.saveFailed", saveError) }}
      </div>
    </div>

    <div v-else class="headroom-empty">{{ t("common.loading") }}</div>
  </div>
</template>

<style scoped>
.headroom-settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.headroom-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.headroom-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.headroom-block {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.headroom-block-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.headroom-grid {
  display: grid;
  grid-template-columns: minmax(120px, 160px) 1fr;
  gap: 8px 12px;
  margin: 0;
}

.headroom-grid dt,
.headroom-grid dd {
  margin: 0;
  font-size: 13px;
}

.headroom-grid dt {
  color: var(--text-secondary);
}

.headroom-toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.headroom-toggle-label {
  font-size: 13px;
  font-weight: 500;
}

.headroom-toggle-hint,
.headroom-note,
.headroom-empty,
.headroom-error {
  margin: 0;
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.5;
}

.headroom-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.headroom-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.headroom-field-key {
  font-size: 12px;
  color: var(--text-secondary);
}

.headroom-input {
  width: 100%;
  padding: 8px 10px;
  border: 1px solid var(--border-subtle);
  border-radius: 6px;
  background: var(--bg-elevated);
  color: var(--text-primary);
  font-size: 13px;
}

.headroom-input:focus {
  outline: none;
  border-color: var(--accent);
}

.headroom-actions {
  display: flex;
  justify-content: flex-end;
}

.headroom-error {
  color: var(--danger, #e5484d);
}
</style>
