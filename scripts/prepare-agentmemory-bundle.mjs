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
