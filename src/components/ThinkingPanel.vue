
<script setup lang="ts">
import { ref, watch, nextTick } from "vue";
import { t } from "../i18n";

defineOptions({
  inheritAttrs: false,
});

const props = defineProps<{
  thinking: string;
  isThinking: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const contentRef = ref<HTMLElement | null>(null);

watch(() => props.thinking, () => {
  nextTick(() => {
    const el = contentRef.value;
    if (el) el.scrollTop = el.scrollHeight;
  });
});
</script>

<template>
  <aside class="thinking-panel" :class="($attrs.class as string | undefined)">
    <div class="panel-header">
      <span class="panel-title">
        <span v-if="isThinking" class="thinking-dot" />
        {{ t("thinking.panel.title") }}
      </span>
      <button class="close-btn" @click="emit('close')" :title="t('thinking.panel.close')">&times;</button>
    </div>
    <div ref="contentRef" class="thinking-content">
      <pre v-if="thinking" class="thinking-text">{{ thinking }}</pre>
      <div v-else class="empty-hint">{{ t("thinking.panel.empty") }}</div>
    </div>
  </aside>
</template>

<style scoped>
.thinking-panel {
  display: flex;
  flex-direction: column;
  user-select: text;
  background: var(--sidebar-bg);
}

.panel-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
  min-height: 36px;
}

.panel-title {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  line-height: 1.2;
  display: flex;
  align-items: center;
  gap: 6px;
}

.thinking-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #3b82f6;
  animation: pulse 1.2s ease-in-out infinite;
  flex-shrink: 0;
}

@keyframes pulse {
  0%, 100% { opacity: 0.4; transform: scale(0.9); }
  50% { opacity: 1; transform: scale(1.1); }
}

.close-btn {
  width: 22px;
  height: 22px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 15px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  box-shadow: none;
  flex-shrink: 0;
}

.close-btn:hover {
  background: var(--hover-bg);
  color: var(--text-color);
}

.thinking-content {
  flex: 1;
  overflow-y: auto;
  padding: 12px 16px;
}

.thinking-text {
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
  font-family: var(--font-prose);
  margin: 0;
}

.empty-hint {
  text-align: center;
  color: var(--text-secondary);
  font-size: 13px;
  padding: 24px 0;
}
</style>
