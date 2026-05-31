
<script setup lang="ts">
import { computed } from "vue";
import type { TokenUsage } from "../../types";
import { t } from "../../i18n";
import {
  buildTokenUsageMetrics,
  formatTokenCount,
  hasContextWindowUsage,
  hasSessionTokenUsage,
  metricI18nKey,
  sessionInputTokenTotal,
  shouldShowTokenUsageBar,
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
const hasSessionTotals = computed(() => hasSessionTokenUsage(props.tokenUsage));
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

const compactLabel = computed(() => {
  if (!hasSessionTotals.value) return "";
  return t(
    "chat.tokenUsage.sessionCompact",
    formatTokenCount(sessionInputTokenTotal(props.tokenUsage)),
    formatTokenCount(props.tokenUsage.totalOutputTokens),
  );
});

const usageTooltip = computed(() => {
  const u = props.tokenUsage;
  const parts: string[] = [t("chat.tokenUsage.sessionTitle")];

  for (const metric of buildTokenUsageMetrics(u)) {
    if (metric.value <= 0 && metric.key !== "cached-input-write" && metric.key !== "cached-input-read") {
      continue;
    }
    parts.push(
      t(metricI18nKey(metric.key), formatTokenCount(metric.value)),
    );
  }

  if (hasContext.value) {
    parts.push(
      t(
        "chat.tokenUsage.context",
        formatTokenCount(contextTokens.value),
        formatTokenCount(contextLimit.value),
        contextPercent.value.toFixed(1),
      ),
    );
  }

  if (hasPrice.value) {
    parts.push(t("chat.tokenUsage.cost", formatUsd(u.totalCostUsd)));
  }

  return parts.join(" · ");
});

const contextAriaLabel = computed(() => {
  if (!hasContext.value) return usageTooltip.value;
  return t(
    "chat.tokenUsage.context",
    formatTokenCount(contextTokens.value),
    formatTokenCount(contextLimit.value),
    contextPercent.value.toFixed(1),
  );
});

</script>

<template>
  <div
    v-if="visible"
    class="token-usage-bar"
    :aria-label="usageTooltip"
    tabindex="0"
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
    <span
      v-if="hasSessionTotals"
      class="token-usage-compact"
      aria-hidden="true"
    >{{ compactLabel }}</span>
    <span class="token-usage-tooltip">{{ usageTooltip }}</span>
  </div>
</template>

<style scoped>
.token-usage-bar {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  align-self: center;
  max-width: min(220px, 42vw);
  color: var(--text-secondary);
  cursor: default;
  outline: none;
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

.token-usage-compact {
  font-size: 11px;
  line-height: 1.2;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.token-usage-tooltip {
  position: absolute;
  left: 50%;
  bottom: calc(100% + 6px);
  z-index: 35;
  max-width: 320px;
  padding: 4px 7px;
  border: 1px solid var(--border-color);
  border-radius: 5px;
  background: var(--surface-elevated, var(--panel-bg));
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.16);
  color: var(--text-color);
  pointer-events: none;
  overflow: hidden;
  font-size: 11px;
  line-height: 1.35;
  opacity: 0;
  transform: translate(-50%, 3px);
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: opacity 0.1s ease, transform 0.1s ease;
}

.token-usage-bar:hover .token-usage-tooltip,
.token-usage-bar:focus-visible .token-usage-tooltip {
  opacity: 1;
  transform: translate(-50%, 0);
  white-space: normal;
}

</style>
