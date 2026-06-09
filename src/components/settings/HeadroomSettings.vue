<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { t } from "../../i18n";
import { ensureHeadroomProxy, getHeadroomSettingsStatus, saveHeadroomSettings } from "../../services/system";
import type { HeadroomSettings, HeadroomSettingsStatus } from "../../types";
import BaseButton from "../ui/BaseButton.vue";
import BaseSwitch from "../ui/BaseSwitch.vue";

const status = ref<HeadroomSettingsStatus | null>(null);
const draft = ref<HeadroomSettings>(defaultSettings());
const loading = ref(false);
const saving = ref(false);
const proxyStarting = ref(false);
const error = ref<string | null>(null);
const saveError = ref<string | null>(null);
let proxyPollTimer: ReturnType<typeof setInterval> | null = null;

function defaultSettings(): HeadroomSettings {
  return {
    enabled: true,
    contextCompressEnabled: true,
    alwaysCompressContext: false,
    baseUrl: "http://127.0.0.1:8787",
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

function shouldEnsureLocalProxy(next: HeadroomSettingsStatus): boolean {
  const proxy = next.proxy;
  if (!proxy.autostartEnabled || proxy.running) {
    return false;
  }
  return proxy.source === "bundled"
    || proxy.source === "external"
    || proxy.source === "notConfigured";
}

function stopProxyPolling() {
  if (proxyPollTimer) {
    clearInterval(proxyPollTimer);
    proxyPollTimer = null;
  }
}

function startProxyPolling() {
  stopProxyPolling();
  proxyPollTimer = setInterval(() => {
    void refreshProxyStatusOnly();
  }, 3000);
}

async function refreshProxyStatusOnly() {
  try {
    const next = await getHeadroomSettingsStatus();
    applyStatus(next);
    if (next.proxy.running) {
      proxyStarting.value = false;
      stopProxyPolling();
    }
  } catch {
    // Keep polling; transient IPC errors should not block the panel.
  }
}

function ensureLocalProxyInBackground() {
  if (proxyStarting.value) {
    return;
  }
  proxyStarting.value = true;
  startProxyPolling();
  void ensureHeadroomProxy()
    .catch(() => undefined)
    .finally(() => {
      proxyStarting.value = false;
      void refreshProxyStatusOnly();
    });
}

async function loadStatus() {
  loading.value = true;
  error.value = null;
  saveError.value = null;
  try {
    const next = await getHeadroomSettingsStatus();
    applyStatus(next);
    if (shouldEnsureLocalProxy(next)) {
      ensureLocalProxyInBackground();
    } else {
      stopProxyPolling();
      proxyStarting.value = false;
    }
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
    const next = await saveHeadroomSettings(draft.value);
    applyStatus(next);
    if (shouldEnsureLocalProxy(next)) {
      ensureLocalProxyInBackground();
    }
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
  draft.value = {
    ...draft.value,
    contextCompressEnabled: value,
    alwaysCompressContext: value ? draft.value.alwaysCompressContext : false,
  };
  await persistDraft();
}

async function updateAlwaysCompressContext(value: boolean) {
  if (draft.value.alwaysCompressContext === value || saving.value) return;
  draft.value = {
    ...draft.value,
    alwaysCompressContext: value,
    contextCompressEnabled: value ? true : draft.value.contextCompressEnabled,
  };
  await persistDraft();
}

async function saveMinCompressChars() {
  if (!status.value) return;
  const normalized = Math.max(1, Math.floor(Number(draft.value.minCompressChars) || 2000));
  if (draft.value.minCompressChars === normalized
    && normalized === status.value.settings.minCompressChars) {
    return;
  }
  draft.value = { ...draft.value, minCompressChars: normalized };
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

const proxySourceLabel = computed(() => {
  const proxy = status.value?.proxy;
  if (!proxy) return "";
  const key = `settings.headroom.proxySource.${proxy.source}`;
  const label = t(key);
  if (proxy.runtimeDetail) {
    return `${label} (${proxy.runtimeDetail})`;
  }
  return label;
});

const proxyRunningLabel = computed(() => {
  if (proxyStarting.value) {
    return t("settings.headroom.proxyStarting");
  }
  const proxy = status.value?.proxy;
  if (!proxy) return "";
  if (proxy.source === "disabled" || proxy.source === "cloud") {
    return proxy.running
      ? t("settings.headroom.proxyReachable")
      : t("settings.headroom.proxyUnreachable");
  }
  return proxy.running
    ? t("settings.headroom.proxyRunningYes")
    : t("settings.headroom.proxyRunningNo");
});

const proxyAutostartLabel = computed(() => {
  const proxy = status.value?.proxy;
  if (!proxy) return "";
  if (proxy.source === "cloud" || proxy.source === "disabled") {
    return t("settings.headroom.proxyAutostartNa");
  }
  return proxy.autostartEnabled
    ? t("settings.headroom.proxyAutostartOn")
    : t("settings.headroom.proxyAutostartOff");
});

const proxyBundleLabel = computed(() => {
  const proxy = status.value?.proxy;
  if (!proxy) return "";
  return proxy.bundlePresent
    ? t("settings.headroom.proxyBundlePresent")
    : t("settings.headroom.proxyBundleMissing");
});

const advancedDirty = computed(() => {
  if (!status.value) return false;
  const saved = status.value.settings;
  return (
    draft.value.baseUrl !== saved.baseUrl
    || draft.value.apiKey !== saved.apiKey
    || draft.value.rtkPath !== saved.rtkPath
  );
});

const compressSettingsDirty = computed(() => {
  if (!status.value) return false;
  return draft.value.minCompressChars !== status.value.settings.minCompressChars;
});

onMounted(() => {
  void loadStatus();
});

onUnmounted(() => {
  stopProxyPolling();
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
          <dt>{{ t("settings.headroom.proxyStatus") }}</dt>
          <dd>{{ proxySourceLabel }}</dd>
          <dt>{{ t("settings.headroom.proxyRunning") }}</dt>
          <dd>{{ proxyRunningLabel }}</dd>
          <dt>{{ t("settings.headroom.proxyEndpoint") }}</dt>
          <dd class="headroom-mono">{{ status.proxy.endpoint }}</dd>
          <template v-if="status.proxy.source === 'bundled' || status.proxy.source === 'external'">
            <dt>{{ t("settings.headroom.proxyAutostart") }}</dt>
            <dd>{{ proxyAutostartLabel }}</dd>
          </template>
          <template v-if="status.proxy.source === 'bundled' || status.proxy.source === 'notConfigured'">
            <dt>{{ t("settings.headroom.proxyBundle") }}</dt>
            <dd>{{ proxyBundleLabel }}</dd>
          </template>
          <template v-if="status.proxy.error">
            <dt>{{ t("settings.headroom.proxyError") }}</dt>
            <dd class="headroom-error-inline">{{ status.proxy.error }}</dd>
          </template>
        </dl>
      </section>

      <section class="headroom-block">
        <div class="headroom-block-title">{{ t("settings.headroom.compression") }}</div>
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
        <div class="headroom-toggle-row">
          <div>
            <div class="headroom-toggle-label">{{ t("settings.headroom.alwaysCompressContext") }}</div>
            <p class="headroom-toggle-hint">{{ t("settings.headroom.alwaysCompressContextHint") }}</p>
          </div>
          <BaseSwitch
            :model-value="draft.alwaysCompressContext"
            :disabled="loading || saving || !draft.enabled || !draft.contextCompressEnabled"
            @update:model-value="updateAlwaysCompressContext"
          />
        </div>
        <label class="headroom-field headroom-threshold-field">
          <span class="headroom-field-key">{{ t("settings.headroom.minCompressChars") }}</span>
          <p class="headroom-toggle-hint">{{ t("settings.headroom.minCompressCharsHint") }}</p>
          <div class="headroom-threshold-row">
            <input
              v-model.number="draft.minCompressChars"
              class="headroom-input headroom-threshold-input"
              type="number"
              min="1"
              step="100"
              :disabled="loading || saving || !draft.enabled"
              @change="saveMinCompressChars"
            />
            <BaseButton
              size="sm"
              :disabled="loading || saving || !compressSettingsDirty"
              @click="saveMinCompressChars"
            >
              {{ saving ? t("common.loading") : t("settings.headroom.save") }}
            </BaseButton>
          </div>
        </label>
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

.headroom-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  word-break: break-all;
}

.headroom-error-inline {
  color: var(--danger, #e5484d);
  font-size: 12px;
  word-break: break-word;
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

.headroom-threshold-field {
  margin-top: 4px;
}

.headroom-threshold-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.headroom-threshold-input {
  max-width: 160px;
}

.headroom-error {
  color: var(--danger, #e5484d);
}
</style>
