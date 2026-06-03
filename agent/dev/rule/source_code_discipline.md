## Source Code Discipline (All Workflow Phases)

**NOTE: This applies at every stage — Read, Implement, Optimize, Review, complex-task phases (plan / execute / debug / verify), and every review retry. Dev, implementer, optimizer, reviewer, and explorer subagents must all follow it.**

### 1. Read and analyze carefully

- Read **relevant source in full** before drawing conclusions — do not skim, guess from filenames, or rely on stale memory.
- **Prefer CodeGraph** for any structural question (where is X, who calls Y, blast radius, trace): `codegraph_search` / `codegraph_context` / `codegraph_impact` / `codegraph_trace` / `codegraph_callers` / `codegraph_callees`. Use `read`/`grep`/`list` only for literal text (logs, comments, string contents) and files not in the index.
- **Build mode:** simple tasks satisfy READ with exploration alone; **complex** edits require `codegraph_gate` before implement/review. `task(explorer)` does not replace CodeGraph on complex scope.
- For structural or cross-file questions, consult CodeGraph **before** editing (complex tasks only).
- When dispatching subagents, pass **concrete file paths and analysis findings** in the prompt — not vague instructions.

### 2. Think deeply about runtime behavior

Before implementing or approving a change, explicitly consider:

- **Happy path** — normal inputs and expected outcomes
- **Edge cases** — null/empty, boundaries, first/last run, missing config
- **Error paths** — failures, retries, partial state, rollback
- **Concurrency & lifecycle** — async, re-entrancy, Unity `Awake` / `OnEnable` / `OnDisable` / `OnDestroy`, editor vs play mode
- **Blast radius** — who else calls this API; what breaks if assumptions change (`codegraph_impact` when editing named symbols)

Do not proceed to edits until you can explain *how the code runs today* and *what your change will do in each scenario above*.

### 3. Modify cautiously

- Prefer the **smallest correct change**; do not refactor unrelated code or add unrequested features.
- Re-read target files immediately before `edit` / `write`; re-read after edits when practical.
- One logical change at a time; avoid mechanical retries of the same failed approach.
- If uncertain about impact, explore more or ask the user — do not speculate with broad edits.

### Per-phase checklist

| Phase | Must do |
|-------|---------|
| **Read** | Full read + structural context; document current behavior and risks before implement/review |
| **Implement** (subagent) | Re-read every file to touch; simulate edge cases; minimal diff only |
| **Optimize** (subagent) | Re-read implementer output; trace runtime depth; tighten efficiency with minimal diffs |
| **Review** (subagent) | Re-read changed files and related callers; verify logic and edge cases, not style-only |
| **Complex task** | Each plan batch still obeys Read → Implement → Optimize → Review with this discipline |
| **Review retry** | Re-analyze from scratch — do not patch blindly from reviewer bullets alone |

Superficial analysis, grep-only understanding, or speculative edits are **not acceptable** at any stage. When the question is structural, use CodeGraph — `grep` alone is never enough.
