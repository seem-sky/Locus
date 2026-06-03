## Multi-Stage Code Editing Workflow (Mandatory)

**NOTE: For substantive code changes in build mode, you MUST follow Read → Plan → Implement → Optimize → Review. The runtime enforces this by default — direct `edit` / `write` on dev will be blocked outside the correct phase. Set `LOCUS_DEV_WORKFLOW_STRICT=0` to disable blocking.**

**At every stage** (Read, Plan, Implement, Optimize, Review, and each review retry), follow `source_code_discipline`: read relevant source in full, reason about normal/edge/error/runtime paths, and make only minimal cautious edits.

### Stage 1: Read (Analysis)

Before implementation or review, explore with read-only tools:

**Simple task** (1–2 files, localized, no API contract change):

1. **`codegraph_search` / `codegraph_context`** first for any structural question; `read`, `grep`, or `list` only for literal text or files not in the index
2. Unity tools when asset-scoped (`unity_yaml_*`, `unity_asset_search`, …)

**Complex code edit** (multi-file, refactor, named symbols, cross-module — see `codegraph` rule):

1. **`codegraph_context`** (preferred), or `codegraph_impact` / `codegraph_trace` / `codegraph_callers` / `codegraph_callees` / `codegraph_search` on the change scope
2. `read` on files/lines CodeGraph surfaced — `grep` only for literal text, never for symbol lookups
3. Optional breadth: `task` with `subagent_type: "explorer"` (does not replace CodeGraph on complex scope)

**Always available (not gated like `edit`/`write`):** `unity_execute`, `unity_run_states`, `unity_recompile`, `unity_capture_viewport`.

**Ambiguous tools in READ/PLAN:** If the runtime cannot classify a call as read-only exploration vs code edit (e.g. unknown `bash` command, `web_fetch`, lazy-loaded tools), the UI prompts the user to allow or deny before execution — even when global permission mode is `auto`.

**Do not** call `edit`/`write` before READ completes — they are runtime-blocked until `read_gate=true`.

Requirements before the next stage (`read_gate=true`):

- **Exploration (`exploration_gate`):** at least one `read` / `grep` / `list`, **`codegraph_context`** (also counts as exploration), or completed `task(explorer)`. **Prefer CodeGraph** for structural questions; `grep` is for literal text only.
- **CodeGraph (`codegraph_gate`):** required only for **complex** edits — at least one relationship-analysis tool (`codegraph_status` / `codegraph_sync` alone do **not** count). Simple tasks do not need `codegraph_gate`.
- You can explain **current behavior** and **risks** for the change scope.

### Stage 2a: Read-only review (no code changes)

When the user asks to **review existing code** without edits (e.g. GC audit, security review, style check):

Dispatch `task(reviewer)` directly from READ or PLAN (before plan confirmation). **Do not** require parent dev `exploration_gate` — the reviewer subagent explores with `read` / `grep` / `list` (and CodeGraph when needed). **Do not** require PLAN for read-only review:

```json
{
  "description": "Review code for issues",
  "prompt": "Review this code:\n\n[file contents and analysis]\n\nReturn PASS, PASS_WITH_RISKS, or BLOCK with actionable fixes.",
  "subagent_type": "reviewer"
}
```

### Stage 2b: Plan (write modification plan + user confirmation)

When code **must change**, after Read write a modification plan and **pause for user confirmation** before implementation.

The plan **must** include:

1. **修改文件清单** — every file path to touch
2. **每个文件的具体变更描述**（须逐项详细，不可笼统概括）— for **each** file include:
   - 变更类型：新增 / 修改 / 删除
   - 目标位置：函数/方法/类/字段名，或具体行号范围
   - 当前行为：该处代码现在做什么（正常路径 + 相关边界情况）
   - 计划行为：改完后应做什么
   - 变更要点：关键代码片段或伪 diff（改前 → 改后）
   - 该文件相关的运行时/边界说明（如 null、生命周期、异步、Unity 场景重载等）
3. **影响范围评估** — callers/callees、爆炸半径、跨模块风险
4. **回滚策略** — how to revert (e.g. `git checkout -- <files>`, feature flag, etc.)

Present the plan to the user, then call `ask_user_question` with these options (last option = custom input for 修改):

**Do NOT ask for confirmation in prose only** (e.g. "请确认是否执行") — the UI shows plan options **only** when `ask_user_question` runs. Ending the turn with text alone leaves the user with nothing to confirm.

```json
{
  "question": "请确认以下修改计划，或选择取消/修改。",
  "options": [
    { "label": "确认执行", "description": "按计划进入实现阶段" },
    { "label": "取消", "description": "取消本次修改计划，回到 Read 阶段重新分析" },
    { "label": "修改", "description": "输入修改意见（在下方输入框填写）" }
  ]
}
```

- **确认执行** — runtime sets `plan_confirmed=true`; dispatch `task(implementer)` next. If a complex edit skipped CodeGraph during READ, run `codegraph_context` / `codegraph_impact` in PLAN phase first to satisfy `codegraph_gate`, then dispatch implementer.
- **取消** — phase resets to READ; re-explore before a new plan
- **修改** — stay in PLAN; revise the plan from user feedback (with the same per-file detail) and call `ask_user_question` again

**Do not** call `task(implementer)` until the user confirms the plan.

### Stage 2c: Implement (Subagent Only)

After the user confirms the plan, dispatch implementation — **do not** call `edit` / `write` yourself:

```json
{
  "description": "Implement code changes",
  "prompt": "Implement based on this analysis and requirement:\n\n[analysis — include file paths, current behavior, callers/callees, edge cases considered]\n\n[requirement]\n\nRe-read every file before editing. Minimal diff only; simulate normal/edge/error paths. List files changed when done.",
  "subagent_type": "implementer"
}
```

### Stage 3: Optimize (after implementer)

After implementer completes, dispatch optimization — **do not** skip:

```json
{
  "description": "Optimize implementation",
  "prompt": "Optimize the implementer changes below for efficiency, concision, and runtime depth:\n\n[implementer summary + file list]\n\nRe-read every changed file. Trace normal/edge/error/runtime paths (Unity lifecycle, async, allocations). Apply minimal surgical improvements only. List files changed and runtime notes for the reviewer.",
  "subagent_type": "optimizer"
}
```

### Stage 4: Review (after optimizer)

After optimizer completes, dispatch review — **do not** skip:

```json
{
  "description": "Review code changes",
  "prompt": "Review these changes:\n\n[optimizer summary + files]\n\nRe-read changed files and related call sites. Check quality, security, performance, logic, and runtime edge cases (null/empty, lifecycle, async). Return PASS, PASS_WITH_RISKS, or BLOCK with actionable fixes.",
  "subagent_type": "reviewer"
}
```

### Review loop (until pass)

The runtime parses the reviewer output. Only **PASS** or **PASS_WITH_RISKS** ends the workflow.

If review is **BLOCK**, unclear, or missing a pass verdict:

1. Phase resets to **Read** (full cycle required)
2. Re-analyze with read tools or `task(explorer)`
3. **Plan** — write a new modification plan + `ask_user_question` confirmation
4. `task(implementer)` with fixes from review feedback (if code must change)
5. `task(optimizer)` to refine the fix
6. `task(reviewer)` again

Repeat until the reviewer returns PASS or PASS_WITH_RISKS.

### Subagent Quick Reference

| Need | `subagent_type` |
|------|-----------------|
| Explore / find code | `explorer` |
| Write code | `implementer` |
| Refine for efficiency / runtime depth | `optimizer` |
| Review existing or changed code | `reviewer` |

### Exemptions (no three-stage gate)

- Plan mode (read-only by design)
- **Knowledge and Skill tools** (`knowledge_*`, `skill_*`) — always available; not blocked, hidden, or counted toward READ exploration
- Knowledge-only edits under `Locus/knowledge/` markdown via `edit`/`write` (not application source)
- User explicitly requests a trivial one-line fix and you have already `read` the target file
