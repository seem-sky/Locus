# 性能优化标签页实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 新增"性能优化"顶级标签页，内置二级 Tab Bar，Lua GC 监控面板作为首个子工具迁移至此

**架构：** 在 App.vue tab-bar 新增"Performance"标签 → 懒加载 `PerformanceOptimizationView.vue` → 二级 Tab Bar 切换子工具（当前仅有 Lua GC）

**技术栈：** Vue 3, TypeScript, Pinia, Tauri

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `src/stores/ui.ts` | `activeTab` 添加 `"performance"`，新增 `performanceMounted` |
| `src/App.vue` | 新增性能优化 tab button + 懒加载视图组件 |
| `src/components/PerformanceOptimizationView.vue` | **新建** — 二级 Tab 容器，工具注册表 |
| `src/components/LuaGcMonitorPanel.vue` | 从 ChatWorkspaceView 迁移，成为 PerformanceOptimization 的子组件 |
| `src/language/en.json` | 添加国际化文案 |
| `src/language/zh.json` | 添加中文文案 |

---

## 任务 1：修改 ui store

**文件：**
- 修改：`src/stores/ui.ts:20`
- 修改：`src/stores/ui.ts:35-40`
- 修改：`src/stores/ui.ts:187-195`

- [ ] **步骤 1：修改 activeTab 类型**

将 `src/stores/ui.ts` 第 20 行：
```typescript
const activeTab = ref<"chat" | "collab" | "knowledge" | "asset" | "views" | "agent" | "settings">("chat");
```
改为：
```typescript
const activeTab = ref<"chat" | "collab" | "knowledge" | "asset" | "views" | "agent" | "settings" | "performance">("chat");
```

- [ ] **步骤 2：添加 performanceMounted ref**

在 `src/stores/ui.ts` 第 35-40 行附近（其他 mounted ref 之后）添加：
```typescript
const performanceMounted = ref(false);
```

- [ ] **步骤 3：在 setTab 中添加 performance 分支**

将 `src/stores/ui.ts` 第 187-195 行的 `setTab` 函数修改为：
```typescript
function setTab(tab: typeof activeTab.value) {
  activeTab.value = tab;
  if (tab === "collab") collabMounted.value = true;
  if (tab === "knowledge") knowledgeMounted.value = true;
  if (tab === "asset") assetMounted.value = true;
  if (tab === "views") viewMounted.value = true;
  if (tab === "agent") agentMounted.value = true;
  if (tab === "settings") settingsMounted.value = true;
  if (tab === "performance") performanceMounted.value = true;
}
```

- [ ] **步骤 4：在返回值中添加 performanceMounted**

在 `src/stores/ui.ts` 第 272-305 行的返回值对象中，添加：
```typescript
performanceMounted,
```

- [ ] **步骤 5：Commit**

```bash
git add src/stores/ui.ts
git commit -m "feat(ui): add performance tab state"
```

---

## 任务 2：添加国际化文案

**文件：**
- 修改：`src/language/en.json`
- 修改：`src/language/zh.json`

- [ ] **步骤 1：在 en.json 添加性能优化相关文案**

在 `src/language/en.json` 末尾（最后一个 `}` 之前）添加：
```json
,
  "perf.tab.performance": "Performance",
  "perf.tab.luaGc": "Lua GC",
  "perf.tab.frameAnalysis": "Frame Analysis",
  "perf.tab.memoryAnalysis": "Memory Analysis"
```

- [ ] **步骤 2：在 zh.json 添加中文文案**

在 `src/language/zh.json` 末尾（最后一个 `}` 之前）添加：
```json
,
  "perf.tab.performance": "性能优化",
  "perf.tab.luaGc": "Lua GC",
  "perf.tab.frameAnalysis": "帧率分析",
  "perf.tab.memoryAnalysis": "内存分析"
```

- [ ] **步骤 3：Commit**

```bash
git add src/language/en.json src/language/zh.json
git commit -m "feat(i18n): add performance tab labels"
```

---

## 任务 3：创建 PerformanceOptimizationView 组件

**文件：**
- 创建：`src/components/PerformanceOptimizationView.vue`

- [ ] **步骤 1：创建 PerformanceOptimizationView.vue**

创建文件 `src/components/PerformanceOptimizationView.vue`，内容如下：

```vue
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
        :is="tools.find(t => t.id === activeTool)?.component"
        v-if="activeTool === 'lua-gc'"
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
}
</style>
```

- [ ] **步骤 2：Commit**

```bash
git add src/components/PerformanceOptimizationView.vue
git commit -m "feat(perf): create PerformanceOptimizationView with Lua GC tab"
```

---

## 任务 4：修改 App.vue 添加性能优化 Tab

**文件：**
- 修改：`src/App.vue`

- [ ] **步骤 1：创建 performanceView lazy view state**

在 `src/App.vue` 第 183-191 行附近（在 agentView 定义之后）添加：

```typescript
const performanceView = createLazyViewState(
  () => import("./components/PerformanceOptimizationView.vue"),
  "loadPerformanceView",
);
```

- [ ] **步骤 2：添加 performanceViewComponent 等响应式变量**

在 `src/App.vue` 第 217-219 行附近添加：

```typescript
const performanceViewComponent = performanceView.component;
const performanceViewLoading = performanceView.loading;
const performanceViewError = performanceView.error;
```

- [ ] **步骤 3：添加 performance view 的 watch**

在 `src/App.vue` 第 250-254 行附近添加：

```typescript
watch(() => uiStore.performanceMounted, (mounted) => {
  if (!mounted) return;
  void performanceView.ensureLoaded();
}, { immediate: true });
```

- [ ] **步骤 4：在 tab-bar 中添加性能优化按钮**

在 `src/App.vue` 第 732-737 行（settings tab button 之前）添加：

```vue
<button
  class="tab-item"
  :class="{ active: uiStore.activeTab === 'performance' }"
  @click="uiStore.setTab('performance')"
>{{ t("perf.tab.performance") }}</button>
```

- [ ] **步骤 5：在 tab-content 中添加 performance view**

在 `src/App.vue` 第 944-950 行（settings view 条件渲染之前）添加：

```vue
<component
  :is="performanceViewComponent"
  v-if="uiStore.performanceMounted && performanceViewComponent"
  v-show="uiStore.activeTab === 'performance'"
/>
<div
  v-else-if="uiStore.performanceMounted && uiStore.activeTab === 'performance'"
  class="tab-loading-state"
  :class="{ 'is-loading': performanceViewLoading, 'is-error': !!performanceViewError }"
>
  {{ performanceViewError || t("common.loading") }}
</div>
```

- [ ] **步骤 6：Commit**

```bash
git add src/App.vue
git commit -m "feat(ui): add performance tab to App.vue"
```

---

## 任务 5：从 ChatWorkspaceView 移除 Lua GC 相关代码

**文件：**
- 修改：`src/components/ChatWorkspaceView.vue`

- [ ] **步骤 1：移除 LuaGcMonitorPanel import**

删除 `src/components/ChatWorkspaceView.vue` 第 23 行：
```typescript
import LuaGcMonitorPanel from "./LuaGcMonitorPanel.vue";
```

- [ ] **步骤 2：移除 luaGcPanelOpen ref**

删除 `src/components/ChatWorkspaceView.vue` 第 49 行：
```typescript
const luaGcPanelOpen = ref(false);
```

- [ ] **步骤 3：移除 Lua GC toolbar 和面板**

删除 `src/components/ChatWorkspaceView.vue` 中以下内容：
- 第 359-368 行的 `<div class="chat-lua-gc-toolbar">` 块（包含按钮）
- 第 369-373 行的 `<LuaGcMonitorPanel>` 组件

- [ ] **步骤 4：移除 ChatView 中的 ChartNoAxesCombined import 和相关使用**

检查 `src/components/ChatWorkspaceView.vue` 中是否还有其他 lua-gc 相关代码并清理。

- [ ] **步骤 5：Commit**

```bash
git add src/components/ChatWorkspaceView.vue
git commit -m "refactor(chat): remove Lua GC panel from ChatWorkspaceView"
```

---

## 任务 6：调整 LuaGcMonitorPanel 支持 always-open 模式

**文件：**
- 修改：`src/components/LuaGcMonitorPanel.vue`

- [ ] **步骤 1：修改 LuaGcMonitorPanel props**

检查 `LuaGcMonitorPanel.vue` 的 `open` 和 `showClose` props，确保：
- 当 `show-close="false"` 时，面板始终展开且隐藏关闭按钮
- 当作为独立页面时，不依赖 `@close` 事件

- [ ] **步骤 2：验证面板样式适配**

确保面板在 PerformanceOptimizationView 中作为主内容展示时样式正常（全高、无边框）。

- [ ] **步骤 3：Commit**

```bash
git add src/components/LuaGcMonitorPanel.vue
git commit -m "feat(perf): adapt LuaGcMonitorPanel for always-open mode"
```

---

## 任务 7：验证和测试

- [ ] **步骤 1：启动开发服务器**

```bash
bun run dev
```

- [ ] **步骤 2：验证功能**

1. 点击"性能优化"标签，验证二级 Tab Bar 显示
2. 点击"Lua GC"子标签，验证监控面板正常展示
3. 点击其他标签后返回，验证状态保持
4. 验证原有 Chat 页面无 Lua GC 浮动面板
5. 验证国际化文案正确显示

- [ ] **步骤 3：Commit 最终状态**

```bash
git add -A
git commit -m "feat(perf): complete performance tab with Lua GC migration"
```