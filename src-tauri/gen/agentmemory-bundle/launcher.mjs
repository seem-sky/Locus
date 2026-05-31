import { spawn } from "node:child_process";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const bundleRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)));
const packageRoot = path.join(bundleRoot, "node_modules", "@agentmemory", "agentmemory");
const cliEntry = path.join(packageRoot, "dist", "cli.mjs");
const iiiBinDir = path.join(bundleRoot, "bin");
const arch = process.arch === "x64" ? "x64" : process.arch === "arm64" ? "arm64" : process.arch;
const nodeCandidates = [
  path.join(
    bundleRoot,
    "..",
    "codegraph-bundle",
    "node_modules",
    `@colbymchenry/codegraph-${process.platform}-${arch}`,
    "node.exe",
  ),
  path.join(
    bundleRoot,
    "node_modules",
    `@colbymchenry/codegraph-${process.platform}-${arch}`,
    "node.exe",
  ),
];

const nodeProgram = nodeCandidates.find((candidate) => {
  try {
    return fs.statSync(candidate).isFile();
  } catch {
    return false;
  }
});

if (!nodeProgram) {
  console.error("[agentmemory-launcher] bundled Node runtime not found");
  process.exit(1);
}

const env = { ...process.env };
const pathKey = process.platform === "win32" ? "Path" : "PATH";
env[pathKey] = `${iiiBinDir}${path.delimiter}${env[pathKey] ?? ""}`;

const logPath = process.env.AGENTMEMORY_SERVICE_LOG;
const stderr = logPath ? fs.openSync(logPath, "a") : "ignore";

const child = spawn(nodeProgram, ["--liftoff-only", cliEntry], {
  cwd: packageRoot,
  env,
  stdio: ["ignore", "ignore", stderr],
});

const shutdown = () => {
  if (!child.killed) {
    child.kill();
  }
};

process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);

child.on("error", (error) => {
  console.error(`[agentmemory-launcher] failed to start agentmemory: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.exit(1);
  }
  process.exit(code ?? 1);
});
