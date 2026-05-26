<script setup lang="ts">
import { ref } from "vue";
import { t } from "../i18n";
import LuaGcMonitorPanel from "./LuaGcMonitorPanel.vue";

interface PerfTool {
  id: string;
  labelKey: string;
  component: typeof LuaGcMonitorPanel;
}

const tools: PerfTool[] = [
  { id: "lua-gc", labelKey: "perf.tab.luaGc", component: LuaGcMonitorPanel },
];

const activeTool = ref(tools[0].id);
</script>

<template>
  <div class="perf-view">
    <div class="perf-sub-tabs">
      <button
        v-for="tool in tools"
        :key="tool.id"
        class="perf-sub-tab"
        :class="{ active: activeTool === tool.id }"
        @click="activeTool = tool.id"
      >
        {{ t(tool.labelKey) }}
      </button>
    </div>
    <div class="perf-content">
      <component
        v-if="activeTool && tools.find(t => t.id === activeTool)"
        :is="tools.find(t => t.id === activeTool)?.component"
        :open="true"
        :show-close="false"
      />
    </div>
  </div>
</template>

<style scoped>
.perf-view {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  background: var(--panel-bg);
}

.perf-sub-tabs {
  display: flex;
  align-items: center;
  gap: 0;
  padding: 0 12px;
  height: 38px;
  flex-shrink: 0;
  background: var(--sidebar-bg);
  border-bottom: 1px solid var(--border-color);
}

.perf-sub-tab {
  -webkit-app-region: no-drag;
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  position: relative;
  padding: 0 14px;
  height: 100%;
  border: none;
  background: none;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: color 0.15s ease;
  line-height: 1;
  white-space: nowrap;
}

.perf-sub-tab:hover {
  color: var(--text-color);
}

.perf-sub-tab.active {
  color: var(--text-color);
}

.perf-sub-tab.active::after {
  content: "";
  position: absolute;
  bottom: 1px;
  left: 14px;
  right: 14px;
  height: 1px;
  background: var(--accent-color);
  border-radius: 999px;
  opacity: 0.72;
}

.perf-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
  padding: 0 0 0 16px;
}
</style>
