<script setup lang="ts">
import { computed } from "vue";
import { t } from "../../i18n";
import type { WorkflowToolWhitelistPayload } from "../../services/permissions";
import BaseButton from "../ui/BaseButton.vue";
import BaseSegmented from "../ui/BaseSegmented.vue";

type ToolMode = "auto" | "ask";
type FileBoundaryMode = "all" | "workspace";

interface ToolPermissionItem {
  name: string;
  label: string;
  desc: string;
  defaultMode: ToolMode;
}

const props = defineProps<{
  toolPermissionMode: ToolMode;
  toolList: ToolPermissionItem[];
  behaviorList: ToolPermissionItem[];
  toolPermissions: Record<string, ToolMode>;
  fileWorkspaceBoundaryEnabled: boolean;
  fileWorkspaceBoundaryReady: boolean;
  fileWorkspaceBoundaryBusy: boolean;
  workflowToolWhitelist: WorkflowToolWhitelistPayload;
  workflowWhitelistReady: boolean;
  workflowWhitelistBusy: boolean;
  permSaveMsg: string;
}>();

const emit = defineEmits<{
  setGlobalPermissionMode: [mode: ToolMode];
  setPermission: [name: string, mode: ToolMode];
  setFileWorkspaceBoundary: [value: boolean];
  removeWorkflowWhitelistTool: [name: string];
  removeWorkflowWhitelistBash: [command: string];
  clearWorkflowToolWhitelist: [];
}>();

const permissionOptions = [
  { value: "auto", label: "Auto" },
  { value: "ask", label: "Ask" },
] as const;

const fileBoundaryOptions = computed(() => [
  {
    value: "all",
    label: t("settings.perms.fileBoundaryAll"),
    disabled: !props.fileWorkspaceBoundaryReady || props.fileWorkspaceBoundaryBusy,
  },
  {
    value: "workspace",
    label: t("settings.perms.fileBoundaryWorkspace"),
    disabled: !props.fileWorkspaceBoundaryReady || props.fileWorkspaceBoundaryBusy,
  },
]);

const fileBoundaryMode = computed<FileBoundaryMode>(() =>
  props.fileWorkspaceBoundaryEnabled ? "workspace" : "all",
);

const workflowWhitelistEmpty = computed(
  () =>
    props.workflowToolWhitelist.tools.length === 0
    && props.workflowToolWhitelist.bashCommands.length === 0,
);

function getToolMode(name: string): ToolMode {
  const item = [...props.toolList, ...props.behaviorList].find((entry) => entry.name === name);
  return props.toolPermissions[name] ?? (item?.defaultMode ?? "ask");
}

function setFileBoundaryMode(mode: string) {
  if (!props.fileWorkspaceBoundaryReady || props.fileWorkspaceBoundaryBusy) return;
  emit("setFileWorkspaceBoundary", mode === "workspace");
}
</script>

<template>
  <div class="settings-section">
    <div class="perm-shell">
      <div class="perm-header">
        <div class="perm-heading">
          <div class="section-label">{{ t("settings.perms.title") }}</div>
          <p class="section-desc">{{ t("settings.perms.desc") }}</p>
        </div>

        <Transition name="fade">
          <div
            v-if="permSaveMsg"
            class="perm-toast"
            role="status"
            aria-live="polite"
          >
            {{ permSaveMsg }}
          </div>
        </Transition>
      </div>

      <div class="perm-panel">
        <div class="perm-panel-heading">
          <div class="perm-panel-title">{{ t("settings.perms.fileBoundaryTitle") }}</div>
          <div class="perm-panel-desc">{{ t("settings.perms.fileBoundaryDesc") }}</div>
        </div>

        <div class="perm-row perm-simple-row">
          <div class="perm-info">
            <span class="perm-name perm-text-name">{{ t("settings.perms.fileBoundaryScope") }}</span>
          </div>

          <div class="perm-control perm-boundary-control">
            <BaseSegmented
              v-if="fileWorkspaceBoundaryReady"
              size="sm"
              :model-value="fileBoundaryMode"
              :options="fileBoundaryOptions"
              :aria-label="t('settings.perms.fileBoundaryTitle')"
              @update:model-value="setFileBoundaryMode"
            />
            <span v-else class="perm-segmented-placeholder" aria-hidden="true" />
          </div>
        </div>
      </div>

      <div class="perm-panel">
        <div class="perm-panel-heading">
          <div class="perm-panel-title">{{ t("settings.perms.behaviorTitle") }}</div>
          <div class="perm-panel-desc">{{ t("settings.perms.behaviorDesc") }}</div>
        </div>

        <div class="perm-table-head perm-behavior-head" aria-hidden="true">
          <span>{{ t("settings.perms.columnBehavior") }}</span>
          <span>{{ t("settings.perms.columnMode") }}</span>
        </div>

        <div class="perm-list">
          <div
            v-for="behavior in behaviorList"
            :key="behavior.name"
            class="perm-row perm-behavior-row"
          >
            <div class="perm-info">
              <span class="perm-name perm-behavior-name">{{ behavior.label }}</span>
              <span class="perm-desc">{{ behavior.desc }}</span>
            </div>

            <div class="perm-control">
              <BaseSegmented
                size="sm"
                :model-value="getToolMode(behavior.name)"
                :options="[...permissionOptions]"
                @update:model-value="emit('setPermission', behavior.name, $event as ToolMode)"
              />
            </div>
          </div>
        </div>
      </div>

      <div class="perm-panel">
        <div class="perm-panel-heading perm-panel-heading-with-action">
          <div>
            <div class="perm-panel-title">{{ t("settings.perms.workflowWhitelistTitle") }}</div>
            <div class="perm-panel-desc">{{ t("settings.perms.workflowWhitelistDesc") }}</div>
          </div>
          <BaseButton
            v-if="workflowWhitelistReady && !workflowWhitelistEmpty"
            class="perm-whitelist-clear"
            size="sm"
            :disabled="workflowWhitelistBusy"
            @click="emit('clearWorkflowToolWhitelist')"
          >
            {{ t("settings.perms.workflowWhitelistClearAll") }}
          </BaseButton>
        </div>

        <div v-if="!workflowWhitelistReady" class="perm-whitelist-loading">
          {{ t("settings.perms.workflowWhitelistLoading") }}
        </div>
        <div v-else-if="workflowWhitelistEmpty" class="perm-whitelist-empty">
          {{ t("settings.perms.workflowWhitelistEmpty") }}
        </div>
        <template v-else>
          <div v-if="workflowToolWhitelist.tools.length > 0" class="perm-whitelist-group">
            <div class="perm-whitelist-group-label">{{ t("settings.perms.workflowWhitelistTools") }}</div>
            <ul class="perm-whitelist-list">
              <li
                v-for="toolName in workflowToolWhitelist.tools"
                :key="`tool:${toolName}`"
                class="perm-whitelist-item"
              >
                <code class="perm-whitelist-value">{{ toolName }}</code>
                <BaseButton
                  class="perm-whitelist-remove"
                  size="sm"
                  :disabled="workflowWhitelistBusy"
                  :aria-label="t('settings.perms.workflowWhitelistRemoveTool', toolName)"
                  @click="emit('removeWorkflowWhitelistTool', toolName)"
                >
                  {{ t("settings.perms.workflowWhitelistRemove") }}
                </BaseButton>
              </li>
            </ul>
          </div>
          <div v-if="workflowToolWhitelist.bashCommands.length > 0" class="perm-whitelist-group">
            <div class="perm-whitelist-group-label">{{ t("settings.perms.workflowWhitelistBash") }}</div>
            <ul class="perm-whitelist-list">
              <li
                v-for="command in workflowToolWhitelist.bashCommands"
                :key="`bash:${command}`"
                class="perm-whitelist-item"
              >
                <code class="perm-whitelist-value">{{ command }}</code>
                <BaseButton
                  class="perm-whitelist-remove"
                  size="sm"
                  :disabled="workflowWhitelistBusy"
                  :aria-label="t('settings.perms.workflowWhitelistRemoveBash', command)"
                  @click="emit('removeWorkflowWhitelistBash', command)"
                >
                  {{ t("settings.perms.workflowWhitelistRemove") }}
                </BaseButton>
              </li>
            </ul>
          </div>
        </template>
      </div>

      <div class="perm-panel">
        <div class="perm-panel-heading">
          <div class="perm-panel-title">{{ t("settings.perms.globalMode") }}</div>
          <div class="perm-panel-desc">{{ t("settings.perms.globalModeDesc") }}</div>
        </div>

        <div class="perm-row perm-simple-row">
          <div class="perm-info">
            <span class="perm-name perm-text-name">{{ t("settings.perms.globalModeDefault") }}</span>
          </div>

          <div class="perm-control">
            <BaseSegmented
              size="sm"
              :model-value="toolPermissionMode"
              :options="[...permissionOptions]"
              @update:model-value="emit('setGlobalPermissionMode', $event as ToolMode)"
            />
          </div>
        </div>
      </div>

      <div class="perm-panel">
        <div class="perm-panel-heading">
          <div class="perm-panel-title">{{ t("settings.perms.toolTitle") }}</div>
          <div class="perm-panel-desc">{{ t("settings.perms.toolDesc") }}</div>
        </div>

        <div class="perm-table-head" aria-hidden="true">
          <span>{{ t("settings.perms.columnTool") }}</span>
          <span>{{ t("settings.perms.columnMode") }}</span>
        </div>

        <div class="perm-list">
          <div
            v-for="tool in toolList"
            :key="tool.name"
            class="perm-row"
          >
            <div class="perm-info">
              <span class="perm-name">{{ tool.label }}</span>
              <span class="perm-desc">{{ tool.desc }}</span>
            </div>

            <div class="perm-control">
              <BaseSegmented
                size="sm"
                :model-value="getToolMode(tool.name)"
                :options="[...permissionOptions]"
                @update:model-value="emit('setPermission', tool.name, $event as ToolMode)"
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.perm-shell {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.perm-header {
  position: relative;
  padding-right: 88px;
  margin-bottom: 2px;
}

.perm-toast {
  position: absolute;
  top: 0;
  right: 0;
  display: inline-flex;
  align-items: center;
  min-height: 26px;
  padding: 0 10px;
  border-radius: 6px;
  border: 1px solid var(--status-good-border);
  background: color-mix(in srgb, var(--status-good-bg) 84%, var(--panel-bg) 16%);
  color: var(--status-good-fg);
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  pointer-events: none;
}

.perm-panel {
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: color-mix(in srgb, var(--panel-bg) 84%, var(--sidebar-bg) 16%);
  overflow: hidden;
}

.perm-panel-heading {
  padding: 12px 16px 10px;
  border-bottom: 1px solid var(--border-color);
}

.perm-panel-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-color);
}

.perm-panel-desc {
  margin-top: 3px;
  font-size: 12px;
  line-height: 1.45;
  color: var(--text-secondary);
}

.perm-segmented-placeholder {
  display: block;
  width: 116px;
  height: 28px;
  border: 1px solid color-mix(in srgb, var(--border-strong) 82%, var(--text-secondary) 18%);
  border-radius: 8px;
  background: color-mix(in srgb, var(--input-bg) 76%, var(--hover-bg) 24%);
  opacity: 0.55;
}

.perm-table-head {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 16px;
  align-items: center;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border-color);
  background: color-mix(in srgb, var(--panel-bg) 78%, var(--hover-bg) 22%);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--text-secondary);
}

.perm-list {
  display: flex;
  flex-direction: column;
}

.perm-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 16px;
  align-items: center;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-color);
  transition: background 0.15s ease;
}

.perm-simple-row {
  min-height: 52px;
}

.perm-row:last-child {
  border-bottom: none;
}

.perm-row:hover {
  background: color-mix(in srgb, var(--panel-bg) 82%, var(--hover-bg) 18%);
}

.perm-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.perm-name {
  font-size: 13px;
  font-weight: 600;
  font-family: var(--font-mono-identifier);
  color: var(--text-color);
}

.perm-behavior-name,
.perm-text-name {
  font-family: inherit;
}

.perm-desc {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.45;
}

.perm-control {
  width: 116px;
  flex-shrink: 0;
}

.perm-control :deep(.base-segmented) {
  display: flex;
  width: 100%;
}

.perm-control :deep(.base-segmented-item) {
  flex: 1;
  justify-content: center;
  white-space: nowrap;
}

.perm-boundary-control {
  width: 132px;
}

.perm-panel-heading-with-action {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.perm-whitelist-clear {
  flex-shrink: 0;
}

.perm-whitelist-loading,
.perm-whitelist-empty {
  padding: 14px 16px;
  font-size: 12px;
  line-height: 1.45;
  color: var(--text-secondary);
}

.perm-whitelist-group {
  border-top: 1px solid var(--border-color);
}

.perm-whitelist-group-label {
  padding: 10px 16px 6px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--text-secondary);
}

.perm-whitelist-list {
  margin: 0;
  padding: 0 16px 12px;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.perm-whitelist-item {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 10px;
  align-items: start;
  padding: 8px 10px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel-bg) 90%, var(--sidebar-bg) 10%);
}

.perm-whitelist-value {
  display: block;
  min-width: 0;
  font-size: 12px;
  line-height: 1.45;
  word-break: break-word;
  white-space: pre-wrap;
  color: var(--text-color);
}

.perm-whitelist-remove {
  flex-shrink: 0;
}

@media (max-width: 860px) {
  .perm-header {
    padding-right: 0;
  }

  .perm-toast {
    position: static;
    margin-bottom: 12px;
  }

  .perm-table-head {
    display: none;
  }

  .perm-row {
    grid-template-columns: 1fr;
    gap: 10px;
  }

  .perm-control {
    width: 100%;
  }

  .perm-segmented-placeholder {
    width: 100%;
  }
}
</style>
