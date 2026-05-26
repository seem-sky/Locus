<script setup lang="ts">
import { ChartNoAxesCombined } from "lucide";
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { drawLuaGcChart } from "../composables/luaGcChart";
import { useLuaGcMonitor } from "../composables/useLuaGcMonitor";
import { t } from "../i18n";
import BaseButton from "./ui/BaseButton.vue";
import LucideIcon from "./icons/LucideIcon.vue";

const props = withDefaults(defineProps<{
  open: boolean;
  showClose?: boolean;
}>(), {
  open: false,
  showClose: true,
});

const emit = defineEmits<{
  close: [];
}>();

// emit is used conditionally in template (modal mode)
void emit;

const monitor = useLuaGcMonitor();
const chartRef = ref<HTMLCanvasElement | null>(null);
const exportPath = ref("");

function resizeChart() {
  const canvas = chartRef.value;
  if (!canvas) return;
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.floor(rect.width * dpr));
  canvas.height = Math.max(1, Math.floor(rect.height * dpr));
  const ctx = canvas.getContext("2d");
  if (ctx) ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  drawLuaGcChart(canvas, monitor.chartModel.value, {
    showMemory: monitor.showMemory.value,
    showDebt: monitor.showDebt.value,
    showAlloc: monitor.showAlloc.value,
  });
}

watch(
  () => [props.open, monitor.chartModel.value, monitor.showMemory.value, monitor.showDebt.value, monitor.showAlloc.value],
  () => {
    if (!props.open) return;
    nextTick(() => resizeChart());
  },
  { deep: true },
);

let resizeObserver: ResizeObserver | null = null;

onMounted(() => {
  if (chartRef.value) {
    resizeObserver = new ResizeObserver(() => resizeChart());
    resizeObserver.observe(chartRef.value);
  }
});

onUnmounted(() => {
  resizeObserver?.disconnect();
  resizeObserver = null;
});

async function handleExport(format: "json" | "csv") {
  const path = await monitor.exportData(format);
  if (path) exportPath.value = path;
}
</script>

<template v-if="props.showClose">
  <div v-if="open" class="lua-gc-panel-backdrop" @click.self="emit('close')">
    <section class="lua-gc-panel" role="dialog" :aria-label="t('luaGc.title')">
      <header class="lua-gc-header">
        <div class="lua-gc-title-row">
          <LucideIcon :icon="ChartNoAxesCombined" :size="16" />
          <h2 class="lua-gc-title">{{ t("luaGc.title") }}</h2>
          <span
            class="lua-gc-status-pill"
            :class="monitor.status.value?.active ? 'is-active' : 'is-idle'"
          >
            {{ monitor.status.value?.active ? t("luaGc.status.recording") : t("luaGc.status.idle") }}
          </span>
        </div>
        <button type="button" class="lua-gc-close" :aria-label="t('common.close')" @click="emit('close')">
          &times;
        </button>
      </header>

      <p v-if="monitor.error.value" class="lua-gc-error">{{ monitor.error.value }}</p>
      <p
        v-else-if="monitor.status.value && !monitor.status.value.runtimeAvailable"
        class="lua-gc-hint"
      >
        {{ monitor.status.value.runtimeMessage || t("luaGc.runtimeMissing") }}
      </p>

      <div class="lua-gc-metrics">
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.memory") }}</span>
          <span class="lua-gc-metric-value">
            {{ monitor.latestSample.value ? monitor.latestSample.value.memoryKb.toFixed(1) : "—" }} KB
          </span>
        </div>
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.debt") }}</span>
          <span class="lua-gc-metric-value">
            {{ monitor.latestSample.value ? monitor.latestSample.value.gcDebtKb.toFixed(1) : "—" }} KB
          </span>
        </div>
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.allocRate") }}</span>
          <span class="lua-gc-metric-value">
            {{ monitor.latestSample.value ? monitor.latestSample.value.allocKbSinceLast.toFixed(1) : "—" }} KB
          </span>
        </div>
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.samples") }}</span>
          <span class="lua-gc-metric-value">{{ monitor.samples.value.length }}</span>
        </div>
      </div>

      <div class="lua-gc-chart-wrap">
        <canvas ref="chartRef" class="lua-gc-chart" />
      </div>

      <div class="lua-gc-legend">
        <label><input v-model="monitor.showMemory.value" type="checkbox" /> {{ t("luaGc.legend.memory") }}</label>
        <label><input v-model="monitor.showDebt.value" type="checkbox" /> {{ t("luaGc.legend.debt") }}</label>
        <label><input v-model="monitor.showAlloc.value" type="checkbox" /> {{ t("luaGc.legend.alloc") }}</label>
      </div>

      <div class="lua-gc-controls">
        <label class="lua-gc-interval">
          <span>{{ t("luaGc.interval") }}</span>
          <input
            v-model.number="monitor.sampleIntervalMs.value"
            type="number"
            min="50"
            max="2000"
            step="50"
            :disabled="monitor.status.value?.active"
          />
          <span>ms</span>
        </label>
        <div class="lua-gc-actions">
          <BaseButton
            v-if="!monitor.status.value?.active"
            :disabled="monitor.loading.value"
            @click="monitor.start()"
          >
            {{ t("luaGc.start") }}
          </BaseButton>
          <BaseButton
            v-else
            :disabled="monitor.loading.value"
            @click="monitor.stop()"
          >
            {{ t("luaGc.stop") }}
          </BaseButton>
          <BaseButton :disabled="monitor.loading.value" @click="monitor.clear()">
            {{ t("luaGc.clear") }}
          </BaseButton>
          <BaseButton :disabled="monitor.loading.value" @click="handleExport('json')">
            {{ t("luaGc.exportJson") }}
          </BaseButton>
          <BaseButton :disabled="monitor.loading.value" @click="handleExport('csv')">
            {{ t("luaGc.exportCsv") }}
          </BaseButton>
        </div>
      </div>

      <p v-if="exportPath" class="lua-gc-export-path">{{ t("luaGc.exportedTo", exportPath) }}</p>

      <section v-if="monitor.analysis.value" class="lua-gc-analysis">
        <h3 class="lua-gc-section-title">{{ t("luaGc.analysisTitle") }}</h3>
        <ul v-if="monitor.analysis.value.alerts.length" class="lua-gc-alerts">
          <li
            v-for="(alert, index) in monitor.analysis.value.alerts"
            :key="`${alert.kind}-${index}`"
            :class="`lua-gc-alert lua-gc-alert-${alert.severity}`"
          >
            {{ alert.message }}
          </li>
        </ul>
        <p v-else class="lua-gc-hint">{{ t("luaGc.noAlerts") }}</p>
        <ul class="lua-gc-suggestions">
          <li
            v-for="(item, index) in monitor.analysis.value.suggestions"
            :key="index"
          >
            {{ item }}
          </li>
        </ul>
      </section>
    </section>
  </div>
</template>

<!-- showClose=false: standalone page mode -->
<template v-else>
  <div v-if="open" class="lua-gc-panel lua-gc-panel--standalone">
    <section class="lua-gc-panel" role="dialog" :aria-label="t('luaGc.title')">
      <header class="lua-gc-header">
        <div class="lua-gc-title-row">
          <LucideIcon :icon="ChartNoAxesCombined" :size="16" />
          <h2 class="lua-gc-title">{{ t("luaGc.title") }}</h2>
          <span
            class="lua-gc-status-pill"
            :class="monitor.status.value?.active ? 'is-active' : 'is-idle'"
          >
            {{ monitor.status.value?.active ? t("luaGc.status.recording") : t("luaGc.status.idle") }}
          </span>
        </div>
      </header>

      <p v-if="monitor.error.value" class="lua-gc-error">{{ monitor.error.value }}</p>
      <p
        v-else-if="monitor.status.value && !monitor.status.value.runtimeAvailable"
        class="lua-gc-hint"
      >
        {{ monitor.status.value.runtimeMessage || t("luaGc.runtimeMissing") }}
      </p>

      <div class="lua-gc-metrics">
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.memory") }}</span>
          <span class="lua-gc-metric-value">
            {{ monitor.latestSample.value ? monitor.latestSample.value.memoryKb.toFixed(1) : "—" }} KB
          </span>
        </div>
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.debt") }}</span>
          <span class="lua-gc-metric-value">
            {{ monitor.latestSample.value ? monitor.latestSample.value.gcDebtKb.toFixed(1) : "—" }} KB
          </span>
        </div>
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.allocRate") }}</span>
          <span class="lua-gc-metric-value">
            {{ monitor.latestSample.value ? monitor.latestSample.value.allocKbSinceLast.toFixed(1) : "—" }} KB
          </span>
        </div>
        <div class="lua-gc-metric">
          <span class="lua-gc-metric-label">{{ t("luaGc.metrics.samples") }}</span>
          <span class="lua-gc-metric-value">{{ monitor.samples.value.length }}</span>
        </div>
      </div>

      <div class="lua-gc-chart-wrap">
        <canvas ref="chartRef" class="lua-gc-chart" />
      </div>

      <div class="lua-gc-legend">
        <label><input v-model="monitor.showMemory.value" type="checkbox" /> {{ t("luaGc.legend.memory") }}</label>
        <label><input v-model="monitor.showDebt.value" type="checkbox" /> {{ t("luaGc.legend.debt") }}</label>
        <label><input v-model="monitor.showAlloc.value" type="checkbox" /> {{ t("luaGc.legend.alloc") }}</label>
      </div>

      <div class="lua-gc-controls">
        <label class="lua-gc-interval">
          <span>{{ t("luaGc.interval") }}</span>
          <input
            v-model.number="monitor.sampleIntervalMs.value"
            type="number"
            min="50"
            max="2000"
            step="50"
            :disabled="monitor.status.value?.active"
          />
          <span>ms</span>
        </label>
        <div class="lua-gc-actions">
          <BaseButton
            v-if="!monitor.status.value?.active"
            :disabled="monitor.loading.value"
            @click="monitor.start()"
          >
            {{ t("luaGc.start") }}
          </BaseButton>
          <BaseButton
            v-else
            :disabled="monitor.loading.value"
            @click="monitor.stop()"
          >
            {{ t("luaGc.stop") }}
          </BaseButton>
          <BaseButton :disabled="monitor.loading.value" @click="monitor.clear()">
            {{ t("luaGc.clear") }}
          </BaseButton>
          <BaseButton :disabled="monitor.loading.value" @click="handleExport('json')">
            {{ t("luaGc.exportJson") }}
          </BaseButton>
          <BaseButton :disabled="monitor.loading.value" @click="handleExport('csv')">
            {{ t("luaGc.exportCsv") }}
          </BaseButton>
        </div>
      </div>

      <p v-if="exportPath" class="lua-gc-export-path">{{ t("luaGc.exportedTo", exportPath) }}</p>

      <section v-if="monitor.analysis.value" class="lua-gc-analysis">
        <h3 class="lua-gc-section-title">{{ t("luaGc.analysisTitle") }}</h3>
        <ul v-if="monitor.analysis.value.alerts.length" class="lua-gc-alerts">
          <li
            v-for="(alert, index) in monitor.analysis.value.alerts"
            :key="`${alert.kind}-${index}`"
            :class="`lua-gc-alert lua-gc-alert-${alert.severity}`"
          >
            {{ alert.message }}
          </li>
        </ul>
        <p v-else class="lua-gc-hint">{{ t("luaGc.noAlerts") }}</p>
        <ul class="lua-gc-suggestions">
          <li
            v-for="(item, index) in monitor.analysis.value.suggestions"
            :key="index"
          >
            {{ item }}
          </li>
        </ul>
      </section>
    </section>
  </div>
</template>

<style scoped>
.lua-gc-panel-backdrop {
  position: fixed;
  inset: 0;
  z-index: 220;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: color-mix(in srgb, var(--bg-color) 35%, transparent);
  backdrop-filter: blur(2px);
}

.lua-gc-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: min(920px, 100%);
  max-height: min(88vh, 900px);
  padding: 14px 16px 16px;
  border-radius: 10px;
  border: 1px solid var(--border-color);
  background: var(--panel-bg);
  box-shadow: 0 16px 48px rgba(15, 23, 42, 0.18);
  overflow: auto;
}

.lua-gc-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.lua-gc-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.lua-gc-title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.lua-gc-status-pill {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid var(--border-color);
}

.lua-gc-status-pill.is-active {
  color: var(--status-good-fg);
  background: color-mix(in srgb, var(--status-good-bg) 70%, transparent);
}

.lua-gc-status-pill.is-idle {
  color: var(--text-secondary);
  background: var(--hover-bg);
}

.lua-gc-close {
  border: none;
  background: none;
  color: var(--text-secondary);
  font-size: 22px;
  line-height: 1;
  cursor: pointer;
  padding: 0 4px;
}

.lua-gc-error {
  margin: 0;
  color: var(--status-danger-fg);
  font-size: 12px;
}

.lua-gc-hint {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.45;
}

.lua-gc-metrics {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}

.lua-gc-metric {
  padding: 8px 10px;
  border-radius: 6px;
  border: 1px solid color-mix(in srgb, var(--border-color) 80%, transparent);
  background: color-mix(in srgb, var(--hover-bg) 70%, var(--panel-bg));
}

.lua-gc-metric-label {
  display: block;
  font-size: 11px;
  color: var(--text-secondary);
}

.lua-gc-metric-value {
  display: block;
  margin-top: 4px;
  font-size: 14px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.lua-gc-chart-wrap {
  height: 220px;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, var(--border-color) 75%, transparent);
  overflow: hidden;
}

.lua-gc-chart {
  width: 100%;
  height: 100%;
  display: block;
}

.lua-gc-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  font-size: 12px;
  color: var(--text-secondary);
}

.lua-gc-controls {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.lua-gc-interval {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}

.lua-gc-interval input {
  width: 72px;
  padding: 4px 6px;
  border-radius: 4px;
  border: 1px solid var(--border-color);
  background: var(--panel-bg);
  color: var(--text-color);
}

.lua-gc-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.lua-gc-export-path {
  margin: 0;
  font-size: 11px;
  color: var(--text-secondary);
  word-break: break-all;
}

.lua-gc-analysis {
  border-top: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
  padding-top: 10px;
}

.lua-gc-section-title {
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 600;
}

.lua-gc-alerts,
.lua-gc-suggestions {
  margin: 0 0 8px;
  padding-left: 18px;
  font-size: 12px;
  line-height: 1.45;
}

.lua-gc-alert-warn {
  color: var(--status-warn-fg);
}

.lua-gc-alert-info {
  color: var(--text-secondary);
}

/* Standalone page mode */
.lua-gc-panel.lua-gc-panel--standalone {
  width: 100%;
  height: 100%;
}

.lua-gc-panel--standalone .lua-gc-panel {
  width: 100%;
  height: 100%;
  max-height: none;
  border-radius: 0;
  border: none;
  box-shadow: none;
}
</style>
