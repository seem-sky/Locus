# Lua GC 实时监控

Locus 在 Unity **Play Mode** 下通过 xLua 采样 Lua 虚拟机内存与 GC 指标，经命名管道推送到 Tauri，供内置面板、View 包与 Agent 工具分析。

## 快速开始

1. 打开 Unity 项目并连接 Locus。
2. 进入 **Play Mode**。
3. 在会话工具栏点击 **Lua GC**，开始录制（默认 100ms 间隔）。
4. 查看实时曲线、规则告警，或导出 JSON/CSV。

数据落盘目录：`Library/Locus/LuaGc/<sessionId>/`。

## xLua 注册

若面板提示未检测到 xLua，需在项目中注册 `LuaEnv`：

```csharp
// 示例：在初始化 xLua 后
Locus.LuaGcBootstrap.Register(() => luaEnv);
```

也可通过 `unity_execute` 在 Play Mode 执行一次性注册片段（参见 `locus_unity/Editor/LocusBridge.LuaGc.cs`）。

## 指标说明

| 字段 | 含义 |
|------|------|
| `memoryKb` | Lua 堆内存（KB） |
| `gcDebtKb` | GC 债务（Lua 5.4） |
| `allocKbSinceLast` | 距上次采样的内存增量（分配速率代理） |
| `gcPhase` | 推断的 GC 阶段（pause / propagate / atomic / sweep） |

## 内置面板 vs View 包

- **内置面板**：`LuaGcMonitorPanel`（主窗口与 Unity 嵌入会话工具栏 **Lua GC** 按钮）。
- **View 包**：使用 `view_create` 且 `template: "lua-gc-monitor"`，在 `Locus/views/lua-gc-monitor/` 生成可定制仪表盘；实时曲线仍建议用内置面板。

```json
{
  "id": "lua-gc-monitor",
  "name": "Lua GC Monitor",
  "template": "lua-gc-monitor",
  "icon": "ChartNoAxesCombined"
}
```

## Agent 工作流

1. Play Mode 下让用户开始录制，或确认已有会话数据。
2. 调用 **`lua_gc_analyze`** 获取规则告警与最近采样摘要。
3. 结合 **`knowledge/skill/gc.md`** 给出静态优化建议（表复用、`table.concat`、事件解绑等）。

C# Profiler 热点请继续使用 `unity_run_states` / profiler 技能，与本功能互补。

## IPC 命令（Tauri）

- `lua_gc_monitor_start` / `stop` / `status`
- `lua_gc_monitor_get_samples`
- `lua_gc_monitor_get_analysis`
- `lua_gc_monitor_export`
- `lua_gc_monitor_clear_samples`

## 限制

- 仅 Play Mode（Edit Mode 通常无 Lua VM）。
- 高频采样会增加开销；可调到 250–500ms。
- View 包默认不直接调用 IPC，需自行扩展或读取 `Library/Locus/LuaGc` 导出文件。
