import { existsSync, mkdirSync } from "node:fs";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleDir = path.join(repoRoot, "src-tauri", "gen", "agentmemory-bundle");
const codegraphBundleDir = path.join(repoRoot, "src-tauri", "gen", "codegraph-bundle");
const exportRoot = path.join(repoRoot, ".cache", "agentmemory-integration-test");

function platformTarget() {
  const arch = process.arch === "x64" ? "x64" : process.arch === "arm64" ? "arm64" : process.arch;
  return `${process.platform}-${arch}`;
}

function resolveCodegraphNode() {
  if (process.platform === "win32") {
    const flatNode = path.join(codegraphBundleDir, "node.exe");
    if (existsSync(flatNode)) return flatNode;
    const pkgNode = path.join(
      codegraphBundleDir,
      "node_modules",
      `@colbymchenry/codegraph-${platformTarget()}`,
      "node.exe",
    );
    if (existsSync(pkgNode)) return pkgNode;
    throw new Error("codegraph Node missing — run `bun run codegraph:bundle`");
  }
  const flatLauncher = path.join(codegraphBundleDir, "bin", "codegraph");
  if (existsSync(flatLauncher)) return flatLauncher;
  throw new Error("codegraph runtime missing — run `bun run codegraph:bundle`");
}

function paths() {
  const packageRoot = path.join(bundleDir, "node_modules", "@agentmemory", "agentmemory");
  const cliEntry = path.join(packageRoot, "dist", "cli.mjs");
  const iiiBinDir = path.join(bundleDir, "bin");
  const iiiName = process.platform === "win32" ? "iii.exe" : "iii";
  const iiiProgram = path.join(iiiBinDir, iiiName);
  return { packageRoot, cliEntry, iiiBinDir, iiiProgram };
}

async function waitForHealth(timeoutMs = 90_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch("http://127.0.0.1:3111/agentmemory/health");
      if (response.ok) {
        return response.json();
      }
    } catch {
      // keep polling
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error("agentmemory health endpoint did not become ready in time");
}

function prependPath(binDir) {
  const sep = process.platform === "win32" ? ";" : ":";
  return `${binDir}${sep}${process.env.PATH ?? ""}`;
}

async function main() {
  const { packageRoot, cliEntry, iiiBinDir, iiiProgram } = paths();
  const nodeProgram = resolveCodegraphNode();

  const checks = [
    ["bundle cli.mjs", cliEntry],
    ["iii binary", iiiProgram],
    ["iii-config.yaml", path.join(packageRoot, "iii-config.yaml")],
    ["codegraph node", nodeProgram],
  ];
  for (const [label, target] of checks) {
    if (!existsSync(target)) {
      throw new Error(`${label} missing at ${target}`);
    }
    console.log(`[ok] ${label}`);
  }

  mkdirSync(exportRoot, { recursive: true });

  const args =
    process.platform === "win32"
      ? ["--liftoff-only", cliEntry]
      : ["--liftoff-only", cliEntry];

  const child = spawn(nodeProgram, args, {
    cwd: packageRoot,
    env: {
      ...process.env,
      PATH: prependPath(iiiBinDir),
      AGENTMEMORY_EXPORT_ROOT: exportRoot,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });

  let stderr = "";
  child.stderr?.on("data", (chunk) => {
    stderr += chunk.toString();
  });

  const cleanup = () => {
    if (!child.killed) {
      if (process.platform === "win32") {
        try {
          spawn("taskkill", ["/F", "/T", "/PID", String(child.pid)], { stdio: "ignore" });
        } catch {
          child.kill();
        }
      } else {
        child.kill("SIGTERM");
      }
    }
  };
  process.on("exit", cleanup);
  process.on("SIGINT", () => {
    cleanup();
    process.exit(130);
  });

  try {
    console.log("[..] starting bundled agentmemory...");
    const health = await waitForHealth();
    console.log("[ok] health:", JSON.stringify(health));

    const rememberResponse = await fetch("http://127.0.0.1:3111/agentmemory/remember", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        content: "Locus integration smoke test",
        type: "fact",
        concepts: ["locus-integration-test"],
        project: "locus-smoke",
        agentId: "locus-test",
      }),
    });
    if (!rememberResponse.ok) {
      const body = await rememberResponse.text();
      throw new Error(`remember failed ${rememberResponse.status}: ${body}`);
    }
    const remembered = await rememberResponse.json();
    console.log("[ok] remember:", remembered.memory?.id ?? remembered.id ?? "saved");

    const searchResponse = await fetch("http://127.0.0.1:3111/agentmemory/search", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        query: "Locus integration smoke test",
        project: "locus-smoke",
        limit: 3,
      }),
    });
    if (!searchResponse.ok) {
      const body = await searchResponse.text();
      throw new Error(`search failed ${searchResponse.status}: ${body}`);
    }
    const search = await searchResponse.json();
    const hitCount = Array.isArray(search.results) ? search.results.length : 0;
    console.log(`[ok] search returned ${hitCount} result(s)`);

    console.log("\nagentmemory integration smoke test PASSED");
  } catch (error) {
    if (stderr.trim()) {
      console.error("\n[stderr]\n" + stderr.trim());
    }
    throw error;
  } finally {
    cleanup();
  }
}

main().catch((error) => {
  console.error(`\nagentmemory integration smoke test FAILED: ${error.message ?? error}`);
  process.exit(1);
});
