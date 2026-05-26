import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  luaGcMonitorClearSamples,
  luaGcMonitorExport,
  luaGcMonitorGetAnalysis,
  luaGcMonitorGetSamples,
  luaGcMonitorStart,
  luaGcMonitorStatus,
  luaGcMonitorStop,
  subscribeLuaGcSamples,
  type LuaGcAnalysis,
  type LuaGcMonitorStatus,
  type LuaGcSample,
} from "../services/luaGcMonitor";
import { buildLuaGcChartModel } from "./luaGcChart";

const POLL_MS = 500;

export function useLuaGcMonitor() {
  const status = ref<LuaGcMonitorStatus | null>(null);
  const samples = ref<LuaGcSample[]>([]);
  const analysis = ref<LuaGcAnalysis | null>(null);
  const loading = ref(false);
  const error = ref("");
  const sampleIntervalMs = ref(100);
  const showMemory = ref(true);
  const showDebt = ref(true);
  const showAlloc = ref(true);

  let unsubscribeSample: (() => void) | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  const chartModel = computed(() => buildLuaGcChartModel(samples.value));

  const latestSample = computed(() =>
    samples.value.length > 0 ? samples.value[samples.value.length - 1] : null,
  );

  async function refreshStatus() {
    try {
      status.value = await luaGcMonitorStatus();
    } catch (e) {
      error.value = (e as { message?: string })?.message ?? String(e);
    }
  }

  async function refreshSamples() {
    try {
      const response = await luaGcMonitorGetSamples({
        sessionId: status.value?.sessionId,
        maxPoints: 1200,
      });
      samples.value = response.samples;
    } catch (e) {
      error.value = (e as { message?: string })?.message ?? String(e);
    }
  }

  async function refreshAnalysis() {
    try {
      analysis.value = await luaGcMonitorGetAnalysis(status.value?.sessionId);
    } catch (e) {
      error.value = (e as { message?: string })?.message ?? String(e);
    }
  }

  async function start() {
    loading.value = true;
    error.value = "";
    try {
      status.value = await luaGcMonitorStart({
        sampleIntervalMs: sampleIntervalMs.value,
      });
      await refreshSamples();
      await refreshAnalysis();
    } catch (e) {
      error.value = (e as { message?: string })?.message ?? String(e);
    } finally {
      loading.value = false;
    }
  }

  async function stop() {
    loading.value = true;
    error.value = "";
    try {
      status.value = await luaGcMonitorStop("user");
      await refreshAnalysis();
    } catch (e) {
      error.value = (e as { message?: string })?.message ?? String(e);
    } finally {
      loading.value = false;
    }
  }

  async function clear() {
    loading.value = true;
    error.value = "";
    try {
      await luaGcMonitorClearSamples();
      samples.value = [];
      analysis.value = null;
      await refreshStatus();
    } catch (e) {
      error.value = (e as { message?: string })?.message ?? String(e);
    } finally {
      loading.value = false;
    }
  }

  async function exportData(format: "json" | "csv" = "json") {
    loading.value = true;
    error.value = "";
    try {
      const path = await luaGcMonitorExport({
        sessionId: status.value?.sessionId,
        format,
      });
      return path;
    } catch (e) {
      error.value = (e as { message?: string })?.message ?? String(e);
      return "";
    } finally {
      loading.value = false;
    }
  }

  function appendSample(sample: LuaGcSample) {
    const sessionId = status.value?.sessionId;
    if (sessionId && sample.sessionId !== sessionId) return;
    samples.value = [...samples.value, sample].slice(-1200);
  }

  function startPolling() {
    stopPolling();
    pollTimer = setInterval(() => {
      void refreshSamples();
      void refreshAnalysis();
    }, POLL_MS);
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  watch(
    () => status.value?.active,
    (active) => {
      if (active) startPolling();
      else stopPolling();
    },
  );

  onMounted(async () => {
    await refreshStatus();
    if (status.value?.active) {
      await refreshSamples();
      await refreshAnalysis();
      startPolling();
    }
    unsubscribeSample = await subscribeLuaGcSamples((sample) => {
      appendSample(sample);
    });
  });

  onUnmounted(() => {
    stopPolling();
    unsubscribeSample?.();
    unsubscribeSample = null;
  });

  return {
    status,
    samples,
    analysis,
    loading,
    error,
    sampleIntervalMs,
    showMemory,
    showDebt,
    showAlloc,
    chartModel,
    latestSample,
    refreshStatus,
    refreshSamples,
    refreshAnalysis,
    start,
    stop,
    clear,
    exportData,
  };
}
