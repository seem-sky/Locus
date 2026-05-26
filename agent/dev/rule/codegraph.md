## CodeGraph Strategy

CodeGraph indexes the workspace as an AST-level knowledge graph (symbols, call edges, files). Use it for **structure** — definitions, callers, callees, blast radius — not for literal strings (use `grep`) or Unity assets (use `unity_`* tools).

**Prerequisites:** a selected working directory; `.codegraph/` present and reasonably fresh. CodeGraph tools are **lazy-loaded** — call `tool_load` with the tool name(s) you need before first use in a session.

### Tool selection


| Goal                                 | Tool                | Notes                                                         |
| ------------------------------------ | ------------------- | ------------------------------------------------------------- |
| Find a symbol by name                | `codegraph_search`  | Pass `query`; optional `kind`, `limit`                        |
| Understand a module / task area      | `codegraph_context` | Pass `task` (natural language); includes snippets when useful |
| Who calls this symbol?               | `codegraph_callers` | Pass `target` (e.g. `Foo::bar`, `MyClass`)                    |
| What does this symbol call?          | `codegraph_callees` | Pass `target`                                                 |
| Blast radius before editing          | `codegraph_impact`  | Pass `target`; optional `depth` (default 2)                   |
| Indexed paths / languages            | `codegraph_files`   | Optional `filter`, `pattern`                                  |
| Index health / freshness             | `codegraph_status`  | Run when results look empty or stale                          |
| Refresh after large pull or refactor | `codegraph_sync`    | After sync, wait before re-querying                           |


`path` is optional on all tools — defaults to the current working directory.

### Standard workflows

**Explore unfamiliar code**

1. `codegraph_context` with a short task description.
2. `codegraph_search` only if you need a specific symbol name from the context.
3. `read` the 1–3 files/lines CodeGraph surfaced — do not bulk-read the whole tree.

**Edit a function, method, class, or exported API**

1. `codegraph_search` to confirm the symbol (file + line).
2. `codegraph_impact` on that symbol; summarize direct callers and risk to the user.
3. `read` the symbol and critical callers, then `edit` / `write`.

**Trace a call relationship (no dedicated trace tool)**

1. `codegraph_callers` or `codegraph_callees` from the known symbol.
2. Repeat on the next hop only if needed — avoid rebuilding long chains with many serial tool calls.
3. `read` bodies for the 2–4 symbols on the path.

### Rules

- Prefer CodeGraph over `grep` for **symbol names**; prefer `grep` for string literals, comments, log messages, and config text.
- Trust CodeGraph locations (file + line). Do not re-verify symbol placement with `grep`.
- Do not spawn a subagent or run a grep→read loop when `codegraph_context` or `codegraph_impact` already answers the question.
- After you **edit** source in the same turn, do not immediately re-run CodeGraph — the index debounces (~500ms); use `read` for the file you just changed.
- If `codegraph_status` shows a missing or stale index, ask the user whether to run `codegraph init` / `codegraph sync` in the project root (or use `codegraph_sync` when the tool is loaded).

### Parallelism

Independent CodeGraph calls (e.g. `codegraph_status` + `codegraph_search` for unrelated symbols) may run in the same turn. Serialize when a later call needs an earlier result (search → impact → read).