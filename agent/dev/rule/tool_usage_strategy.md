## Tool Usage Strategy

* Use `unity_yaml_list`, `unity_yaml_search`, and `unity_yaml_read` to inspect Unity assets, and use the `read` tool to read ordinary files.

* Use `edit` to modify existing files, and `write` to create new files. For multiple independent modifications to the same file, use the `edits` array in a single call rather than making multiple consecutive calls.

* Use `list` to determine the file system structure within the working directory.

* **NOTE (build mode):** `edit`/`write` are hidden until READ completes (`exploration_gate`; plus `codegraph_gate` for **complex** edits). In READ, `bash` is available for **read-only** commands (e.g. `git diff`, `git status`, `git log`). **Prefer CodeGraph** for any structural question: `codegraph_search` / `codegraph_context` / `codegraph_impact` / `codegraph_trace` / `codegraph_callers` / `codegraph_callees`. Simple tasks: `codegraph_search` / `read` on target files; use `grep` only for literal text (logs, comments, string contents). Complex edits: CodeGraph first, then `read` on surfaced files. Apply code changes via `task(implementer)`, not direct `edit` on dev.

* Use `grep` only for **literal text** (logs, comments, string contents, regex over content) — never for symbol/call lookups; those go through CodeGraph. Use `unity_asset_search` to search for asset and code names, and `unity_ref_search` to search dependency relationships.
* **NOTE:** The `bash` tool auto-rewrites supported commands through [RTK](https://github.com/rtk-ai/rtk) for compact output — use normal `git`/`cargo`/`vitest` commands; you do not need to prefix `rtk` manually.

* Use `unity_execute` to execute code inside the Unity Editor, and use `bash` to run scripts on the system.

* For Unity debugging, use `unity_execute` / `unity_run_states` with `print` / `ctx.Print` to inspect internal state; request Unity Console copies only when tool-based inspection is insufficient.

* If a task requires understanding multiple files or project-level architecture, prefer project browsing tools or subagents over loading a large amount of raw file content all at once.

* **NOTE: If two or more tool calls are independent of each other and do not depend on one another’s results, they must be sent together in the same reply. Only when a later call depends on the result of an earlier one may they be serialized.**
