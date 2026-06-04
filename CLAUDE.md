<!-- superpowers-zh:begin (do not edit between these markers) -->

# Superpowers-ZH 中文增强版

本项目已安装 superpowers-zh 技能框架（20 个 skills）。

## 核心规则

1. **收到任务时，先检查是否有匹配的 skill** — 哪怕只有 1% 的可能性也要检查
2. **设计先于编码** — 收到功能需求时，先用 brainstorming skill 做需求分析
3. **测试先于实现** — 写代码前先写测试（TDD）
4. **验证先于完成** — 声称完成前必须运行验证命令

## 可用 Skills

Skills 位于 `.claude/skills/` 目录，每个 skill 有独立的 `SKILL.md` 文件。

- **brainstorming**: 在任何创造性工作之前必须使用此技能——创建功能、构建组件、添加功能或修改行为。在实现之前先探索用户意图、需求和设计。
- **chinese-code-review**: 中文 review 沟通参考——话术模板、分级标注（必须修复/建议修改/仅供参考）、国内团队常见反模式应对。仅在用户显式 /chinese-code-review 时调用，不要根据上下文自动触发。
- **chinese-commit-conventions**: 中文 commit 与 changelog 配置参考——Conventional Commits 中文适配、commitlint/husky/commitizen 中文模板、conventional-changelog 中文配置。仅在用户显式 /chinese-commit-conventions 时调用，不要根据上下文自动触发。
- **chinese-documentation**: 中文文档排版参考——中英文空格、全半角标点、术语保留、链接格式、中文文案排版指北约定。仅在用户显式 /chinese-documentation 时调用，不要根据上下文自动触发。
- **chinese-git-workflow**: 国内 Git 平台配置参考——Gitee、Coding.net、极狐 GitLab、CNB 的 SSH/HTTPS/凭据/CI 接入差异与镜像同步配置。仅在用户显式 /chinese-git-workflow 时调用，不要根据上下文自动触发。
- **dispatching-parallel-agents**: 当面对 2 个以上可以独立进行、无共享状态或顺序依赖的任务时使用
- **executing-plans**: 当你有一份书面实现计划需要在单独的会话中执行，并设有审查检查点时使用
- **finishing-a-development-branch**: 当实现完成、所有测试通过、需要决定如何集成工作时使用——通过提供合并、PR 或清理等结构化选项来引导开发工作的收尾
- **mcp-builder**: MCP 服务器构建方法论 — 系统化构建生产级 MCP 工具，让 AI 助手连接外部能力
- **receiving-code-review**: 收到代码审查反馈后、实施建议之前使用，尤其当反馈不明确或技术上有疑问时——需要技术严谨性和验证，而非敷衍附和或盲目执行
- **requesting-code-review**: 完成任务、实现重要功能或合并前使用，用于验证工作成果是否符合要求
- **subagent-driven-development**: 当在当前会话中执行包含独立任务的实现计划时使用
- **systematic-debugging**: 遇到任何 bug、测试失败或异常行为时使用，在提出修复方案之前执行
- **test-driven-development**: 在实现任何功能或修复 bug 时使用，在编写实现代码之前
- **using-git-worktrees**: 当需要开始与当前工作区隔离的功能开发或执行实现计划之前使用——创建具有智能目录选择和安全验证的隔离 git 工作树
- **using-superpowers**: 在开始任何对话时使用——确立如何查找和使用技能，要求在任何响应（包括澄清性问题）之前调用 Skill 工具
- **verification-before-completion**: 在宣称工作完成、已修复或测试通过之前使用，在提交或创建 PR 之前——必须运行验证命令并确认输出后才能声称成功；始终用证据支撑断言
- **workflow-runner**: 在 Claude Code / OpenClaw / Cursor 中直接运行 agency-orchestrator YAML 工作流——无需 API key，使用当前会话的 LLM 作为执行引擎。当用户提供 .yaml 工作流文件或要求多角色协作完成任务时触发。
- **writing-plans**: 当你有规格说明或需求用于多步骤任务时使用，在动手写代码之前
- **writing-skills**: 当创建新技能、编辑现有技能或在部署前验证技能是否有效时使用

## 如何使用

当任务匹配某个 skill 时，使用 `Skill` 工具加载对应 skill 并严格遵循其流程。绝不要用 Read 工具读取 SKILL.md 文件。

如果你认为哪怕只有 1% 的可能性某个 skill 适用于你正在做的事情，你必须调用该 skill 检查。

<!-- superpowers-zh:end -->

<!-- CODEGRAPH_START -->
## CodeGraph 优先，grep 辅助

Cursor 规则详见 `.cursor/rules/codegraph.mdc`（`alwaysApply: true`）。**默认优先使用 CodeGraph**，`grep` 仅用于字面量搜索。

1. 任何结构性问题（找符号、调用关系、影响范围、追踪 A→B、谁调用谁）：一律 `codegraph_search` / `codegraph_context` / `codegraph_impact` / `codegraph_trace` / `codegraph_callers` / `codegraph_callees`。
2. 改具名函数/方法/类/对外 API 前：先 `codegraph_impact`（upstream），向用户说明影响面后再改。
3. **不要用 `grep` 做结构查询**——`grep` 只用于字面文本（日志、注释、字符串常量、正则匹配文件内容）。
4. 简单任务也优先 CodeGraph；不强制但 `codegraph_search` 比 `grep` 更准更快。

本地索引：`codegraph init -i`（首次克隆后）；大改或拉代码后：`codegraph sync`。MCP 配置：`.cursor/mcp.json`。
<!-- CODEGRAPH_END -->

<!-- headroom-instructions v4 -->
# Headroom - Context Compression for Locus Agents

Locus integrates [Headroom](https://github.com/chopratejas/headroom) in three layers:

1. **RTK (bash, primary)** — `git`, `cargo test`, `grep`, `vitest`, etc. are rewritten via `rtk rewrite` and executed as `rtk …`. RTK compresses CLI output inline — **no proxy required**.
2. **headroom-ai bash fallback** — non-RTK bash commands with large output use single-message `compress()` after execution.
3. **headroom-ai Library (LLM context)** — when context nears the auto-compact threshold, Locus compresses the full prepared message history via `compress(messages)` **before** auto-compact. Transient for that LLM request (not written to DB).

## Setup

**RTK:**

```bash
# ~/.headroom/bin/rtk or PATH; optional: LOCUS_HEADROOM_RTK_PATH
```

**headroom-ai (bash fallback + Library context compress):**

```bash
headroom proxy          # default http://localhost:8787 (Locus autostarts when Headroom enabled + local base URL)
# or: export HEADROOM_API_KEY=hr_... with https://api.headroom.ai (no local proxy)
# LOCUS_HEADROOM_PROXY_AUTOSTART=0 to disable proxy autostart
# LOCUS_HEADROOM_CLI=/path/to/headroom if not on PATH
bun install             # dev: headroom-ai npm dependency
bun run headroom:bundle # packaged builds
```

Env: `LOCUS_HEADROOM_RTK_PATH`, `HEADROOM_BASE_URL`, `HEADROOM_API_KEY`, `LOCUS_HEADROOM_SCRIPT`, `LOCUS_HEADROOM_DISABLED=1`, `LOCUS_HEADROOM_CONTEXT_COMPRESS=0` (disable Library only), `LOCUS_BUN_EXE`.

## Golden Rule for agents

**Write normal shell commands** — do not prefix `rtk` yourself. Locus rewrites and compresses automatically.

```bash
# ✅ Correct — Locus handles Headroom/RTK
git status
cargo test
vitest run

# ❌ Unnecessary manual prefix
rtk git status
```

## Typical savings

| Stage | What | Typical savings |
|-------|------|-----------------|
| RTK rewrite | git, cargo test, grep, vitest | 60–90% |
| bash headroom-ai fallback | large logs, custom scripts | 60–95% |
| Library context compress | full agent history before LLM | 60–95% |

Proxy down? RTK still works; bash fallback and Library compress degrade gracefully; auto-compact remains the last resort.
<!-- /headroom-instructions -->