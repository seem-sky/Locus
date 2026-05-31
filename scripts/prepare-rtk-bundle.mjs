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

const REPO = "rtk-ai/rtk";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleDir = path.join(repoRoot, "src-tauri", "gen", "rtk");
const cacheDir = path.join(repoRoot, ".cache", "rtk-bundle");
const versionFile = path.join(bundleDir, "version.txt");

function platformAsset() {
  const arch = process.arch === "x64" ? "x86_64" : process.arch === "arm64" ? "aarch64" : process.arch;
  if (process.platform === "win32") {
    return `rtk-${arch}-pc-windows-msvc.zip`;
  }
  if (process.platform === "darwin") {
    return `rtk-${arch}-apple-darwin.tar.gz`;
  }
  return `rtk-${arch}-unknown-linux-gnu.tar.gz`;
}

function binaryName() {
  return process.platform === "win32" ? "rtk.exe" : "rtk";
}

function readPinnedVersion() {
  return readFileSync(versionFile, "utf8").trim();
}

function isBundleReady() {
  return existsSync(path.join(bundleDir, binaryName()));
}

function writeManifest(entry) {
  writeFileSync(
    path.join(bundleDir, "manifest.json"),
    `${JSON.stringify(
      {
        version: 1,
        generatedAt: new Date().toISOString(),
        target: `${process.platform}-${process.arch}`,
        rtkVersion: readPinnedVersion(),
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
      { headers: { "User-Agent": "Locus rtk bundler" } },
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
  const direct = path.join(stage, binaryName());
  if (existsSync(direct)) {
    return direct;
  }

  function walk(dir) {
    for (const entry of require("node:fs").readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        const found = walk(full);
        if (found) return found;
      } else if (entry.isFile() && entry.name === binaryName()) {
        return full;
      }
    }
    return null;
  }

  const candidate = walk(stage);
  if (!candidate) {
    throw new Error(`Could not locate ${binaryName()} in extracted archive`);
  }
  return candidate;
}

async function downloadReleaseBundle(version) {
  const asset = platformAsset();
  const url =
    process.env.RTK_DOWNLOAD_BASE?.trim() ||
    `https://github.com/${REPO}/releases/download/v${version}/${asset}`;
  mkdirSync(cacheDir, { recursive: true });
  mkdirSync(bundleDir, { recursive: true });
  const archivePath = path.join(cacheDir, asset);

  if (!existsSync(archivePath)) {
    console.log(`[locus] Downloading RTK v${version} (${asset})...`);
    await download(url, archivePath);
  } else {
    console.log(`[locus] Using cached RTK archive: ${path.relative(repoRoot, archivePath)}`);
  }

  const stage = path.join(cacheDir, `.extract-${process.platform}-${process.arch}`);
  rmSync(stage, { recursive: true, force: true });
  mkdirSync(stage, { recursive: true });
  extractArchive(archivePath, stage);

  const extractedBinary = locateExtractedBinary(stage);
  const targetBinary = path.join(bundleDir, binaryName());
  rmSync(targetBinary, { force: true });
  const copy = spawnSync(
    process.platform === "win32" ? "powershell" : "cp",
    process.platform === "win32"
      ? ["-NoProfile", "-Command", `Copy-Item -Path '${extractedBinary.replace(/'/g, "''")}' -Destination '${targetBinary.replace(/'/g, "''")}' -Force`]
      : [extractedBinary, targetBinary],
    { stdio: "inherit" },
  );
  if (copy.status !== 0) {
    throw new Error(`failed to copy ${binaryName()} into bundle dir`);
  }
  rmSync(stage, { recursive: true, force: true });

  return {
    sourceUrl: url,
    archiveSha256: createHash("sha256").update(readFileSync(archivePath)).digest("hex"),
  };
}

function verifyCli() {
  const binary = path.join(bundleDir, binaryName());
  const result = spawnSync(binary, ["--version"], { encoding: "utf8" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status}`);
  }
  return result.stdout.trim();
}

async function main() {
  const version = readPinnedVersion();

  if (isBundleReady()) {
    const cliVersion = verifyCli();
    writeManifest({ cliVersion });
    console.log(`[locus] RTK bundle already ready (${cliVersion})`);
    return;
  }

  const manifestEntry = await downloadReleaseBundle(version);
  const cliVersion = verifyCli();
  writeManifest({ ...manifestEntry, cliVersion });
  console.log(`[locus] Prepared RTK ${cliVersion} at ${path.relative(repoRoot, bundleDir)}`);
}

main().catch((error) => {
  console.error(`[locus] Failed to prepare RTK bundle: ${error.stack ?? error.message ?? error}`);
  process.exit(1);
});
