# 更新文档（Update Docs）

将 `README.md` / `CLAUDE.md` / `docs/*.md` 与代码库保持一致。本项目为 Unity（2022.3 LTS）+ C# + Lua（XLua）混合架构，文档更新采用混合策略：

- **可同步区块**：来自单一事实来源（Source of Truth）的表格/索引，用 `<!-- AUTO-GENERATED:... -->` 标记并允许更新。
- **手写章节**：架构说明、设计取舍、故障分析只做一致性校验，避免机械改写。

## 步骤 1：建立事实来源映射

| 来源                                                                                              | 类型                  | 可同步内容（示例）                   | 典型影响文档                                                               |
| ------------------------------------------------------------------------------------------------- | --------------------- | ------------------------------------ | -------------------------------------------------------------------------- |
| `Packages/manifest.json`                                                                          | Unity Package Manager | 关键依赖与版本（节选）               | `docs/gamekit-packages.md`、`README.md`                                    |
| `Assets/Gamekit/Games/App.cs`、`Loading.cs`、`StartGame.cs`                                       | C# 启动链路           | 启动阶段、关键步骤、事件与超时逻辑   | `docs/startup-flow.md`、`CLAUDE.md`、`README.md`                           |
| `Assets.Lua/Application.lua`、`Assets.Lua/Hall/Main.lua`、`Assets.Lua/Games.lua`                  | Lua 入口与模块注册    | Lua 入口、模块加载顺序、游戏注册关系 | `CLAUDE.md`、`docs/lua-core-architecture.md`、`docs/project-structure.md`  |
| `Assets/Gamekit/Core/**`、`Assets/Gamekit/Core/Common/**`                                         | Core 运行时实现       | Core 模块职责、接口与限制            | `docs/core-architecture.md`、`docs/core-*.md`、`docs/pack-architecture.md` |
| `Assets/Gamekit/Core/UI/**`、`Assets.Lua/Core/app/manage/context/**`                              | UI 上下文与窗口流转   | 窗口上下文、UI 生命周期、层级流转    | `docs/ui-windows-context-flow.md`                                          |
| `Assets/Arts/**`                                                                                  | 资源目录结构          | 资源路径、目录用途、约定边界         | `docs/arts-assets-usage.md`、`docs/project-structure.md`                   |
| `Assets/Resources/Config/*.asset` + `Configuration.Get*` 调用点                                   | 配置系统              | 配置键、默认值、生效范围、使用位置   | `docs/configuration-keys.md`、`README.md`、`CLAUDE.md`                     |
| `Packages/com.unity.ai.tools.mcp/README.md` + `Packages/com.unity.ai.tools.mcp/Documentation~/**` | Unity MCP             | 集成步骤、工具能力、排错说明         | `docs/unity-mcp-usage.md`                                                  |
| `docs/xasset-architecture.md`、`docs/pack-architecture.md`                                        | 专题权威文档          | 资源与打包机制                       | 其他文档引用时以专题文档为准                                               |

## 步骤 2：确定更新范围（按优先级）

- **P0（必须同步）**
    - `README.md`：快速开始、构建路径、关键入口、文档索引。
    - `CLAUDE.md`：架构总览、启动流程、关键文件路径。
    - `docs/startup-flow.md`：`App.cs -> Loading.cs -> StartGame.cs` 链路。
    - `docs/project-structure.md`：目录结构与导航入口。
    - `docs/configuration-keys.md`：配置键变化（新增/删除/重命名/默认值/调用位置）。
- **P1（核心专题）**
    - `docs/core-architecture.md`、`docs/core-*.md`：Core 模块职责与 API 描述一致性。
    - `docs/gamekit-packages.md`：依赖版本与用途。
    - `docs/lua-core-architecture.md`：Lua Core 与加载机制。
    - `docs/unity-mcp-usage.md`：与 MCP 包内文档保持一致。
    - `docs/xasset-architecture.md`、`docs/pack-architecture.md`：资源与打包机制一致性。
- **P2（故障/专项文档）**
    - `docs/ios-*.md`、`docs/gc-hotspots.md` 等专题：做陈旧性检查与必要修正。

## 步骤 3：更新规则

### 3.1 仅更新自动区块

仅修改 `<!-- AUTO-GENERATED:* -->` 标记内部内容；标记外内容默认视为手写内容。

### 3.2 配置键引用规范

当其他文档引用配置键时：

- 不复制整张键表。
- 统一链接到 `docs/configuration-keys.md`，并仅在上下文中说明行为影响与关键默认值。

### 3.3 Unity MCP 引用规范

- 优先引用 `Packages/com.unity.ai.tools.mcp/Documentation~/**` 与包内 `README.md`。
- 不引用过期路径（如不存在的 `.../docs/*`）。
- 文档中若涉及工具参数大小写，必须与当前工具 schema 一致。

## 步骤 4：一致性校验清单

针对以下高风险点做交叉校验：

- **启动链路**：`App.cs` / `Loading.cs` / `StartGame.cs` 与 `docs/startup-flow.md`、`CLAUDE.md`、`README.md` 一致。
- **Lua 入口**：`Assets.Lua/Application.lua`、`Assets.Lua/Hall/Main.lua`、`Assets.Lua/Games.lua` 的路径与描述一致。
- **结构文档**：`docs/project-structure.md` 中目录、入口、跳转关系与仓库一致。
- **构建菜单**：`Core > Build` 与 `xasset > Build` 路径与命名一致。
- **资源规范**：`Load/Release` 引用计数与异步加载建议与 `docs/xasset-architecture.md` 一致。
- **MCP 集成**：`docs/unity-mcp-usage.md` 与包内实际能力、路径和常见排障项一致。

## 步骤 5：陈旧性检查（Staleness）

1. 找出 `docs/` 下超过 90 天未更新的文档。
2. 若其事实来源近期有变更，标记为待复核。
3. 标记方式（文档顶部短提示）：

```markdown
> ⚠️ 该文档可能已过时：对应事实来源近期有更新，请复核相关描述与示例。
```

## 步骤 6：输出更新摘要（Show Summary）

输出必须可追踪，包含：

- **Updated**：实际修改了哪些文件、改了什么。
- **Flagged**：标记为可能过时的文件、原因和对应事实来源。
- **Skipped**：未改文件及理由（如“事实来源无变化”）。

示例：

```text
Docs Update
────────────────────────────────────────
Updated:  docs/startup-flow.md（StartGame 超时逻辑与代码对齐）
Updated:  docs/unity-mcp-usage.md（MCP 包文档路径修正为 Documentation~）
Flagged:  docs/core-memory.md（90+ 天未更新，且内存管理实现近期有变动）
Skipped:  docs/xasset-architecture.md（校验后无不一致）
────────────────────────────────────────
```

## 规则（Rules）

- **事实来源优先**：涉及版本、路径、入口链路、配置键的内容必须可追溯到源文件。
- **最小改动**：无明确不一致不改写手写章节。
- **禁止复制大资产全文**：不要把 `.asset` / 大型配置全文粘贴进文档。
- **禁止无需求新建文档**：除非用户明确要求，否则不新增 docs 文件。
