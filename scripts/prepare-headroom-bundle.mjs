import { cpSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleDir = path.join(repoRoot, "src-tauri", "gen", "headroom-bundle");
const compressScript = path.join(scriptDir, "headroom-compress.mjs");

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

mkdirSync(bundleDir, { recursive: true });
writeFileSync(
  path.join(bundleDir, "package.json"),
  `${JSON.stringify(
    {
      name: "@locus/headroom-bundle",
      private: true,
      type: "module",
      dependencies: {
        "headroom-ai": "^0.1.0",
      },
    },
    null,
    2,
  )}\n`,
);

runInstall();
cpSync(compressScript, path.join(bundleDir, "headroom-compress.mjs"));
console.log(`Headroom bundle ready at ${bundleDir}`);
