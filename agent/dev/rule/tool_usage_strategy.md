## Tool Usage Strategy

* Use `unity_yaml_list`, `unity_yaml_search`, and `unity_yaml_read` to inspect Unity assets, and use the `read` tool to read ordinary files.

* Use `edit` to modify existing files, and `write` to create new files. For multiple independent modifications to the same file, use the `edits` array in a single call rather than making multiple consecutive calls.

* Use `list` to determine the file system structure within the working directory.

* Use `grep` for literal text in files. For symbols, callers, and change impact, follow **CodeGraph Strategy** (`codegraph.md`): load lazy tools via `tool_load`, then use `codegraph_*` builtins.
* Use `unity_asset_search` to search for asset and code names, and use `unity_ref_search` to search dependency relationships.

* Use `unity_execute` to execute code inside the Unity Editor, and use `bash` to run scripts on the system.

* For **Lua / xLua GC** issues in Play Mode: have the user start recording from the **Lua GC** monitor panel (or ensure samples exist), then call **`lua_gc_analyze`** for rule-based alerts. Follow with **`knowledge/skill/gc.md`** for static tuning patterns. Use `unity_run_states` profiler flows for **C#** GC, not Lua VM metrics.

* For **Lua/xLua GC or memory pressure during Play Mode**, prefer the Lua GC monitor workflow before guessing from static code:
  1. Ensure Play Mode and `Locus.LuaGcBootstrap.Register(() => luaEnv)` when needed.
  2. Start sampling via the built-in **Lua GC Monitor** panel or `lua_gc_monitor_start` IPC (user-driven).
  3. After enough samples, call `lua_gc_analyze` for alerts and tuning suggestions.
  4. Cross-check hotspots with the `gc` knowledge skill and `unity_run_states` only for C# Profiler markers when needed.

* If a task requires understanding multiple files or project-level architecture, prefer project browsing tools or subagents over loading a large amount of raw file content all at once.

* **NOTE: If two or more tool calls are independent of each other and do not depend on one another’s results, they must be sent together in the same reply. Only when a later call depends on the result of an earlier one may they be serialized.**
