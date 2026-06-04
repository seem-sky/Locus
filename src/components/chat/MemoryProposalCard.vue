<script setup lang="ts">
import { computed } from "vue";
import BaseButton from "../ui/BaseButton.vue";
import { t } from "../../i18n";
import type { MemoryCategory, MemoryProposal } from "../../types";

const props = defineProps<{
  proposal: MemoryProposal;
}>();

const emit = defineEmits<{
  apply: [proposalId: string];
  ignore: [proposalId: string];
}>();

const summaryText = computed(() => {
  const count = props.proposal.items.length;
  const confidence = Math.round(props.proposal.confidence * 100);
  return t("memory.proposal.summary", count, confidence);
});

const reminderText = computed(() => t("memory.proposal.reminder"));

function labelForCategory(category: MemoryCategory): string {
  return t(`memory.category.${category}`);
}

function labelForScope(scope: string): string {
  return scope === "user" ? t("memory.scope.user") : t("memory.scope.project");
}
</script>

<template>
  <div class="memory-card">
    <div class="memory-card-header">
      <div class="memory-card-title">{{ t("memory.proposal.title") }}</div>
      <div class="memory-card-meta">
        <span>{{ summaryText }}</span>
      </div>
    </div>
    <div class="memory-card-reminder">
      {{ reminderText }}
    </div>

    <div class="memory-card-items">
      <div
        v-for="(item, index) in proposal.items"
        :key="`${proposal.proposalId}-${index}`"
        class="memory-card-item"
      >
        <div class="memory-card-item-main">
          <span class="memory-card-item-kind">{{ labelForCategory(item.category) }}</span>
          <span class="memory-card-item-scope">{{ labelForScope(item.scope) }}</span>
        </div>
        <div class="memory-card-item-content">{{ item.content }}</div>
        <div v-if="item.tags.length > 0" class="memory-card-item-tags">
          <span v-for="tag in item.tags" :key="tag" class="memory-card-tag">{{ tag }}</span>
        </div>
      </div>
    </div>

    <div class="memory-card-actions">
      <template v-if="proposal.status === 'pending'">
        <BaseButton variant="neutral" @click="emit('ignore', proposal.proposalId)">{{ t("memory.proposal.ignore") }}</BaseButton>
        <BaseButton variant="primary" :disabled="proposal.status !== 'pending'" @click="emit('apply', proposal.proposalId)">{{ t("memory.proposal.apply") }}</BaseButton>
      </template>
      <template v-else-if="proposal.status === 'applying'">
        <span class="memory-card-status">{{ t("memory.proposal.applying") }}</span>
      </template>
      <template v-else-if="proposal.status === 'applied'">
        <span class="memory-card-status success">{{ t("memory.proposal.applied") }}</span>
      </template>
    </div>
  </div>
</template>

<style scoped>
.memory-card {
  margin: 6px 0 2px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel-bg) 88%, var(--bg-color) 12%);
  padding: 12px;
}

.memory-card-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.memory-card-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-color);
}

.memory-card-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-card-reminder {
  margin-top: 10px;
  padding: 8px 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: color-mix(in srgb, var(--bg-color) 72%, transparent);
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-secondary);
}

.memory-card-items {
  margin-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.memory-card-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: color-mix(in srgb, var(--bg-color) 68%, transparent);
}

.memory-card-item-main {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.memory-card-item-kind {
  font-size: 11px;
  color: var(--accent-color);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  font-weight: 600;
}

.memory-card-item-scope {
  font-size: 11px;
  color: var(--text-secondary);
}

.memory-card-item-content {
  font-size: 13px;
  line-height: 1.45;
  color: var(--text-color);
  white-space: pre-wrap;
  word-break: break-word;
}

.memory-card-item-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.memory-card-tag {
  padding: 2px 6px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--accent-color) 12%, transparent);
  color: var(--text-secondary);
  font-size: 11px;
}

.memory-card-actions {
  margin-top: 12px;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}

.memory-card-status {
  font-size: 12px;
  color: var(--text-secondary);
}

.memory-card-status.success {
  color: var(--accent-color);
}
</style>
