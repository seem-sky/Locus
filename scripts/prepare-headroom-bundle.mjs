import { cpSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const HEADROOM_NPM_VERSION = "^0.1.0";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleDir = path.join(repoRoot, "src-tauri", "gen", "headroom-bundle");
const compressScript = path.join(scriptDir, "headroom-compress.mjs");
const compressOutput = path.join(bundleDir, "headroom-compress.mjs");
const headroomEntry = path.join(bundleDir, "node_modules", "headroom-ai", "dist", "index.js");

function isBundleReady() {
  return existsSync(compressOutput) && existsSync(headroomEntry);
}

function runInstall() {
  const bun = process.platform === "win32" ? "bun.exe" : "bun";
  const bunResult = spawnSync(bun, ["install", "--production"], {
    cwd: bundleDir,
    stdio: "inherit",
    env: process.env,
  });
  if (bunResult.status === 0) {
    return;
  }

  const npm = process.platform === "win32" ? "npm.cmd" : "npm";
  const npmResult = spawnSync(npm, ["install", "--omit=dev"], {
    cwd: bundleDir,
    stdio: "inherit",
    env: process.env,
  });
  if (npmResult.status !== 0) {
    process.exit(npmResult.status ?? 1);
  }
}

function main() {
  if (isBundleReady()) {
    console.log(`[locus] Headroom compress bundle already ready (${HEADROOM_NPM_VERSION})`);
    return;
  }

  mkdirSync(bundleDir, { recursive: true });
  writeFileSync(
    path.join(bundleDir, "package.json"),
    `${JSON.stringify(
      {
        name: "@locus/headroom-bundle",
        private: true,
        type: "module",
        dependencies: {
          "headroom-ai": HEADROOM_NPM_VERSION,
        },
      },
      null,
      2,
    )}\n`,
  );

  runInstall();
  cpSync(compressScript, compressOutput);
  console.log(`[locus] Headroom compress bundle ready at ${path.relative(repoRoot, bundleDir)}`);
}

main();
