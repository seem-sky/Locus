## Complex Task Workflow (Heuristic — Rules + Skills)

When a task is **large or multi-step**, follow this pipeline **before** claiming done. Each implementation batch inside the plan still obeys **Read → Plan → Implement (subagent) → Optimize (subagent) → Review (subagent)**.

**Every phase below** also requires `source_code_discipline`: careful full read of relevant source, deep reasoning about runtime scenarios (normal / edge / error / lifecycle), and cautious minimal edits only.

### When to treat a task as complex (self-check)

Use the full pipeline when **any** applies:

- Expected to touch **≥3** source files, or **≥2** top-level directories
- User asks for refactor, new feature, architecture change, or multi-step delivery
- No approved written plan exists and your `todowrite` list has **≥5** concrete steps
- You are unsure about blast radius — plan first

### Five phases (in order)

1. **writing-plans** — Announce: "我正在使用 writing-plans 技能创建实现计划."  
   Produce `docs/superpowers/plans/YYYY-MM-DD-<feature>.md` (or project-agreed path). Get user approval before heavy coding.

2. **executing-plans** — Announce: "我正在使用 executing-plans 技能来实现此计划."  
   Execute in small batches; after each batch, run the three-stage code workflow.

3. **systematic-debugging** — When bugs, test failures, or unexpected behavior appear, announce: "我正在使用 systematic-debugging 技能."  
   Root cause before speculative fixes. Use `/debug` skill or follow its checklist.

4. **code-review** — Before merge-ready state: use `/code-reviewer` skill or `task(reviewer)` on the full change set.

5. **verification-before-completion** — Announce: "我正在使用 verification-before-completion 技能."  
   Run relevant tests/build/lint; cite command output. Never claim "done" or "fixed" without evidence.

### Skill triggers in Locus

| Phase | Slash command / Skill |
|-------|------------------------|
| Plan | `/writing-plans` |
| Execute plan | `/executing-plans` |
| Debug | `/debug` |
| Review | `/code-reviewer` |
| Verify | `/verify` |

Select the skill in chat when starting that phase so injected content is available.

### Notes

- Complex workflow is **not** runtime-blocked like the three-stage edit gate; you must self-enforce via this rule.
- Prefer `explorer` subagent for wide codebase search instead of loading entire trees into context.
- Break work into verifiable steps with `todowrite` and mark steps complete as you finish them.
