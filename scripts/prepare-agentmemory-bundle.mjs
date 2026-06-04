import { createHash } from "node:crypto";
import {
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import https from "node:https";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const III_REPO = "iii-hq/iii";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleDir = path.join(repoRoot, "src-tauri", "gen", "agentmemory-bundle");
const codegraphBundleDir = path.join(repoRoot, "src-tauri", "gen", "codegraph-bundle");
const cacheDir = path.join(repoRoot, ".cache", "agentmemory-bundle");

function platformTarget() {
  return `${process.platform}-${process.arch}`;
}

function iiiAssetName() {
  const arch = process.arch === "x64" ? "x86_64" : process.arch === "arm64" ? "aarch64" : process.arch;
  if (process.platform === "win32") {
    return `iii-${arch}-pc-windows-msvc.zip`;
  }
  if (process.platform === "darwin") {
    return `iii-${arch}-apple-darwin.tar.gz`;
  }
  return `iii-${arch}-unknown-linux-gnu.tar.gz`;
}

function iiiBinaryName() {
  return process.platform === "win32" ? "iii.exe" : "iii";
}

function readPinnedVersion() {
  return readFileSync(path.join(bundleDir, "version.txt"), "utf8").trim();
}

function readIiiVersion() {
  return readFileSync(path.join(bundleDir, "iii-version.txt"), "utf8").trim();
}

function cliEntryPath() {
  return path.join(bundleDir, "node_modules", "@agentmemory", "agentmemory", "dist", "cli.mjs");
}

function agentmemoryIndexPath() {
  return path.join(bundleDir, "node_modules", "@agentmemory", "agentmemory", "dist", "index.mjs");
}

function agentmemoryIndexCandidates() {
  const candidates = [agentmemoryIndexPath()];
  const extraRoots = [
    process.env.LOCUS_AGENTMEMORY_PATCH_ROOT?.trim(),
    path.join(process.env.APPDATA || "", "npm", "node_modules", "@agentmemory", "agentmemory"),
    path.join(process.env.LOCALAPPDATA || "", "npm", "node_modules", "@agentmemory", "agentmemory"),
    path.join(process.env.HOME || process.env.USERPROFILE || "", ".agentmemory", "node_modules", "@agentmemory", "agentmemory"),
    path.join(process.env.LOCALAPPDATA || "", "npm-cache", "_npx", "ba4b5775a0ab44e2", "node_modules", "@agentmemory", "agentmemory"),
  ].filter(Boolean);
  for (const root of extraRoots) {
    candidates.push(path.join(root, "dist", "index.mjs"));
  }
  return [...new Set(candidates.map((candidate) => path.resolve(candidate)))];
}

function isReplayPatchApplied(source) {
  return source.includes("toolName: isConversation ? void 0 : obs.title || void 0");
}

function isReplayHydrateApplied(source) {
  return source.includes("/* locus-replay-v3-hydrate */");
}

function isPreToolObserveApplied(source) {
  return source.includes('payload.hookType === "pre_tool_use"');
}

function isSyntheticPreToolHydrateApplied(source) {
  return source.includes("/* locus-synthetic-pre-tool-hydrate */");
}

function isCompressHydrateApplied(source) {
  return source.includes("/* locus-compress-hydrate */");
}

function isObserveSkipEmptyPreToolApplied(source) {
  return (
    source.includes("/* locus-observe-skip-empty-pre-tool */") &&
    source.includes("payloadData.toolName")
  );
}

function isPreToolObserveCamelApplied(source) {
  return source.includes('d["tool_name"] || d["toolName"]');
}

function isApiObserveGuardApplied(source) {
  return source.includes("/* locus-api-observe-guard */");
}

function isPreToolGuardV5Applied(source) {
  return source.includes("/* locus-pre-tool-guard-v5 */");
}

function isPreToolGuardV6Applied(source) {
  return source.includes("/* locus-pre-tool-guard-v6 */");
}

function isPreToolGuardV7Applied(source) {
  return source.includes("/* locus-pre-tool-guard-v7 */");
}

const PRE_TOOL_GUARD_V5_HELPERS = `/* locus-pre-tool-guard-v5 */
function locusNormalizeHookType(hookType) {
\tif (typeof hookType !== "string") return "";
\tconst s = hookType.trim().toLowerCase().replace(/-/g, "_");
\tif (s === "pretooluse" || s === "pre_tool_use") return "pre_tool_use";
\treturn hookType.trim();
}
function locusEmptyPreToolToolName(name) {
\tif (typeof name !== "string") return true;
\tconst trimmed = name.trim();
\tif (!trimmed) return true;
\tconst n = trimmed.toLowerCase().replace(/-/g, "_").replace(/\\s+/g, "_");
\tconst noise = ["pre_tool_use", "pretooluse", "pre_tool", "notification", "hook", "tool_call", "tool_use", "pre_tool_use_hook", "pretooluse_hook"];
\tif (noise.includes(n)) return true;
\tif (n.includes("pretool") || n === "pre") return true;
\tif (n.includes("hook") && !n.includes("webhook")) return true;
\treturn false;
}
function locusResolvePreToolName(raw) {
\t/* locus-pre-tool-guard-v6 */
\tlet toolName = typeof raw?.toolName === "string" ? raw.toolName.trim() : "";
\tlet payload = null;
\tif (raw?.raw && typeof raw.raw === "object" && !Array.isArray(raw.raw)) payload = raw.raw;
\telse if (raw?.data && typeof raw.data === "object" && !Array.isArray(raw.data)) payload = raw.data;
\tif (!toolName && payload) {
\t\tif (typeof payload.tool_name === "string") toolName = payload.tool_name.trim();
\t\telse if (typeof payload.toolName === "string") toolName = payload.toolName.trim();
\t}
\treturn toolName;
}
function locusShouldSkipPreToolObserve(payload, raw) {
\tconst hook = locusNormalizeHookType(payload?.hookType);
\tif (hook !== "pre_tool_use") return false;
\t// Locus records tool results via post_tool_use; pre_tool_use only produces hook-noise cards.
\treturn true;
}
`;

function patchAgentmemoryPreToolGuardV5(source) {
  let next = source;
  if (!isPreToolGuardV5Applied(next)) {
    const registerNeedle = "function registerObserveFunction(sdk, kv, dedupMap, maxObservationsPerSession) {";
    if (next.includes(registerNeedle)) {
      next = next.replace(registerNeedle, `${PRE_TOOL_GUARD_V5_HELPERS}${registerNeedle}`);
    }
  }

  const observeSkipOld = `\t\t/* locus-observe-skip-empty-pre-tool */
\t\tif (payload.hookType === "pre_tool_use") {
\t\t\tif (raw.raw && typeof raw.raw === "object" && !Array.isArray(raw.raw)) {
\t\t\t\tconst payloadData = raw.raw;
\t\t\t\tif (!raw.toolName && typeof payloadData.tool_name === "string") raw.toolName = payloadData.tool_name;
\t\t\t\tif (!raw.toolName && typeof payloadData.toolName === "string") raw.toolName = payloadData.toolName;
\t\t\t}
\t\t\tconst resolvedToolName = typeof raw.toolName === "string" ? raw.toolName.trim() : "";
\t\t\tif (!resolvedToolName || resolvedToolName === payload.hookType) {
\t\t\t\tlogger.info("Skipping empty pre_tool_use observation", { sessionId: payload.sessionId });
\t\t\t\treturn { success: true, skipped: true, reason: "empty_pre_tool_use" };
\t\t\t}
\t\t}`;
  const observeSkipNew = `\t\t/* locus-observe-skip-empty-pre-tool */
\t\tif (locusShouldSkipPreToolObserve(payload, raw)) {
\t\t\tlogger.info("Skipping empty pre_tool_use observation", { sessionId: payload.sessionId });
\t\t\treturn { success: true, skipped: true, reason: "empty_pre_tool_use" };
\t\t}`;
  if (next.includes(observeSkipOld)) {
    next = next.replace(observeSkipOld, observeSkipNew);
  }

  const apiGuardOld = `\t\t/* locus-api-observe-guard */
\t\tif (hookType === "pre_tool_use") {
\t\t\tconst data = body.data;
\t\t\tlet toolName = "";
\t\t\tif (data && typeof data === "object" && !Array.isArray(data)) {
\t\t\t\tif (typeof data.tool_name === "string") toolName = data.tool_name.trim();
\t\t\t\telse if (typeof data.toolName === "string") toolName = data.toolName.trim();
\t\t\t}
\t\t\tif (!toolName || toolName === hookType) {
\t\t\t\treturn {
\t\t\t\t\tstatus_code: 201,
\t\t\t\t\tbody: { success: true, skipped: true, reason: "empty_pre_tool_use" }
\t\t\t\t};
\t\t\t}
\t\t}`;
  const apiGuardNew = `\t\t/* locus-api-observe-guard */
\t\tif (locusNormalizeHookType(hookType) === "pre_tool_use") {
\t\t\tconst data = body.data;
\t\t\tlet toolName = "";
\t\t\tif (data && typeof data === "object" && !Array.isArray(data)) {
\t\t\t\tif (typeof data.tool_name === "string") toolName = data.tool_name.trim();
\t\t\t\telse if (typeof data.toolName === "string") toolName = data.toolName.trim();
\t\t\t}
\t\t\tif (locusEmptyPreToolToolName(toolName)) {
\t\t\t\treturn {
\t\t\t\t\tstatus_code: 201,
\t\t\t\t\tbody: { success: true, skipped: true, reason: "empty_pre_tool_use" }
\t\t\t\t};
\t\t\t}
\t\t}`;
  if (next.includes(apiGuardOld)) {
    next = next.replace(apiGuardOld, apiGuardNew);
  }

  const compressSkipOld = `\t\tif (data.raw.hookType === "pre_tool_use" && (!data.raw.toolName || data.raw.toolName === data.raw.hookType)) {
\t\t\tlogger.info("Skipping LLM compression for empty pre_tool_use hook", { obsId: data.observationId, sessionId: data.sessionId });
\t\t\treturn { success: true, skipped: true, reason: "empty_pre_tool_use" };
\t\t}`;
  const compressSkipNew = `\t\tif (locusShouldSkipPreToolObserve({ hookType: data.raw.hookType }, data.raw)) {
\t\t\ttry {
\t\t\t\tawait kv.delete(KV.observations(data.sessionId), data.observationId);
\t\t\t} catch (err) {
\t\t\t\tlogger.warn("Failed to delete empty pre_tool_use observation", {
\t\t\t\t\tobsId: data.observationId,
\t\t\t\t\tsessionId: data.sessionId,
\t\t\t\t\terror: err instanceof Error ? err.message : String(err)
\t\t\t\t});
\t\t\t}
\t\t\tlogger.info("Skipping LLM compression for empty pre_tool_use hook", { obsId: data.observationId, sessionId: data.sessionId });
\t\t\treturn { success: true, skipped: true, reason: "empty_pre_tool_use" };
\t\t}`;
  if (next.includes(compressSkipOld)) {
    next = next.replace(compressSkipOld, compressSkipNew);
  }

  const eventObserveNeedle = `sdk.registerFunction("event::observation", async (data) => sdk.trigger({
\t\tfunction_id: "mem::observe",
\t\tpayload: data
\t}));`;
  const eventObserveReplacement = `sdk.registerFunction("event::observation", async (data) => {
\t\t/* locus-event-observe-guard */
\t\tif (locusShouldSkipPreToolObserve(data, data)) {
\t\t\treturn { success: true, skipped: true, reason: "empty_pre_tool_use" };
\t\t}
\t\treturn sdk.trigger({
\t\t\tfunction_id: "mem::observe",
\t\t\tpayload: data
\t\t});
\t});`;
  if (!next.includes("/* locus-event-observe-guard */") && next.includes(eventObserveNeedle)) {
    next = next.replace(eventObserveNeedle, eventObserveReplacement);
  }

  return next === source ? null : next;
}

function patchAgentmemoryPreToolGuardV6(source) {
  let next = source;
  const resolveOld = `function locusResolvePreToolName(raw) {
\tlet toolName = typeof raw?.toolName === "string" ? raw.toolName.trim() : "";
\tif (!toolName && raw?.raw && typeof raw.raw === "object" && !Array.isArray(raw.raw)) {
\t\tconst d = raw.raw;
\t\tif (typeof d.tool_name === "string") toolName = d.tool_name.trim();
\t\telse if (typeof d.toolName === "string") toolName = d.toolName.trim();
\t}
\treturn toolName;
}`;
  const resolveNew = `function locusResolvePreToolName(raw) {
\t/* locus-pre-tool-guard-v6 */
\tlet toolName = typeof raw?.toolName === "string" ? raw.toolName.trim() : "";
\tlet payload = null;
\tif (raw?.raw && typeof raw.raw === "object" && !Array.isArray(raw.raw)) payload = raw.raw;
\telse if (raw?.data && typeof raw.data === "object" && !Array.isArray(raw.data)) payload = raw.data;
\tif (!toolName && payload) {
\t\tif (typeof payload.tool_name === "string") toolName = payload.tool_name.trim();
\t\telse if (typeof payload.toolName === "string") toolName = payload.toolName.trim();
\t}
\treturn toolName;
}`;
  if (!isPreToolGuardV6Applied(next) && next.includes(resolveOld)) {
    next = next.replace(resolveOld, resolveNew);
  }
  return next === source ? null : next;
}

function patchAgentmemoryPreToolGuardV7(source) {
  let next = source;
  if (isPreToolGuardV7Applied(next)) {
    return null;
  }

  const skipOldVariants = [
    `function locusShouldSkipPreToolObserve(payload, raw) {
\tconst hook = locusNormalizeHookType(payload?.hookType);
\tif (hook !== "pre_tool_use") return false;
\tconst toolName = locusResolvePreToolName(raw);
\tif (locusEmptyPreToolToolName(toolName)) return true;
\treturn false;
}`,
    `function locusShouldSkipPreToolObserve(payload, raw) {
\tconst hook = locusNormalizeHookType(payload?.hookType);
\tif (hook !== "pre_tool_use") return false;
\treturn true;
}`,
  ];
  const skipNew = `function locusShouldSkipPreToolObserve(payload, raw) {
\t/* locus-pre-tool-guard-v7 */
\tconst hook = locusNormalizeHookType(payload?.hookType);
\tif (hook !== "pre_tool_use") return false;
\treturn true;
}`;
  for (const old of skipOldVariants) {
    if (next.includes(old)) {
      next = next.replace(old, skipNew);
      break;
    }
  }

  const apiGuardPartial = `\t\t/* locus-api-observe-guard */
\t\tif (locusNormalizeHookType(hookType) === "pre_tool_use") {
\t\t\tconst data = body.data;
\t\t\tlet toolName = "";
\t\t\tif (data && typeof data === "object" && !Array.isArray(data)) {
\t\t\t\tif (typeof data.tool_name === "string") toolName = data.tool_name.trim();
\t\t\t\telse if (typeof data.toolName === "string") toolName = data.toolName.trim();
\t\t\t}
\t\t\tif (locusEmptyPreToolToolName(toolName)) {
\t\t\t\treturn {
\t\t\t\t\tstatus_code: 201,
\t\t\t\t\tbody: { success: true, skipped: true, reason: "empty_pre_tool_use" }
\t\t\t\t};
\t\t\t}
\t\t}`;
  const apiGuardAllPre = `\t\t/* locus-api-observe-guard-v7 */
\t\tif (locusNormalizeHookType(hookType) === "pre_tool_use") {
\t\t\treturn {
\t\t\t\tstatus_code: 201,
\t\t\t\tbody: { success: true, skipped: true, reason: "pre_tool_use_disabled" }
\t\t\t};
\t\t}`;
  if (next.includes(apiGuardPartial)) {
    next = next.replace(apiGuardPartial, apiGuardAllPre);
  }

  const legacyApiGuard = `\t\t/* locus-api-observe-guard */
\t\tif (hookType === "pre_tool_use") {
\t\t\tconst data = body.data;
\t\t\tlet toolName = "";
\t\t\tif (data && typeof data === "object" && !Array.isArray(data)) {
\t\t\t\tif (typeof data.tool_name === "string") toolName = data.tool_name.trim();
\t\t\t\telse if (typeof data.toolName === "string") toolName = data.toolName.trim();
\t\t\t}
\t\t\tif (!toolName || toolName === hookType) {
\t\t\t\treturn {
\t\t\t\t\tstatus_code: 201,
\t\t\t\t\tbody: { success: true, skipped: true, reason: "empty_pre_tool_use" }
\t\t\t\t};
\t\t\t}
\t\t}`;
  if (next.includes(legacyApiGuard)) {
    next = next.replace(legacyApiGuard, apiGuardAllPre);
  }

  if (!isPreToolGuardV7Applied(next) && next.includes("function locusShouldSkipPreToolObserve")) {
    next = next.replace(
      "function locusShouldSkipPreToolObserve(payload, raw) {",
      "function locusShouldSkipPreToolObserve(payload, raw) {\n\t/* locus-pre-tool-guard-v7 */",
    );
  }

  return next === source ? null : next;
}

function patchAgentmemoryIndexFile(indexPath) {
  if (!existsSync(indexPath)) {
    return false;
  }
  const marker = "/* locus-replay-v2 */";
  let source = readFileSync(indexPath, "utf8");
  const original = source;
  // Repair earlier patch runs that left `/* locus-replay-v2 */-raw` before the function.
  source = source.replace(/\/\* locus-replay-v2 \*\/-raw\s*\n(?=function rawFromCompressed)/, "");
  source = source.replace(/\/\* locus-replay-v2 \*\/-body\s*\n(?=function bodyFor)/, "");

  if (
    isReplayPatchApplied(source) &&
    isReplayHydrateApplied(source) &&
    isPreToolObserveApplied(source) &&
    isSyntheticPreToolHydrateApplied(source) &&
    isCompressHydrateApplied(source) &&
    isObserveSkipEmptyPreToolApplied(source) &&
    isPreToolObserveCamelApplied(source) &&
    isApiObserveGuardApplied(source) &&
    isPreToolGuardV5Applied(source) &&
    isPreToolGuardV6Applied(source) &&
    isPreToolGuardV7Applied(source) &&
    source.includes("locus-observations-hydrate") &&
    source === original
  ) {
    return false;
  }

  const v5 = patchAgentmemoryPreToolGuardV5(source);
  if (v5) {
    source = v5;
  }
  const v6 = patchAgentmemoryPreToolGuardV6(source);
  if (v6) {
    source = v6;
  }
  const v7 = patchAgentmemoryPreToolGuardV7(source);
  if (v7) {
    source = v7;
  }

  const observeNeedle = `\t\t\tif (payload.hookType === "prompt_submit") raw.userPrompt = d["prompt"];`;
  const observeReplacement = `\t\t\tif (payload.hookType === "pre_tool_use") {
\t\t\t\traw.toolName = d["tool_name"] || d["toolName"];
\t\t\t\traw.toolInput = d["tool_input"] ?? d["toolInput"];
\t\t\t}
\t\t\tif (payload.hookType === "prompt_submit") raw.userPrompt = d["prompt"];`;
  if (source.includes(observeNeedle)) {
    if (!isPreToolObserveApplied(source)) {
      source = source.replace(observeNeedle, observeReplacement);
    } else if (!isPreToolObserveCamelApplied(source)) {
      source = source.replace(
        `\t\t\tif (payload.hookType === "pre_tool_use") {
\t\t\t\traw.toolName = d["tool_name"];
\t\t\t\traw.toolInput = d["tool_input"];
\t\t\t}`,
        `\t\t\tif (payload.hookType === "pre_tool_use") {
\t\t\t\traw.toolName = d["tool_name"] || d["toolName"];
\t\t\t\traw.toolInput = d["tool_input"] ?? d["toolInput"];
\t\t\t}`,
      );
    }
  }

  const observeSkipNeedle = `\t\t} else if (typeof sanitizedRaw === "string") {
\t\t\textractedImage = extractImage(sanitizedRaw);
\t\t\tif (extractedImage) raw.modality = "image";
\t\t}
\t\tconst pendingImageData = extractedImage;`;
  const observeSkipReplacement = `\t\t} else if (typeof sanitizedRaw === "string") {
\t\t\textractedImage = extractImage(sanitizedRaw);
\t\t\tif (extractedImage) raw.modality = "image";
\t\t}
\t\t/* locus-observe-skip-empty-pre-tool */
\t\tif (payload.hookType === "pre_tool_use") {
\t\t\tif (raw.raw && typeof raw.raw === "object" && !Array.isArray(raw.raw)) {
\t\t\t\tconst payloadData = raw.raw;
\t\t\t\tif (!raw.toolName && typeof payloadData.tool_name === "string") raw.toolName = payloadData.tool_name;
\t\t\t\tif (!raw.toolName && typeof payloadData.toolName === "string") raw.toolName = payloadData.toolName;
\t\t\t}
\t\t\tconst resolvedToolName = typeof raw.toolName === "string" ? raw.toolName.trim() : "";
\t\t\tif (!resolvedToolName || resolvedToolName === payload.hookType) {
\t\t\t\tlogger.info("Skipping empty pre_tool_use observation", { sessionId: payload.sessionId });
\t\t\t\treturn { success: true, skipped: true, reason: "empty_pre_tool_use" };
\t\t\t}
\t\t}
\t\tconst pendingImageData = extractedImage;`;
  if (!isObserveSkipEmptyPreToolApplied(source) && source.includes(observeSkipNeedle)) {
    source = source.replace(observeSkipNeedle, observeSkipReplacement);
  }

  const apiObserveNeedle = `\t\tconst payload = {
\t\t\thookType,
\t\t\tsessionId,
\t\t\tproject,
\t\t\tcwd,
\t\t\ttimestamp,
\t\t\tdata: body.data
\t\t};
\t\treturn {
\t\t\tstatus_code: 201,
\t\t\tbody: await sdk.trigger({
\t\t\t\tfunction_id: "mem::observe",
\t\t\t\tpayload
\t\t\t})
\t\t};`;
  const apiObserveReplacement = `\t\t/* locus-api-observe-guard */
\t\tif (hookType === "pre_tool_use") {
\t\t\tconst data = body.data;
\t\t\tlet toolName = "";
\t\t\tif (data && typeof data === "object" && !Array.isArray(data)) {
\t\t\t\tif (typeof data.tool_name === "string") toolName = data.tool_name.trim();
\t\t\t\telse if (typeof data.toolName === "string") toolName = data.toolName.trim();
\t\t\t}
\t\t\tif (!toolName || toolName === hookType) {
\t\t\t\treturn {
\t\t\t\t\tstatus_code: 201,
\t\t\t\t\tbody: { success: true, skipped: true, reason: "empty_pre_tool_use" }
\t\t\t\t};
\t\t\t}
\t\t}
\t\tconst payload = {
\t\t\thookType,
\t\t\tsessionId,
\t\t\tproject,
\t\t\tcwd,
\t\t\ttimestamp,
\t\t\tdata: body.data
\t\t};
\t\treturn {
\t\t\tstatus_code: 201,
\t\t\tbody: await sdk.trigger({
\t\t\t\tfunction_id: "mem::observe",
\t\t\t\tpayload
\t\t\t})
\t\t};`;
  if (!isApiObserveGuardApplied(source) && source.includes(apiObserveNeedle)) {
    source = source.replace(apiObserveNeedle, apiObserveReplacement);
  }

  const syntheticNeedle = `function buildSyntheticCompression(raw) {
\tconst toolName = raw.toolName ?? raw.hookType;
\tconst inputStr = stringifyForNarrative(raw.toolInput);`;
  const syntheticReplacement = `function buildSyntheticCompression(raw) {
\t/* locus-synthetic-pre-tool-hydrate */
\tlet toolName = raw.toolName;
\tlet toolInput = raw.toolInput;
\tif ((!toolName || toolName === raw.hookType) && raw.raw && typeof raw.raw === "object" && !Array.isArray(raw.raw)) {
\t\tconst payload = raw.raw;
\t\tif (typeof payload.tool_name === "string" && payload.tool_name.trim()) toolName = payload.tool_name.trim();
\t\telse if (typeof payload.toolName === "string" && payload.toolName.trim()) toolName = payload.toolName.trim();
\t\tif (toolInput === void 0 && payload.tool_input !== void 0) toolInput = payload.tool_input;
\t\telse if (toolInput === void 0 && payload.toolInput !== void 0) toolInput = payload.toolInput;
\t}
\tconst resolvedToolName = toolName && toolName !== raw.hookType ? toolName : toolName ?? raw.hookType;
\tconst inputStr = stringifyForNarrative(toolInput);`;
  if (!isSyntheticPreToolHydrateApplied(source) && source.includes(syntheticNeedle)) {
    source = source.replace(syntheticNeedle, syntheticReplacement);
    source = source.replace(
      "\t\ttitle: truncate$2(toolName || \"observation\", 80),",
      "\t\ttitle: truncate$2(resolvedToolName || \"observation\", 80),",
    );
    source = source.replace(
      "\t\ttype: inferType(toolName, raw.hookType),",
      "\t\ttype: inferType(resolvedToolName, raw.hookType),",
    );
  }

  const compressNeedle = `\t\tconst prompt = buildCompressionPrompt({
\t\t\thookType: data.raw.hookType,
\t\t\ttoolName: data.raw.toolName,`;
  const compressReplacement = `\t\t/* locus-compress-hydrate */
\t\tif (data.raw.raw && typeof data.raw.raw === "object" && !Array.isArray(data.raw.raw)) {
\t\t\tconst payload = data.raw.raw;
\t\t\tif (!data.raw.toolName && typeof payload.tool_name === "string") data.raw.toolName = payload.tool_name;
\t\t\tif (data.raw.toolInput === void 0 && payload.tool_input !== void 0) data.raw.toolInput = payload.tool_input;
\t\t}
\t\tif (data.raw.hookType === "pre_tool_use" && (!data.raw.toolName || data.raw.toolName === data.raw.hookType)) {
\t\t\tlogger.info("Skipping LLM compression for empty pre_tool_use hook", { obsId: data.observationId, sessionId: data.sessionId });
\t\t\treturn { success: true, skipped: true, reason: "empty_pre_tool_use" };
\t\t}
\t\tconst prompt = buildCompressionPrompt({
\t\t\thookType: data.raw.hookType,
\t\t\ttoolName: data.raw.toolName,`;
  if (!isCompressHydrateApplied(source) && source.includes(compressNeedle)) {
    source = source.replace(compressNeedle, compressReplacement);
  }

  const bodyForReplacement = `${marker}
function bodyFor(obs, kind) {
\tif (kind === "prompt") return obs.userPrompt ?? obs.raw?.narrative;
\tif (kind === "response") return obs.assistantResponse;
\tif (kind === "tool_result" || kind === "tool_error") {
\t\tif (typeof obs.toolOutput === "string" && obs.toolOutput.trim()) return obs.toolOutput;
\t\tconst facts = obs.raw?.facts;
\t\tif (Array.isArray(facts) && facts.length) return facts.filter(Boolean).join("\\n• ");
\t\treturn obs.raw?.narrative;
\t}
\tif (kind === "tool_call") {
\t\tif (typeof obs.toolInput === "string" && obs.toolInput.trim()) return obs.toolInput;
\t\tif (obs.toolInput !== void 0) try {
\t\t\treturn JSON.stringify(obs.toolInput, null, 2);
\t\t} catch {
\t\t\treturn void 0;
\t\t}
\t\tconst nested = obs.raw?.tool_input ?? obs.raw?.toolInput;
\t\tif (nested !== void 0) try {
\t\t\treturn typeof nested === "string" ? nested : JSON.stringify(nested, null, 2);
\t\t} catch {
\t\t\treturn void 0;
\t\t}
\t}
}`;

  if (!isReplayPatchApplied(source) && source.includes("function bodyFor(obs, kind)")) {
    source = source.replace(/^function bodyFor\(obs, kind\) \{[\s\S]*?\n\}/m, bodyForReplacement);
  }

  const rawFromCompressedReplacement = `${marker}
function rawFromCompressed(obs) {
\tconst isConversation = obs.type === "conversation";
\tconst hookType = isConversation ? "prompt_submit" : obs.type === "error" ? "post_tool_failure" : "post_tool_use";
\tconst factsText = Array.isArray(obs.facts) ? obs.facts.filter(Boolean).join("\\n• ") : "";
\tconst narrative = String(obs.narrative || factsText || "").trim();
\tconst subtitle = String(obs.subtitle || "").trim();
\treturn {
\t\tid: obs.id,
\t\tsessionId: obs.sessionId,
\t\ttimestamp: obs.timestamp,
\t\thookType,
\t\ttoolName: isConversation ? void 0 : obs.title || void 0,
\t\ttoolInput: isConversation ? void 0 : subtitle || void 0,
\t\ttoolOutput: isConversation ? void 0 : narrative || subtitle || void 0,
\t\tuserPrompt: isConversation ? narrative : void 0,
\t\tassistantResponse: void 0,
\t\traw: {
\t\t\ttitle: obs.title,
\t\t\tnarrative: obs.narrative,
\t\t\tfacts: obs.facts,
\t\t\ttype: obs.type
\t\t}
\t};
}`;

  if (!isReplayPatchApplied(source) && source.includes("function rawFromCompressed(obs)")) {
    source = source.replace(/^function rawFromCompressed\(obs\) \{[\s\S]*?\n\}/m, rawFromCompressedReplacement);
  }

  const compressFailNeedle = `\t\t\t\treturn {
\t\t\t\t\tsuccess: false,
\t\t\t\t\terror: "parse_failed"
\t\t\t\t};`;
  const compressFailReplacement = `${marker}-compress
\t\t\t\tconst synthetic = buildSyntheticCompression(data.raw);
\t\t\t\tawait kv.set(KV.observations(data.sessionId), data.observationId, synthetic);
\t\t\t\tgetSearchIndex().add(synthetic);
\t\t\t\treturn {
\t\t\t\t\tsuccess: true,
\t\t\t\t\tcompressed: synthetic,
\t\t\t\t\tqualityScore: 0,
\t\t\t\t\tfallback: "synthetic"
\t\t\t\t};`;
  if (!source.includes(`${marker}-compress`) && source.includes(compressFailNeedle)) {
    source = source.replace(compressFailNeedle, compressFailReplacement);
  }

  const hydrateMarker = "/* locus-replay-v3-hydrate */";
  const hydrateReplacement = `${hydrateMarker}
function hydrateObservationFields(obs) {
\tconst payload = obs?.raw;
\tif (!payload || typeof payload !== "object" || Array.isArray(payload)) return obs;
\tif (!obs.toolName && typeof payload.tool_name === "string") obs.toolName = payload.tool_name;
\tif (obs.toolInput === void 0 && payload.tool_input !== void 0) obs.toolInput = payload.tool_input;
\tif (obs.toolOutput === void 0 && (payload.tool_output !== void 0 || payload.error !== void 0)) obs.toolOutput = payload.tool_output ?? payload.error;
\tif (!obs.userPrompt && typeof payload.prompt === "string") obs.userPrompt = payload.prompt;
\treturn obs;
}`;
  const inlineHydrateBlock = `${hydrateMarker}
function hydrateObservationFields(obs) {
\tconst payload = obs?.raw;
\tif (!payload || typeof payload !== "object" || Array.isArray(payload)) return obs;
\tif (!obs.toolName && typeof payload.tool_name === "string") obs.toolName = payload.tool_name;
\tif (obs.toolInput === void 0 && payload.tool_input !== void 0) obs.toolInput = payload.tool_input;
\tif (obs.toolOutput === void 0 && (payload.tool_output !== void 0 || payload.error !== void 0)) obs.toolOutput = payload.tool_output ?? payload.error;
\tif (!obs.userPrompt && typeof payload.prompt === "string") obs.userPrompt = payload.prompt;
\treturn obs;
}
\tfor (const obs of sorted) {
\t\thydrateObservationFields(obs);
\t\tconst kind = kindFromHook(obs);`;
  const projectTimelineNeedle = `\tfor (const obs of sorted) {
\t\tconst kind = kindFromHook(obs);`;
  const projectTimelineReplacement = `\tfor (const obs of sorted) {
\t\thydrateObservationFields(obs);
\t\tconst kind = kindFromHook(obs);`;
  if (source.includes(inlineHydrateBlock)) {
    source = source.replace(inlineHydrateBlock, projectTimelineReplacement);
  }
  if (!isReplayHydrateApplied(source) && source.includes("function projectTimeline(observations) {")) {
    source = source.replace(
      "function projectTimeline(observations) {",
      `${hydrateReplacement}
function projectTimeline(observations) {`,
    );
  }
  if (!source.includes("\t\thydrateObservationFields(obs);") && source.includes(projectTimelineNeedle)) {
    source = source.replace(projectTimelineNeedle, projectTimelineReplacement);
  }

  const obsApiNeedle = `\t\tconst observations = await kv.list(KV.observations(sessionId));
\t\tconst normalizedAgentId`;
  const obsApiReplacement = `\t\tconst observations = (await kv.list(KV.observations(sessionId))).map((o) => hydrateObservationFields({ ...o }));
\t\tconst normalizedAgentId`;
  if (!source.includes("locus-observations-hydrate") && source.includes(obsApiNeedle)) {
    source = source.replace(obsApiNeedle, obsApiReplacement.replace("hydrateObservationFields({ ...o })", "/* locus-observations-hydrate */ hydrateObservationFields({ ...o })"));
  }

  if (source === original) {
    return false;
  }
  writeFileSync(indexPath, source);
  return true;
}

function patchAgentmemoryReplay() {
  let patched = 0;
  for (const indexPath of agentmemoryIndexCandidates()) {
    if (patchAgentmemoryIndexFile(indexPath)) {
      patched += 1;
      console.log(`[locus] Patched agentmemory replay mapping (v3): ${path.relative(repoRoot, indexPath)}`);
    }
  }
  if (patched === 0 && existsSync(agentmemoryIndexPath())) {
    console.log("[locus] agentmemory replay mapping (v2) already present");
  }
}

function agentmemoryViewerCandidates() {
  return agentmemoryIndexCandidates()
    .map((indexPath) => path.join(path.dirname(indexPath), "viewer", "index.html"))
    .filter((viewerPath) => existsSync(viewerPath));
}

function patchAgentmemoryViewerFile(viewerPath) {
  let source = readFileSync(viewerPath, "utf8");
  const original = source;
  if (source.includes("/* locus-viewer-timeline-v1 */")) {
    return false;
  }

  const toolMapNeedle =
    "var TOOL_TYPE_MAP = { Read: 'file_read', Write: 'file_write', Edit: 'file_edit', Bash: 'command_run', Grep: 'search', Glob: 'search', WebFetch: 'web_fetch', WebSearch: 'web_fetch', AskUserQuestion: 'conversation', Task: 'subagent' };";
  const toolMapReplacement =
    "var TOOL_TYPE_MAP = { Read: 'file_read', Write: 'file_write', Edit: 'file_edit', Bash: 'command_run', Grep: 'search', Glob: 'search', WebFetch: 'web_fetch', WebSearch: 'web_fetch', AskUserQuestion: 'conversation', Task: 'subagent', read: 'file_read', write: 'file_write', edit: 'file_edit', bash: 'command_run', grep: 'search', glob: 'search', list: 'search', task: 'subagent', ask_user_question: 'conversation', codegraph_search: 'search', unity_yaml_read: 'file_read', unity_yaml_search: 'search', memory_recall: 'search', memory_save: 'conversation' };";
  if (source.includes(toolMapNeedle)) {
    source = source.replace(toolMapNeedle, toolMapReplacement);
  }

  const renderNeedle = "    function renderObservations() {";
  const renderReplacement = `    /* locus-viewer-timeline-v1 */
    function locusResolveToolName(o) {
      if (!o) return '';
      if (o.toolName) return o.toolName;
      if (o.raw && typeof o.raw === 'object') return o.raw.tool_name || o.raw.toolName || '';
      return '';
    }
    function locusObsTitle(o) {
      if (o.title) return o.title;
      var name = locusResolveToolName(o);
      if (name) {
        if (o.hookType === 'pre_tool_use') return name + ' ▸ call';
        if (o.hookType === 'post_tool_use') return name + ' ▸ result';
        if (o.hookType === 'post_tool_failure') return name + ' ▸ error';
        return name;
      }
      if (o.hookType) return o.hookType.replace(/_/g, ' ');
      return 'Observation';
    }
    function locusObsType(o, toolTypeMap) {
      if (o.type) return o.type;
      var name = locusResolveToolName(o);
      if (name) {
        var mapped = toolTypeMap[name] || toolTypeMap[name.charAt(0).toUpperCase() + name.slice(1)];
        if (mapped) return mapped;
        return name;
      }
      if (o.hookType === 'pre_tool_use') return 'tool_call';
      if (o.hookType === 'post_tool_use' || o.hookType === 'post_tool_failure') return 'tool_result';
      if (o.hookType) return o.hookType.replace(/_/g, ' ');
      return 'other';
    }

    function renderObservations() {`;
  if (source.includes(renderNeedle)) {
    source = source.replace(renderNeedle, renderReplacement);
  }

  source = source.replace(
    /var t = o\.type \|\| TOOL_TYPE_MAP\[o\.toolName\] \|\| \(o\.hookType \? o\.hookType\.replace\(\/_\/g, ' '\) : 'other'\);/g,
    "var t = locusObsType(o, TOOL_TYPE_MAP);",
  );
  source = source.replace(
    "var type = o.type || TOOL_TYPE_MAP[o.toolName] || 'other';",
    "var type = locusObsType(o, TOOL_TYPE_MAP);",
  );
  source = source.replace(
    "var title = o.title || o.toolName || (o.hookType ? o.hookType.replace(/_/g, ' ') : 'Observation');",
    "var title = locusObsTitle(o);",
  );

  if (source === original) {
    return false;
  }
  writeFileSync(viewerPath, source);
  return true;
}

function patchAgentmemoryViewerV3(viewerPath) {
  let source = readFileSync(viewerPath, "utf8");
  const original = source;
  const noiseFnNeedle = `    function locusIsHookNoiseObservation(o) {
      if (!o) return true;
      var hydrated = typeof locusHydrateObservation === 'function' ? locusHydrateObservation(o) : o;
      var tool = locusResolveToolName(hydrated);
      if (tool) return false;
      var text = ((hydrated.title || '') + ' ' + (hydrated.narrative || '') + ' ' + (hydrated.subtitle || '')).toLowerCase();
      if (!text.trim()) return hydrated.hookType === 'pre_tool_use';
      var markers = [
        'pre-tool-use hook', 'pre_tool_use hook', 'pretooluse hook', 'hook fired before tool execution with no payload',
        'hook notification', 'hook event lifecycle',
        'hook event with no', 'no tool details', 'no associated tool', 'minimal hook notification',
        'no actionable content', 'standalone hook firing', 'bare hook trigger'
      ];
      return markers.some(function(m) { return text.indexOf(m) >= 0; });
    }`;
  const noiseFnReplacement = `    /* locus-viewer-timeline-v3 */
    function locusIsHookNoiseObservation(o) {
      if (!o) return true;
      var hydrated = typeof locusHydrateObservation === 'function' ? locusHydrateObservation(o) : o;
      var tool = locusResolveToolName(hydrated);
      if (tool) return false;
      var realToolTypes = ['file_read','file_write','file_edit','command_run','search','web_fetch','subagent','error','decision','discovery','task','image'];
      if (hydrated.type && realToolTypes.indexOf(hydrated.type) >= 0) return false;
      if (hydrated.hookType === 'post_tool_use' || hydrated.hookType === 'post_tool_failure') return false;
      var text = ((hydrated.title || '') + ' ' + (hydrated.narrative || '') + ' ' + (hydrated.subtitle || '')).toLowerCase();
      if (!text.trim()) return hydrated.hookType === 'pre_tool_use';
      var markers = [
        'pre-tool-use hook', 'pre_tool_use hook', 'pretooluse hook', 'hook fired before tool execution with no payload',
        'hook notification', 'hook event lifecycle',
        'hook event with no', 'no tool details', 'no associated tool', 'minimal hook notification',
        'no actionable content', 'standalone hook firing', 'bare hook trigger'
      ];
      return markers.some(function(m) { return text.indexOf(m) >= 0; });
    }`;
  if (source.includes(noiseFnNeedle)) {
    source = source.replace(noiseFnNeedle, noiseFnReplacement);
    if (source !== original) {
      writeFileSync(viewerPath, source);
      return true;
    }
    return false;
  }
  if (source.includes("realToolTypes")) {
    return false;
  }
  if (!source.includes("function locusResolveToolName(o)")) {
    return false;
  }

  const noiseFn = `    /* locus-viewer-timeline-v3 */
    function locusIsHookNoiseObservation(o) {
      if (!o) return true;
      var hydrated = typeof locusHydrateObservation === 'function' ? locusHydrateObservation(o) : o;
      var tool = locusResolveToolName(hydrated);
      if (tool) return false;
      var realToolTypes = ['file_read','file_write','file_edit','command_run','search','web_fetch','subagent','error','decision','discovery','task','image'];
      if (hydrated.type && realToolTypes.indexOf(hydrated.type) >= 0) return false;
      if (hydrated.hookType === 'post_tool_use' || hydrated.hookType === 'post_tool_failure') return false;
      var text = ((hydrated.title || '') + ' ' + (hydrated.narrative || '') + ' ' + (hydrated.subtitle || '')).toLowerCase();
      if (!text.trim()) return hydrated.hookType === 'pre_tool_use';
      var markers = [
        'pre-tool-use hook', 'pre_tool_use hook', 'pretooluse hook', 'hook fired before tool execution with no payload',
        'hook notification', 'hook event lifecycle',
        'hook event with no', 'no tool details', 'no associated tool', 'minimal hook notification',
        'no actionable content', 'standalone hook firing', 'bare hook trigger'
      ];
      return markers.some(function(m) { return text.indexOf(m) >= 0; });
    }
`;

  source = source.replace(
    "function locusResolveToolName(o) {",
    `${noiseFn}
    function locusResolveToolName(o) {`,
  );

  source = source.replace(
    "var obs = state.timeline.observations.map(function(o) { return locusHydrateObservation(o); });",
    "var obs = state.timeline.observations.map(function(o) { return locusHydrateObservation(o); }).filter(function(o) { return !locusIsHookNoiseObservation(o); });",
  );

  source = source.replace(
    "state.timeline.observations = ((result && result.observations) || []).map(function(o) { return locusHydrateObservation(o); });",
    "state.timeline.observations = ((result && result.observations) || []).map(function(o) { return locusHydrateObservation(o); }).filter(function(o) { return !locusIsHookNoiseObservation(o); });",
  );

  if (source === original) {
    return false;
  }
  writeFileSync(viewerPath, source);
  return true;
}

function patchAgentmemoryViewerV4(viewerPath) {
  let source = readFileSync(viewerPath, "utf8");
  const original = source;
  if (source.includes("/* locus-viewer-timeline-v4 */")) {
    return false;
  }

  const loadNeedle =
    "state.timeline.observations = ((result && result.observations) || []).map(function(o) { return locusHydrateObservation(o); }).filter(function(o) { return !locusIsHookNoiseObservation(o); });";
  const loadReplacement = `      /* locus-viewer-timeline-v4 */
      var rawObs = (result && result.observations) || [];
      state.timeline.observationsRawCount = rawObs.length;
      state.timeline.observations = rawObs.map(function(o) { return locusHydrateObservation(o); }).filter(function(o) { return !locusIsHookNoiseObservation(o); });`;

  const emptyNeedle = `      if (paged.length === 0) {
        html += '<div class="empty-state"><div class="empty-icon">&#128337;</div><p>No observations' + (obs.length > 0 ? ' match the filter (' + obs.length + ' total)' : ' for this session') + '</p></div>';
        content.innerHTML = html;
        return;
      }`;
  const emptyReplacement = `      /* locus-viewer-timeline-v4 */
      if (paged.length === 0) {
        var rawTotal = (state.timeline.observationsRawCount != null) ? state.timeline.observationsRawCount : obs.length;
        var hiddenNoise = rawTotal > obs.length ? (rawTotal - obs.length) : 0;
        var emptyMsg = 'No observations for this session';
        if (rawTotal > 0 && obs.length === 0) {
          emptyMsg = hiddenNoise > 0
            ? ('All ' + rawTotal + ' observations are empty hook noise (hidden). Approve pending tools in Locus, then retry.')
            : ('All ' + rawTotal + ' observations were hidden by filters. Lower the importance threshold or clear the type filter.');
        } else if (obs.length > 0) {
          emptyMsg = 'No observations match the filter (' + obs.length + ' total)';
        }
        html += '<div class="empty-state"><div class="empty-icon">&#128337;</div><p>' + esc(emptyMsg) + '</p></div>';
        content.innerHTML = html;
        return;
      }`;

  if (!source.includes(loadNeedle) || !source.includes(emptyNeedle)) {
    return false;
  }
  source = source.replace(loadNeedle, loadReplacement);
  source = source.replace(emptyNeedle, emptyReplacement);

  if (source === original) {
    return false;
  }
  writeFileSync(viewerPath, source);
  return true;
}

const LOCUS_VIEWER_NOISE_FN_V5 = `    /* locus-viewer-timeline-v5 */
    function locusIsHookNoiseObservation(o) {
      if (!o) return true;
      var hydrated = typeof locusHydrateObservation === 'function' ? locusHydrateObservation(o) : o;
      if (hydrated.hookType === 'pre_tool_use') return true;
      if (hydrated.hookType === 'post_tool_use' || hydrated.hookType === 'post_tool_failure') return false;
      var realToolTypes = ['file_read','file_write','file_edit','command_run','search','web_fetch','subagent','error','decision','discovery','task','image'];
      if (hydrated.type && realToolTypes.indexOf(hydrated.type) >= 0) return false;
      var text = ((hydrated.title || '') + ' ' + (hydrated.narrative || '') + ' ' + (hydrated.subtitle || '')).toLowerCase();
      var markers = [
        'pre-tool-use hook', 'pre_tool_use hook', 'pretooluse hook', 'hook fired before tool execution with no payload',
        'hook notification', 'hook event lifecycle', 'hook triggered', 'hook fired',
        'hook event with no', 'no tool details', 'no associated tool', 'minimal hook notification',
        'no actionable content', 'standalone hook firing', 'bare hook trigger', 'pretooluse', 'pre tool use hook'
      ];
      if (markers.some(function(m) { return text.indexOf(m) >= 0; })) return true;
      var tool = locusResolveToolName(hydrated);
      if (tool) return false;
      if (!text.trim()) return true;
      return false;
    }`;

function patchAgentmemoryViewerV5(viewerPath) {
  let source = readFileSync(viewerPath, "utf8");
  const original = source;
  if (source.includes("/* locus-viewer-timeline-v5 */")) {
    return false;
  }
  const v3Start = source.indexOf("/* locus-viewer-timeline-v3 */");
  const fnStart = source.indexOf("function locusIsHookNoiseObservation(o)");
  if (fnStart < 0) {
    return false;
  }
  const fnEnd = source.indexOf("\n    function ", fnStart + 1);
  if (fnEnd < 0) {
    return false;
  }
  const blockStart = v3Start >= 0 && v3Start < fnStart ? v3Start : fnStart;
  source = source.slice(0, blockStart) + LOCUS_VIEWER_NOISE_FN_V5 + source.slice(fnEnd);
  if (source === original) {
    return false;
  }
  writeFileSync(viewerPath, source);
  return true;
}

function patchAgentmemoryViewerV2(viewerPath) {
  let source = readFileSync(viewerPath, "utf8");
  const original = source;
  if (source.includes("/* locus-viewer-timeline-v2 */")) {
    return false;
  }

  const hydrateFn = `    /* locus-viewer-timeline-v2 */
    function locusHydrateObservation(o) {
      if (!o || typeof o !== 'object') return o;
      var obs = Object.assign({}, o);
      var payload = obs.raw;
      if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return obs;
      if (!obs.toolName && typeof payload.tool_name === 'string') obs.toolName = payload.tool_name;
      if (obs.toolInput === void 0 && payload.tool_input !== void 0) obs.toolInput = payload.tool_input;
      if (obs.toolOutput === void 0 && (payload.tool_output !== void 0 || payload.error !== void 0)) {
        obs.toolOutput = payload.tool_output !== void 0 ? payload.tool_output : payload.error;
      }
      if (!obs.userPrompt && typeof payload.prompt === 'string') obs.userPrompt = payload.prompt;
      return obs;
    }
`;

  if (source.includes("function locusObsType(o, toolTypeMap) {")) {
    source = source.replace(
      "function locusObsType(o, toolTypeMap) {",
      `${hydrateFn}
    function locusObsType(o, toolTypeMap) {`,
    );
  }

  source = source.replace(
    "state.timeline.observations = (result && result.observations) || [];",
    "state.timeline.observations = ((result && result.observations) || []).map(function(o) { return locusHydrateObservation(o); });",
  );

  source = source.replace(
    "var obs = state.timeline.observations;",
    "var obs = state.timeline.observations.map(function(o) { return locusHydrateObservation(o); });",
  );

  source = source.replace(
    "var isCompressed = !!o.narrative || !!o.type;\n        var isRaw = !isCompressed;",
    "var isCompressed = !!(o.narrative && String(o.narrative).trim()) || (o.facts && o.facts.length > 0);\n        var isRaw = !isCompressed && (!!o.hookType || !!locusResolveToolName(o));",
  );

  if (source === original) {
    return false;
  }
  writeFileSync(viewerPath, source);
  return true;
}

function patchAgentmemoryViewer() {
  let patched = 0;
  for (const viewerPath of agentmemoryViewerCandidates()) {
    if (patchAgentmemoryViewerFile(viewerPath)) {
      patched += 1;
      console.log(`[locus] Patched agentmemory viewer timeline (v1): ${path.relative(repoRoot, viewerPath)}`);
    }
    if (patchAgentmemoryViewerV2(viewerPath)) {
      patched += 1;
      console.log(`[locus] Patched agentmemory viewer timeline (v2): ${path.relative(repoRoot, viewerPath)}`);
    }
    if (patchAgentmemoryViewerV3(viewerPath)) {
      patched += 1;
      console.log(`[locus] Patched agentmemory viewer timeline (v3): ${path.relative(repoRoot, viewerPath)}`);
    }
    if (patchAgentmemoryViewerV4(viewerPath)) {
      patched += 1;
      console.log(`[locus] Patched agentmemory viewer timeline (v4): ${path.relative(repoRoot, viewerPath)}`);
    }
    if (patchAgentmemoryViewerV5(viewerPath)) {
      patched += 1;
      console.log(`[locus] Patched agentmemory viewer timeline (v5): ${path.relative(repoRoot, viewerPath)}`);
    }
  }
}

function iiiBinaryPath() {
  return path.join(bundleDir, "bin", iiiBinaryName());
}

function isBundleReady() {
  return existsSync(cliEntryPath()) && existsSync(iiiBinaryPath());
}

function writeManifest(entry) {
  writeFileSync(
    path.join(bundleDir, "manifest.json"),
    `${JSON.stringify(
      {
        version: 1,
        generatedAt: new Date().toISOString(),
        target: platformTarget(),
        agentmemoryVersion: readPinnedVersion(),
        iiiVersion: readIiiVersion(),
        replayPatchVersion: 3,
        timelineViewerPatchVersion: 2,
        ...entry,
      },
      null,
      2,
    )}\n`,
  );
}

function request(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(
      url,
      { headers: { "User-Agent": "Locus agentmemory bundler" } },
      (response) => {
        if ([301, 302, 303, 307, 308].includes(response.statusCode ?? 0)) {
          const location = response.headers.location;
          response.resume();
          if (!location) {
            reject(new Error(`redirect without location for ${url}`));
            return;
          }
          request(new URL(location, url).toString()).then(resolve, reject);
          return;
        }
        if (response.statusCode !== 200) {
          response.resume();
          reject(new Error(`request failed ${response.statusCode}: ${url}`));
          return;
        }
        resolve(response);
      },
    );
    req.on("error", reject);
  });
}

async function download(url, destination) {
  const response = await request(url);
  await new Promise((resolve, reject) => {
    const file = createWriteStream(destination);
    response.pipe(file);
    file.on("finish", () => file.close(resolve));
    file.on("error", reject);
  });
}

function extractArchive(archivePath, destDir) {
  const isZip = archivePath.endsWith(".zip");
  if (isZip) {
    const result = spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-Command",
        `Expand-Archive -Path '${archivePath.replace(/'/g, "''")}' -DestinationPath '${destDir.replace(/'/g, "''")}' -Force`,
      ],
      { stdio: "inherit" },
    );
    if (result.status !== 0) {
      throw new Error(`Expand-Archive exited ${result.status ?? "unknown"}`);
    }
    return;
  }

  const result = spawnSync("tar", ["-xzf", archivePath, "-C", destDir], { stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`tar exited ${result.status}`);
  }
}

function locateExtractedBinary(stage) {
  const direct = path.join(stage, iiiBinaryName());
  if (existsSync(direct)) {
    return direct;
  }

  function walk(dir) {
    for (const entry of require("node:fs").readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        const found = walk(full);
        if (found) return found;
      } else if (entry.isFile() && entry.name === iiiBinaryName()) {
        return full;
      }
    }
    return null;
  }

  const candidate = walk(stage);
  if (!candidate) {
    throw new Error(`Could not locate ${iiiBinaryName()} in extracted archive`);
  }
  return candidate;
}

function codegraphPlatformTarget() {
  const arch = process.arch === "x64" ? "x64" : process.arch === "arm64" ? "arm64" : process.arch;
  return `${process.platform}-${arch}`;
}

function resolveCodegraphNodeProgram() {
  if (process.platform === "win32") {
    const flatNode = path.join(codegraphBundleDir, "node.exe");
    const flatEntry = path.join(codegraphBundleDir, "lib", "dist", "bin", "codegraph.js");
    if (existsSync(flatNode) && existsSync(flatEntry)) {
      return flatNode;
    }
    const pkgRoot = path.join(
      codegraphBundleDir,
      "node_modules",
      `@colbymchenry/codegraph-${codegraphPlatformTarget()}`,
    );
    const pkgNode = path.join(pkgRoot, "node.exe");
    if (existsSync(pkgNode)) {
      return pkgNode;
    }
    throw new Error(
      "codegraph Node is missing. Run `bun run codegraph:bundle` before agentmemory:bundle.",
    );
  }

  const flatLauncher = path.join(codegraphBundleDir, "bin", "codegraph");
  if (existsSync(flatLauncher)) {
    return flatLauncher;
  }
  const pkgRoot = path.join(
    codegraphBundleDir,
    "node_modules",
    `@colbymchenry/codegraph-${codegraphPlatformTarget()}`,
  );
  const pkgLauncher = path.join(pkgRoot, "bin", "codegraph");
  if (existsSync(pkgLauncher)) {
    return pkgLauncher;
  }

  throw new Error(
    "codegraph runtime is missing. Run `bun run codegraph:bundle` before agentmemory:bundle.",
  );
}

function ensureCodegraphNodeAvailable() {
  return resolveCodegraphNodeProgram();
}

function runNpmInstall() {
  const skipOptional = process.env.AGENTMEMORY_BUNDLE_SKIP_OPTIONAL === "1";
  const args = skipOptional
    ? ["install", "--omit=dev", "--omit=optional"]
    : ["install", "--omit=dev"];
  const result = spawnSync("npm", args, {
    cwd: bundleDir,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`npm install failed with exit code ${result.status ?? "unknown"}`);
  }
}

async function downloadIiiEngine(iiiVersion) {
  const asset = iiiAssetName();
  const base =
    process.env.AGENTMEMORY_DOWNLOAD_BASE?.trim() ||
    process.env.III_DOWNLOAD_BASE?.trim() ||
    `https://github.com/${III_REPO}/releases/download/iii%2Fv${iiiVersion}`;
  const url = `${base}/${asset}`;
  mkdirSync(cacheDir, { recursive: true });
  mkdirSync(path.join(bundleDir, "bin"), { recursive: true });
  const archivePath = path.join(cacheDir, asset);

  if (!existsSync(archivePath)) {
    console.log(`[locus] Downloading iii-engine v${iiiVersion} (${asset})...`);
    await download(url, archivePath);
  } else {
    console.log(`[locus] Using cached iii archive: ${path.relative(repoRoot, archivePath)}`);
  }

  const stage = path.join(cacheDir, `.extract-iii-${process.platform}-${process.arch}`);
  rmSync(stage, { recursive: true, force: true });
  mkdirSync(stage, { recursive: true });
  extractArchive(archivePath, stage);

  const extractedBinary = locateExtractedBinary(stage);
  const targetBinary = iiiBinaryPath();
  rmSync(targetBinary, { force: true });
  const copy = spawnSync(
    process.platform === "win32" ? "powershell" : "cp",
    process.platform === "win32"
      ? [
          "-NoProfile",
          "-Command",
          `Copy-Item -Path '${extractedBinary.replace(/'/g, "''")}' -Destination '${targetBinary.replace(/'/g, "''")}' -Force`,
        ]
      : [extractedBinary, targetBinary],
    { stdio: "inherit" },
  );
  if (copy.status !== 0) {
    throw new Error(`failed to copy ${iiiBinaryName()} into bundle dir`);
  }
  rmSync(stage, { recursive: true, force: true });

  return {
    sourceUrl: url,
    archiveSha256: createHash("sha256").update(readFileSync(archivePath)).digest("hex"),
  };
}

function verifyCli(nodeProgram) {
  const entry = cliEntryPath();
  if (!existsSync(entry)) {
    throw new Error(`agentmemory CLI entry missing: ${entry}`);
  }

  if (process.platform === "win32") {
    const result = spawnSync(nodeProgram, ["--liftoff-only", entry, "--help"], {
      encoding: "utf8",
      timeout: 30_000,
    });
    if (result.error) {
      throw result.error;
    }
    if (result.status !== 0) {
      throw new Error(
        result.stderr?.trim() || result.stdout?.trim() || `node cli.mjs --help exited ${result.status}`,
      );
    }
    return readPinnedVersion();
  }

  const result = spawnSync(nodeProgram, ["--help"], { encoding: "utf8", timeout: 30_000 });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      result.stderr?.trim() || result.stdout?.trim() || `codegraph launcher --help exited ${result.status}`,
    );
  }
  return readPinnedVersion();
}

async function main() {
  const agentmemoryVersion = readPinnedVersion();
  const iiiVersion = readIiiVersion();
  const nodeProgram = ensureCodegraphNodeAvailable();

  if (!isBundleReady()) {
    if (!existsSync(cliEntryPath())) {
      console.log(`[locus] Installing @agentmemory/agentmemory@${agentmemoryVersion}...`);
      runNpmInstall();
    }
    if (!existsSync(iiiBinaryPath())) {
      await downloadIiiEngine(iiiVersion);
    }
  }

  if (!isBundleReady()) {
    throw new Error("agentmemory bundle is incomplete after preparation");
  }

  const cliVersion = verifyCli(nodeProgram);
  patchAgentmemoryReplay();
  patchAgentmemoryViewer();
  writeManifest({ cliVersion, layout: "npm-plus-iii" });
  console.log(
    `[locus] Prepared agentmemory ${cliVersion} (iii ${iiiVersion}) at ${path.relative(repoRoot, bundleDir)}`,
  );
}

main().catch((error) => {
  console.error(
    `[locus] Failed to prepare agentmemory bundle: ${error.stack ?? error.message ?? error}`,
  );
  process.exit(1);
});
