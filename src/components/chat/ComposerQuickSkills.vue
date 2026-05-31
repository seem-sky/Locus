<script setup lang="ts">
import { computed, ref } from "vue";
import { t } from "../../i18n";
import type { CommandDef } from "../../composables/chatInputIntents";
import {
  quickChatPinFromCommand,
  unpinQuickChatSkill,
} from "../../composables/useQuickChatSkills";
import type { SkillIntentItem, SkillManifest } from "../../types";
import BaseContextMenu from "../ui/BaseContextMenu.vue";

const props = withDefaults(defineProps<{
  commands: CommandDef[];
  activeSkills: SkillIntentItem[];
  skills?: SkillManifest[];
  disabled?: boolean;
}>(), {
  skills: () => [],
});

const emit = defineEmits<{
  (e: "toggle", command: CommandDef): void;
  (e: "unpin", command: CommandDef): void;
}>();

interface QuickSkillContextMenuState {
  x: number;
  y: number;
  command: CommandDef;
}

const ctxMenu = ref<QuickSkillContextMenuState | null>(null);

const activeSkillKeys = computed(() =>
  new Set(props.activeSkills.map((skill) => `${skill.source}:${skill.dirName}`)),
);

const ctxMenuSkillLabel = computed(() => {
  const command = ctxMenu.value?.command;
  if (!command) return "";
  return command.skill?.name || command.name;
});

function isActive(command: CommandDef): boolean {
  if (!command.skill) return false;
  return activeSkillKeys.value.has(`${command.skill.source}:${command.skill.dirName}`);
}

function label(command: CommandDef): string {
  return command.skill?.name || command.name;
}

function title(command: CommandDef): string {
  const trigger = command.name;
  const description = command.description?.trim();
  const hint = t("chat.quickSkills.contextMenuHint");
  if (description) return `${trigger} — ${description}\n${hint}`;
  return `${trigger}\n${hint}`;
}

function openContextMenu(event: MouseEvent, command: CommandDef) {
  if (props.disabled || !command.skill) return;
  event.preventDefault();
  event.stopPropagation();
  ctxMenu.value = {
    x: event.clientX,
    y: event.clientY,
    command,
  };
}

function closeContextMenu() {
  ctxMenu.value = null;
}

function removeFromQuickChatBar() {
  const command = ctxMenu.value?.command;
  const pin = command ? quickChatPinFromCommand(command) : null;
  if (!command || !pin) return;
  unpinQuickChatSkill(pin, props.skills, props.commands);
  emit("unpin", command);
  closeContextMenu();
}
</script>

<template>
  <div
    class="composer-quick-skills"
    role="toolbar"
    :aria-label="t('chat.quickSkills.toolbar')"
  >
    <span class="composer-quick-skills-label">{{ t("chat.quickSkills.label") }}</span>
    <div class="composer-quick-skills-list">
      <button
        v-for="command in commands"
        :key="command.name"
        type="button"
        class="composer-quick-skill-btn ui-select-none"
        :class="{ 'is-active': isActive(command) }"
        :disabled="disabled"
        :title="title(command)"
        :aria-pressed="isActive(command)"
        @click="emit('toggle', command)"
        @contextmenu.prevent="openContextMenu($event, command)"
      >
        {{ label(command) }}
      </button>
    </div>
  </div>

  <BaseContextMenu
    v-if="ctxMenu"
    class="composer-quick-skill-ctx-menu"
    :x="ctxMenu.x"
    :y="ctxMenu.y"
    :min-width="168"
    :aria-label="t('chat.quickSkills.contextMenu')"
    @close="closeContextMenu"
  >
    <div class="composer-quick-skill-ctx-caption" role="presentation">
      {{ ctxMenuSkillLabel }}
    </div>
    <button
      type="button"
      class="danger"
      role="menuitem"
      @mousedown.prevent
      @click.stop="removeFromQuickChatBar"
    >
      {{ t("chat.quickSkills.removeFromBar") }}
    </button>
  </BaseContextMenu>
</template>

<style scoped>
.composer-quick-skills {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-width: 0;
  flex: 1 1 100%;
}

.composer-quick-skills-label {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--text-secondary);
  opacity: 0.85;
}

.composer-quick-skills-list {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  flex: 1 1 auto;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: thin;
  padding-bottom: 1px;
}

.composer-quick-skill-btn {
  flex-shrink: 0;
  max-width: 160px;
  height: 24px;
  padding: 0 10px;
  border: 1px solid color-mix(in srgb, var(--border-color) 88%, transparent);
  border-radius: 999px;
  background: color-mix(in srgb, var(--panel-bg) 70%, var(--bg-color) 30%);
  color: var(--text-color);
  font: inherit;
  font-size: 12px;
  line-height: 1;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition:
    background 0.12s ease,
    border-color 0.12s ease,
    color 0.12s ease;
}

.composer-quick-skill-btn:hover:not(:disabled) {
  background: var(--hover-bg);
  border-color: color-mix(in srgb, var(--accent-color) 35%, var(--border-color));
}

.composer-quick-skill-btn.is-active {
  background: color-mix(in srgb, var(--accent-color) 16%, var(--panel-bg));
  border-color: color-mix(in srgb, var(--accent-color) 55%, var(--border-color));
  color: var(--text-color);
  font-weight: 600;
}

.composer-quick-skill-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.composer-quick-skill-btn:focus-visible {
  outline: 2px solid var(--accent-color);
  outline-offset: 1px;
}
</style>

<style>
.composer-quick-skill-ctx-menu .composer-quick-skill-ctx-caption {
  padding: 4px 10px 2px;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
