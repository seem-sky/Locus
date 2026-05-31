import { createHash } from "node:crypto";
import {
  cpSync,
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

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleDir = path.join(repoRoot, "src-tauri", "gen", "codegraph-bundle");
const cacheDir = path.join(repoRoot, ".cache", "codegraph-bundle");
const REPO = "colbymchenry/codegraph";

function platformTarget() {
  const arch = process.arch === "x64" ? "x64" : process.arch === "arm64" ? "arm64" : process.arch;
  return `${process.platform}-${arch}`;
}

function readBundleVersion() {
  const pkgPath = path.join(bundleDir, "package.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  return pkg.version;
}

function isBundleReady(root, target) {
  if (process.platform === "win32") {
    const flatNode = path.join(root, "node.exe");
    const flatEntry = path.join(root, "lib", "dist", "bin", "codegraph.js");
    if (existsSync(flatNode) && existsSync(flatEntry)) {
      return true;
    }
    const pkgRoot = path.join(root, "node_modules", `@colbymchenry/codegraph-${target}`);
    return (
      existsSync(path.join(pkgRoot, "node.exe")) &&
      existsSync(path.join(pkgRoot, "lib", "dist", "bin", "codegraph.js"))
    );
  }

  const flatLauncher = path.join(root, "bin", "codegraph");
  if (existsSync(flatLauncher)) {
    return true;
  }
  const pkgRoot = path.join(root, "node_modules", `@colbymchenry/codegraph-${target}`);
  return existsSync(path.join(pkgRoot, "bin", "codegraph"));
}

function writeManifest(entry) {
  writeFileSync(
    path.join(bundleDir, "manifest.json"),
    `${JSON.stringify(
      {
        version: 1,
        generatedAt: new Date().toISOString(),
        target: platformTarget(),
        codegraphVersion: readBundleVersion(),
        layout: entry?.layout ?? "unknown",
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
      { headers: { "User-Agent": "Locus codegraph bundler" } },
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
  const isWindows = process.platform === "win32";
  const args = isWindows
    ? ["-xf", archivePath, "-C", destDir, "--strip-components=1"]
    : ["-xzf", archivePath, "-C", destDir, "--strip-components=1"];
  const result = spawnSync("tar", args, { stdio: "inherit" });
  if (result.error) {
    throw new Error(`tar unavailable: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`tar exited ${result.status}`);
  }
}

function copyRuntimeIntoBundle(extractedDir) {
  const preserve = new Set(["package.json", "npm-shim.js", "README.md", "manifest.json"]);
  for (const name of ["node.exe", "lib", "bin"]) {
    const source = path.join(extractedDir, name);
    if (!existsSync(source)) {
      continue;
    }
    const target = path.join(bundleDir, name);
    rmSync(target, { recursive: true, force: true });
    cpSync(source, target, { recursive: true });
  }

  // Remove stale flat artifacts if release omitted them (non-Windows host building metadata only).
  if (process.platform !== "win32") {
    rmSync(path.join(bundleDir, "node.exe"), { force: true });
  }
}

function runNpmInstall() {
  const result = spawnSync("npm", ["install", "--omit=dev"], {
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

function verifyCli() {
  const target = platformTarget();
  if (!isBundleReady(bundleDir, target)) {
    throw new Error(`CodeGraph bundle is incomplete for ${target}`);
  }

  let command;
  let args;
  if (process.platform === "win32") {
    const flatNode = path.join(bundleDir, "node.exe");
    const flatEntry = path.join(bundleDir, "lib", "dist", "bin", "codegraph.js");
    if (existsSync(flatNode) && existsSync(flatEntry)) {
      command = flatNode;
      args = ["--liftoff-only", flatEntry, "--version"];
    } else {
      const pkgRoot = path.join(bundleDir, "node_modules", `@colbymchenry/codegraph-${target}`);
      command = path.join(pkgRoot, "node.exe");
      args = ["--liftoff-only", path.join(pkgRoot, "lib", "dist", "bin", "codegraph.js"), "--version"];
    }
  } else {
    const flatLauncher = path.join(bundleDir, "bin", "codegraph");
    if (existsSync(flatLauncher)) {
      command = flatLauncher;
      args = ["--version"];
    } else {
      const pkgRoot = path.join(bundleDir, "node_modules", `@colbymchenry/codegraph-${target}`);
      command = path.join(pkgRoot, "bin", "codegraph");
      args = ["--version"];
    }
  }

  const result = spawnSync(command, args, { encoding: "utf8" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      `codegraph --version failed: ${result.stderr?.trim() || result.stdout?.trim() || result.status}`,
    );
  }
  return result.stdout.trim();
}

async function downloadReleaseBundle(target, version) {
  const isWindows = process.platform === "win32";
  const asset = `codegraph-${target}.${isWindows ? "zip" : "tar.gz"}`;
  const url =
    process.env.CODEGRAPH_DOWNLOAD_BASE?.trim() ||
    `https://github.com/${REPO}/releases/download/v${version}/${asset}`;
  mkdirSync(cacheDir, { recursive: true });
  const archivePath = path.join(cacheDir, asset);

  if (!existsSync(archivePath)) {
    console.log(`[locus] Downloading CodeGraph v${version} (${asset})...`);
    await download(url, archivePath);
  } else {
    console.log(`[locus] Using cached CodeGraph archive: ${path.relative(repoRoot, archivePath)}`);
  }

  const stage = path.join(cacheDir, `.extract-${target}`);
  rmSync(stage, { recursive: true, force: true });
  mkdirSync(stage, { recursive: true });
  extractArchive(archivePath, stage);
  copyRuntimeIntoBundle(stage);
  rmSync(stage, { recursive: true, force: true });

  return {
    layout: "flat",
    sourceUrl: url,
    archiveSha256: createHash("sha256").update(readFileSync(archivePath)).digest("hex"),
  };
}

async function main() {
  const target = platformTarget();
  const version = readBundleVersion();

  if (isBundleReady(bundleDir, target)) {
    const cliVersion = verifyCli();
    writeManifest({ layout: existsSync(path.join(bundleDir, "node.exe")) ? "flat" : "npm", cliVersion });
    console.log(`[locus] CodeGraph bundle already ready (${cliVersion})`);
    return;
  }

  let manifestEntry = null;
  try {
    manifestEntry = await downloadReleaseBundle(target, version);
  } catch (error) {
    console.warn(
      `[locus] Release download failed (${error.message ?? error}); falling back to npm install...`,
    );
    runNpmInstall();
    manifestEntry = { layout: "npm" };
  }

  const cliVersion = verifyCli();
  writeManifest({ ...manifestEntry, cliVersion });
  console.log(`[locus] Prepared CodeGraph ${cliVersion} at ${path.relative(repoRoot, bundleDir)}`);
}

main().catch((error) => {
  console.error(`[locus] Failed to prepare CodeGraph bundle: ${error.stack ?? error.message ?? error}`);
  process.exit(1);
});
