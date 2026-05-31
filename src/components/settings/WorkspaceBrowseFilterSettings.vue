<script setup lang="ts">
import { computed, ref } from "vue";
import { t } from "../../i18n";
import {
  useWorkspaceBrowseFilters,
  type WorkspaceBrowseFilters,
} from "../../composables/useWorkspaceBrowseFilters";
import { useProjectStore } from "../../stores/project";
import BaseButton from "../ui/BaseButton.vue";

type FilterKey = keyof WorkspaceBrowseFilters;

const project = useProjectStore();
const { state, addRule, removeRule } = useWorkspaceBrowseFilters();

const hasWorkspace = computed(() => !!project.workingDir.trim());

const drafts = ref<Record<FilterKey, string>>({
  blockedFolderNames: "",
  blockedFileNames: "",
  blockedExtensions: "",
});

function commitRule(key: FilterKey) {
  const raw = drafts.value[key];
  if (!raw.trim()) return;
  addRule(key, raw);
  drafts.value[key] = "";
}

function onRuleKeydown(key: FilterKey, event: KeyboardEvent) {
  if (event.key !== "Enter") return;
  event.preventDefault();
  commitRule(key);
}

const sections: Array<{
  key: FilterKey;
  titleKey: string;
  descKey: string;
  placeholderKey: string;
}> = [
  {
    key: "blockedFolderNames",
    titleKey: "settings.general.browseFilters.folders",
    descKey: "settings.general.browseFilters.foldersDesc",
    placeholderKey: "settings.general.browseFilters.foldersPlaceholder",
  },
  {
    key: "blockedFileNames",
    titleKey: "settings.general.browseFilters.files",
    descKey: "settings.general.browseFilters.filesDesc",
    placeholderKey: "settings.general.browseFilters.filesPlaceholder",
  },
  {
    key: "blockedExtensions",
    titleKey: "settings.general.browseFilters.extensions",
    descKey: "settings.general.browseFilters.extensionsDesc",
    placeholderKey: "settings.general.browseFilters.extensionsPlaceholder",
  },
];
</script>

<template>
  <div class="settings-section browse-filter-section">
    <div class="section-label">{{ t("settings.general.browseFilters.title") }}</div>
    <p class="section-desc">{{ t("settings.general.browseFilters.desc") }}</p>
    <p v-if="!hasWorkspace" class="browse-filter-hint">
      {{ t("settings.general.browseFilters.workspaceRequired") }}
    </p>

    <div
      v-for="section in sections"
      :key="section.key"
      class="browse-filter-block"
    >
      <div class="browse-filter-block-title">{{ t(section.titleKey) }}</div>
      <p class="browse-filter-block-desc">{{ t(section.descKey) }}</p>

      <div class="browse-filter-input-row">
        <input
          v-model="drafts[section.key]"
          class="browse-filter-input"
          type="text"
          :placeholder="t(section.placeholderKey)"
          @keydown="onRuleKeydown(section.key, $event)"
        />
        <BaseButton
          size="sm"
          :disabled="!drafts[section.key].trim()"
          @click="commitRule(section.key)"
        >
          {{ t("settings.general.browseFilters.add") }}
        </BaseButton>
      </div>

      <div v-if="state[section.key].length" class="browse-filter-tags">
        <button
          v-for="rule in state[section.key]"
          :key="`${section.key}-${rule}`"
          type="button"
          class="browse-filter-tag"
          :title="t('settings.general.browseFilters.remove')"
          @click="removeRule(section.key, rule)"
        >
          <span>{{ rule }}</span>
          <span class="browse-filter-tag-remove" aria-hidden="true">×</span>
        </button>
      </div>
      <p v-else class="browse-filter-empty">{{ t("settings.general.browseFilters.empty") }}</p>
    </div>
  </div>
</template>

<style scoped>
.browse-filter-section {
  max-width: 760px;
}

.browse-filter-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 14px;
  padding: 14px 16px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: color-mix(in srgb, var(--panel-bg) 84%, var(--sidebar-bg) 16%);
}

.browse-filter-block-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-color);
}

.browse-filter-block-desc {
  margin: 0;
  font-size: 11px;
  color: var(--text-secondary);
  line-height: 1.45;
}

.browse-filter-input-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.browse-filter-input {
  flex: 1;
  min-width: 0;
  height: 32px;
  padding: 0 10px;
  border-radius: 6px;
  border: 1px solid var(--border-color);
  background: var(--input-bg);
  color: var(--text-color);
  font-size: 12px;
}

.browse-filter-input:focus-visible {
  outline: 2px solid var(--accent-color);
  outline-offset: -1px;
}

.browse-filter-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.browse-filter-tag {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 4px 8px;
  border-radius: 999px;
  border: 1px solid var(--border-color);
  background: var(--hover-bg);
  color: var(--text-color);
  font-size: 11px;
  cursor: pointer;
}

.browse-filter-tag:hover {
  border-color: var(--border-strong);
  background: color-mix(in srgb, var(--hover-bg) 70%, var(--status-danger-bg) 30%);
}

.browse-filter-tag span:first-child {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.browse-filter-tag-remove {
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1;
}

.browse-filter-empty {
  margin: 0;
  font-size: 11px;
  color: var(--text-secondary);
}

.browse-filter-hint {
  margin: 0 0 8px;
  font-size: 11px;
  color: var(--status-warning-fg, var(--text-secondary));
}
</style>
