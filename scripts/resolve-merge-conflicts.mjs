import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CODE_TOOLS = [
  "code_find_references",
  "code_goto_definition",
  "code_symbol_search",
  "code_diagnostics",
  "code_hover",
  "unity_code_usages",
];

const CONFLICT_RE = /<<<<<<< HEAD\r?\n([\s\S]*?)\r?\n=======\r?\n[\s\S]*?\r?\n>>>>>>> upstream\/main\r?\n?/;

function gitJson(ref, filePath) {
  return JSON.parse(execSync(`git show ${ref}:${filePath}`, { cwd: repoRoot, encoding: "utf8" }));
}

function mergeToolLists(headTools, upstreamTools) {
  const out = [...headTools];
  for (const tool of [...upstreamTools, ...CODE_TOOLS]) {
    if (!out.includes(tool)) out.push(tool);
  }
  return out;
}

function resolveAgentConfig(relPath) {
  const head = gitJson("HEAD", relPath);
  const upstream = gitJson("upstream/main", relPath);
  const merged = { ...upstream, ...head };
  merged.tools = mergeToolLists(head.tools ?? [], upstream.tools ?? []);
  if (head.sub_agents?.length) merged.sub_agents = head.sub_agents;
  writeFileSync(path.join(repoRoot, relPath), `${JSON.stringify(merged, null, 2)}\n`);
}

function pickHead(relPath) {
  const filePath = path.join(repoRoot, relPath);
  const text = readFileSync(filePath, "utf8");
  const normalized = text.replace(/\r\n/g, "\n");
  const resolved = normalized.replace(CONFLICT_RE, "$1");
  if (resolved.includes("<<<<<<<")) {
    throw new Error(`unresolved conflict in ${relPath}`);
  }
  writeFileSync(filePath, resolved);
}

function resolveReadJson() {
  const filePath = path.join(repoRoot, "tools/read.json");
  const obj = JSON.parse(
    readFileSync(filePath, "utf8").replace(/\r\n/g, "\n").replace(CONFLICT_RE, "$1"),
  );
  obj.description =
    "Read a workspace file.\n" +
    "- Text files return file content (up to 2000 lines by default), each line prefixed with its 1-based line number in cat -n format (number, tab, content). Line numbers are display metadata: never include the number prefix or the tab in edit oldString/newString, and use them as the line argument for code_* tools\n" +
    "- Use offset and limit to page through large text files; line numbers stay absolute\n" +
    "- When Headroom is enabled and output exceeds the compress threshold, large read results are compressed through headroom-ai (requires headroom proxy or HEADROOM_API_KEY)\n" +
    "- PNG, JPEG, GIF, and WebP images return an image attachment; offset and limit are ignored for images\n" +
    "- This tool only reads files; use list for directories\n" +
    "- Do not use this tool for documents under Locus/knowledge, including Memory. Use knowledge_list, knowledge_query, or knowledge_read instead";
  writeFileSync(filePath, `${JSON.stringify(obj, null, 2)}\n`);
}

function resolveGrepJson() {
  const filePath = path.join(repoRoot, "tools/grep.json");
  const obj = JSON.parse(
    readFileSync(filePath, "utf8").replace(/\r\n/g, "\n").replace(CONFLICT_RE, "$1"),
  );
  obj.description =
    "- Fast content search tool for Unity project codebases of any size\n" +
    "- When Headroom RTK is enabled, runs `rtk grep` (compact ripgrep/grep output); otherwise uses native Rust search with Unity-aware skips (Library/, Temp/, etc.)\n" +
    "- Searches file contents using regular expressions\n" +
    "- Supports full regex syntax (eg. \"MonoBehaviour\", \"ScriptableObject\", \"SceneManager\\\\.sceneLoaded\")\n" +
    "- Filter files by pattern with the include parameter (eg. \"*.cs\", \"Assets/**/Editor/*.cs\")\n" +
    "- By default, native fallback skips root-level generated directories such as Library/, Temp/, Obj/, Logs/, and Build*/\n" +
    "- Returns file paths and line numbers with at least one match sorted by path and line number\n" +
    "- You MUST set path explicitly; never rely on the current process directory\n" +
    "- Matches raw text only: results include comments, strings, and docs; identifier hits are not semantic symbol references\n" +
    "- For shell-style search with full RTK rewrite rules, use `bash` with `rg`/`grep` instead of this tool";
  writeFileSync(filePath, `${JSON.stringify(obj, null, 2)}\n`);
}

function resolveUnityYamlSearch() {
  pickHead("tools/unity_yaml_search.json");
  const filePath = path.join(repoRoot, "tools/unity_yaml_search.json");
  const obj = JSON.parse(readFileSync(filePath, "utf8"));
  if (!obj.description.includes("Field matching")) {
    obj.description +=
      "\n\nWhen both query and component_filter are given, a result must satisfy both. Field matching (field_name/field_value in match_fields) is expensive on large scenes — combine it with path_prefix when possible.";
    writeFileSync(filePath, `${JSON.stringify(obj, null, 2)}\n`);
  }
}

function replaceConflicts(text, replacement) {
  const normalized = text.replace(/\r\n/g, "\n");
  const out = normalized.replace(CONFLICT_RE, () => replacement);
  if (out.includes("<<<<<<<")) {
    throw new Error("unresolved conflict markers remain");
  }
  return out;
}

function resolveToolUsageStrategy() {
  const filePath = path.join(repoRoot, "agent/dev/rule/tool_usage_strategy.md");
  const text = readFileSync(filePath, "utf8");
  const merged =
    `* **NOTE (build mode):** \`edit\`/\`write\` are hidden until READ completes (\`exploration_gate\`; plus \`codegraph_gate\` for **complex** edits). In READ, \`bash\` is available for **read-only** commands (e.g. \`git diff\`, \`git status\`, \`git log\`). **Prefer CodeGraph** for any structural question: \`codegraph_search\` / \`codegraph_context\` / \`codegraph_impact\` / \`codegraph_trace\` / \`codegraph_callers\` / \`codegraph_callees\`. Simple tasks: \`codegraph_search\` / \`read\` on target files; use \`grep\` only for literal text (logs, comments, string contents). Complex edits: CodeGraph first, then \`read\` on surfaced files. Apply code changes via \`task(implementer)\`, not direct \`edit\` on dev.\n\n` +
    `* Use \`code_symbol_search\` / \`code_find_references\` for C# declarations and references when CodeGraph is unavailable or for language-server-style lookups. Use \`grep\` only for **literal text** (logs, comments, string contents, regex over content) — never for symbol/call lookups when CodeGraph applies. Use \`unity_asset_search\` to search for asset and code names, and \`unity_ref_search\` to search dependency relationships.\n` +
    `* **NOTE:** The \`bash\` tool auto-rewrites supported commands through [Headroom](https://github.com/chopratejas/headroom) RTK — use normal \`git\`/\`cargo\`/\`vitest\` commands; RTK compresses their output at execution. Large output from non-RTK commands may use \`headroom-ai\` fallback (proxy or \`HEADROOM_API_KEY\`).\n`;
  writeFileSync(filePath, replaceConflicts(text, merged));
}

function resolveExplorerSystem() {
  const filePath = path.join(repoRoot, "agent/explorer/system.md");
  const text = readFileSync(filePath, "utf8");
  const merged =
    "- **Prefer CodeGraph** (`codegraph_search` / `codegraph_context` / `codegraph_impact` / `codegraph_trace` / `codegraph_callers` / `codegraph_callees`) for structural questions. Use `grep` only for **literal text** — regex over file contents (string literals, log messages, comments). Do not use `grep` to look up symbols, callers, or call relationships.\n" +
    "- For C# type/method names when CodeGraph is insufficient, use `code_symbol_search` (fuzzy, semantic) and `code_find_references` for exact reference lists\n" +
    "- Use read when you know the specific file path you need to read — **read relevant sections in full**, not one-line snippets\n";
  writeFileSync(filePath, replaceConflicts(text, merged));
}

for (const rel of [
  "agent/dev/config.json",
  "agent/explorer/config.json",
  "agent/doc/config.json",
  "agent/wiki/config.json",
  "agent/runtime_debugger/config.json",
]) {
  resolveAgentConfig(rel);
  console.log(`${rel} ok`);
}

for (const rel of [
  "tools/bash.json",
  "tools/ask.json",
  "tools/unity_ref_search.json",
  "tools/unity_execute.json",
  "tools/unity_yaml_read.json",
  "tools/unity_asset_search.json",
]) {
  pickHead(rel);
  console.log(`${rel} ok`);
}

resolveReadJson();
console.log("tools/read.json ok");
resolveGrepJson();
console.log("tools/grep.json ok");
resolveUnityYamlSearch();
console.log("tools/unity_yaml_search.json ok");
resolveToolUsageStrategy();
console.log("tool_usage_strategy.md ok");
resolveExplorerSystem();
console.log("explorer/system.md ok");

execSync("bun run view-runtime:export", { cwd: repoRoot, stdio: "inherit" });
console.log("manifest.json regenerated");
