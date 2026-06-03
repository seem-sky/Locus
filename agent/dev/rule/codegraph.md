## CodeGraph Strategy

**NOTE (build mode):** CodeGraph is the **primary** structural search tool. **Prefer `codegraph_*` over `grep`** whenever the question is about code structure, symbols, calls, or cross-file relationships. `grep` is a **secondary fallback** for literal text only (string contents, log messages, comments, regex over content). For simple tasks `read`/`grep`/`list` may still satisfy `exploration_gate`, but always reach for CodeGraph first when the question is structural.

CodeGraph indexes the workspace as an AST-level knowledge graph (symbols, call edges, files). Use it for **structure** — definitions, callers, callees, blast radius — not for literal strings (use `grep`) or Unity assets (use `unity_*` tools).

**Prerequisites:** a selected working directory; `.codegraph/` present and reasonably fresh. READ-gate analysis tools (`codegraph_context`, `codegraph_impact`, etc.) are **direct-loaded** in build mode — **invoke them directly by name**; `tool_load` only returns schema and **does not execute** CodeGraph or satisfy `codegraph_gate`. Maintenance tools (`codegraph_status`, `codegraph_sync`) stay lazy-loaded.

### Simple vs complex

**Simple (CodeGraph preferred, gate optional):** 1–2 files, single directory, localized change (copy, config, comment, typo, obvious local fix), no exported/named API contract change. Even here, reach for `codegraph_search` / `codegraph_context` first; use `read`/`grep`/`list` only when CodeGraph is not applicable (literal text, file not in index).

**Complex (CodeGraph mandatory):** ≥3 source files or ≥2 top-level directories; refactor, new feature, architecture change; editing named functions/methods/classes/exported composables/stores; uncertain blast radius; structural or cross-file questions. Run CodeGraph before implement/review.

### Tool selection

| Goal                                 | Tool                | Notes                                                         |
| ------------------------------------ | ------------------- | ------------------------------------------------------------- |
| Find a symbol by name                | `codegraph_search`  | Pass `query`; optional `kind`, `limit`                        |
| Understand a module / task area      | `codegraph_context` | Pass `task` (natural language); includes snippets when useful |
| Who calls this symbol?               | `codegraph_callers` | Pass `target` (e.g. `Foo::bar`, `MyClass`)                    |
| What does this symbol call?          | `codegraph_callees` | Pass `target`                                                 |
| Trace path between two symbols       | `codegraph_trace`   | Pass `from` and `to`; optional `depth`, `limit`               |
| Blast radius before editing          | `codegraph_impact`  | Pass `target`; optional `depth` (default 2)                   |
| Indexed paths / languages            | `codegraph_files`   | Optional `filter`, `pattern`                                  |
| Index health / freshness             | `codegraph_status`  | Run when results look empty or stale                          |
| Refresh after large pull or refactor | `codegraph_sync`    | After sync, wait before re-querying                           |

`path` is optional on all tools — defaults to the current working directory.

### Standard workflows

**Simple localized edit**

1. `read` (or `grep` → `read`) the target file/section.
2. Dispatch `task(implementer)` after `exploration_gate` is satisfied.

**Explore unfamiliar code (complex)**

1. `codegraph_context` with a short task description.
2. `codegraph_search` only if you need a specific symbol name from the context.
3. `read` the 1–3 files/lines CodeGraph surfaced — do not bulk-read the whole tree.

**Edit a function, method, class, or exported API (complex)**

1. `codegraph_search` to confirm the symbol (file + line).
2. `codegraph_impact` on that symbol; summarize direct callers and risk to the user.
3. `read` the symbol and critical callers, then dispatch `task(implementer)`.

**Trace a call relationship (complex)**

1. Use `codegraph_trace` with `from` and `to` parameters.
2. If no path, fall back to `codegraph_callers` or `codegraph_callees`.
3. `read` bodies for the 2–4 symbols on the path.

### Rules

- **Default to CodeGraph first.** For any structural question (where is X, who calls Y, what does Z call, trace, blast radius), call `codegraph_*` before `grep`. `grep` is reserved for literal text — log messages, comments, string contents, regex over content.
- **Simple tasks:** `read`/`grep` may still satisfy `exploration_gate`, but prefer CodeGraph when the question is about code structure; do not require CodeGraph for purely literal searches.
- **Don't grep first** for symbol lookups — `codegraph_search` is faster and returns kind + location + signature in one call.
- **Don't re-verify CodeGraph with grep** — the AST parse is authoritative; running `grep` to confirm a CodeGraph answer wastes context.
- **Don't loop `codegraph_search` + `codegraph_node`** — use `codegraph_context` / `codegraph_explore` for breadth; do not spawn a subagent or run a grep→read loop when `codegraph_context` or `codegraph_impact` already answers the question.
- After you **edit** source in the same turn, do not immediately re-run CodeGraph — the index debounces (~500ms); use `read` for the file you just changed.
- If `codegraph_status` shows a missing or stale index, ask the user whether to run `codegraph init` / `codegraph sync`.

### Parallelism

Independent CodeGraph calls may run in the same turn. Serialize when a later call needs an earlier result (search → impact → read).
