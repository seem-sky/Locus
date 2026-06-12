
<script setup lang="ts">
import { computed } from "vue";
import type { TokenUsage } from "../../types";
import { t } from "../../i18n";
import {
  formatTokenCount,
  hasContextWindowUsage,
  metricI18nKey,
  shouldShowTokenUsageBar,
  visibleTokenUsageMetrics,
} from "./tokenUsageDisplay";

const props = defineProps<{
  tokenUsage: TokenUsage;
}>();

function formatUsd(n: number): string {
  if (n >= 1) return `$${n.toFixed(2)}`;
  if (n >= 0.01) return `$${n.toFixed(4)}`;
  return `$${n.toFixed(6)}`;
}

const hasPrice = computed(() => props.tokenUsage.pricedRounds > 0);
const visible = computed(() => shouldShowTokenUsageBar(props.tokenUsage));

const contextTokens = computed(() => props.tokenUsage.contextTokens);
const contextLimit = computed(() => props.tokenUsage.contextLimit);
const hasContext = computed(() => hasContextWindowUsage(props.tokenUsage));
const contextPercent = computed(() =>
  contextLimit.value > 0 ? Math.min(100, (contextTokens.value / contextLimit.value) * 100) : 0,
);
const contextIndicatorColor = computed(() => {
  const pct = contextPercent.value;
  if (pct >= 80) return "var(--context-danger, #e53e3e)";
  if (pct >= 60) return "var(--context-warning, #d69e2e)";
  return "var(--text-secondary)";
});

const visibleMetrics = computed(() => visibleTokenUsageMetrics(props.tokenUsage));

const contextSummary = computed(() => {
  if (!hasContext.value) return "";
  return t(
    "chat.tokenUsage.context",
    formatTokenCount(contextTokens.value),
    formatTokenCount(contextLimit.value),
    contextPercent.value.toFixed(1),
  );
});

const usageAriaLabel = computed(() => {
  const parts: string[] = [t("chat.tokenUsage.sessionTitle")];

  for (const metric of visibleMetrics.value) {
    parts.push(t(metricI18nKey(metric.key), formatTokenCount(metric.value)));
  }

  if (contextSummary.value) {
    parts.push(contextSummary.value);
  }

  if (hasPrice.value) {
    parts.push(t("chat.tokenUsage.cost", formatUsd(props.tokenUsage.totalCostUsd)));
  }

  return parts.join(" · ");
});

const contextAriaLabel = computed(() => contextSummary.value || usageAriaLabel.value);

</script>

<template>
  <div
    v-if="visible"
    class="token-usage-bar"
    :aria-label="usageAriaLabel"
  >
    <div
      v-if="hasContext"
      class="token-usage-group"
      role="meter"
      aria-valuemin="0"
      aria-valuemax="100"
      :aria-valuenow="contextPercent.toFixed(1)"
      :aria-label="contextAriaLabel"
      :aria-valuetext="contextAriaLabel"
      :style="{ color: contextIndicatorColor }"
    >
      <svg
        class="context-progress-ring"
        viewBox="0 0 16 16"
        aria-hidden="true"
      >
        <circle
          class="context-progress-track"
          cx="8"
          cy="8"
          r="5.2"
          pathLength="100"
        />
        <circle
          class="context-progress-value"
          cx="8"
          cy="8"
          r="5.2"
          pathLength="100"
          :stroke-dasharray="`${contextPercent} 100`"
        />
      </svg>
    </div>
    <div
      v-if="visibleMetrics.length > 0 || contextSummary || hasPrice"
      class="token-usage-metrics"
      aria-hidden="true"
    >
      <span
        v-for="metric in visibleMetrics"
        :key="metric.key"
        class="token-usage-metric"
      >
        {{ t(metricI18nKey(metric.key), formatTokenCount(metric.value)) }}
      </span>
      <span
        v-if="contextSummary"
        class="token-usage-metric token-usage-context"
      >
        {{ contextSummary }}
      </span>
      <span
        v-if="hasPrice"
        class="token-usage-metric token-usage-cost"
      >
        {{ t("chat.tokenUsage.cost", formatUsd(tokenUsage.totalCostUsd)) }}
      </span>
    </div>
  </div>
</template>

<style scoped>
.token-usage-bar {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex: 1 1 100%;
  flex-shrink: 0;
  align-self: center;
  min-width: 0;
  max-width: 100%;
  color: var(--text-secondary);
}

.token-usage-group {
  position: relative;
  width: 24px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  line-height: 0;
}

.context-progress-ring {
  width: 15px;
  height: 15px;
  display: block;
  flex-shrink: 0;
  transform: translateY(1px) rotate(-90deg);
}

.context-progress-track,
.context-progress-value {
  fill: none;
  stroke-width: 2;
}

.context-progress-track {
  stroke: color-mix(in srgb, currentColor 28%, transparent);
}

.context-progress-value {
  stroke: currentColor;
  stroke-linecap: round;
  transition: stroke-dasharray 0.2s ease, stroke 0.2s ease;
}

.token-usage-metrics {
  display: flex;
  flex: 1 1 auto;
  flex-wrap: wrap;
  align-items: center;
  align-content: center;
  gap: 4px 10px;
  min-width: 0;
  min-height: 28px;
  font-size: 11px;
  line-height: 1.35;
}

.token-usage-metric {
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.token-usage-context {
  color: inherit;
}

.token-usage-cost {
  color: var(--text-secondary);
}

</style>
