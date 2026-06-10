import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const HEADROOM_PROXY_VERSION = "0.5.23";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleDir = path.join(repoRoot, "src-tauri", "gen", "headroom-proxy-bundle");
const libDir = path.join(bundleDir, "lib");
const managedPythonExe = path.join(repoRoot, "src-tauri", "gen", "managed-python", "windows-x64", "python.exe");
const managedPipZipapp = path.join(repoRoot, "src-tauri", "gen", "managed-python", "pip.pyz");

function resolveBuildPython() {
  if (process.platform === "win32" && existsSync(managedPythonExe)) {
    return {
      python: managedPythonExe,
      pip: existsSync(managedPipZipapp) ? [managedPipZipapp] : null,
      pythonHome: path.dirname(managedPythonExe),
      label: "managed-python",
    };
  }

  for (const candidate of ["python3", "python", "py"]) {
    const probe = spawnSync(candidate, ["--version"], { encoding: "utf8" });
    if (probe.status === 0) {
      return {
        python: candidate,
        pip: null,
        pythonHome: null,
        label: candidate,
      };
    }
  }

  return null;
}

function runPython(python, pythonHome, args) {
  const env = { ...process.env };
  if (pythonHome) {
    env.PYTHONHOME = pythonHome;
    env.PYTHONPATH = "";
    env.PYTHONNOUSERSITE = "1";
  }
  const result = spawnSync(python, args, {
    cwd: bundleDir,
    stdio: "inherit",
    env,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${python} ${args.join(" ")} failed with exit code ${result.status ?? "unknown"}`);
  }
}

function verifyBundle(python, pythonHome, { quiet = false } = {}) {
  const env = { ...process.env, PYTHONPATH: libDir, PYTHONNOUSERSITE: "1" };
  if (pythonHome) {
    env.PYTHONHOME = pythonHome;
  }
  const importCheck = spawnSync(
    python,
    ["-c", "import headroom; from headroom.cli.main import main; print(headroom.__file__)"],
    { encoding: "utf8", env },
  );
  if (importCheck.status !== 0) {
    throw new Error(
      `headroom import check failed: ${importCheck.stderr || importCheck.stdout || "unknown"}`,
    );
  }
  if (!quiet) {
    console.log(`[locus] headroom proxy bundle verified: ${importCheck.stdout.trim()}`);
  }
}

function isBundleReady(buildPython) {
  const versionPath = path.join(bundleDir, "version.txt");
  const headroomInit = path.join(libDir, "headroom", "__init__.py");
  if (!existsSync(versionPath) || !existsSync(headroomInit)) {
    return false;
  }

  const version = readFileSync(versionPath, "utf8").trim();
  if (version !== HEADROOM_PROXY_VERSION) {
    return false;
  }

  try {
    verifyBundle(buildPython.python, buildPython.pythonHome, { quiet: true });
    return true;
  } catch {
    return false;
  }
}

function writeSkippedManifest(reason) {
  writeFileSync(
    path.join(bundleDir, "manifest.json"),
    `${JSON.stringify(
      {
        skipped: true,
        reason,
        headroomVersion: HEADROOM_PROXY_VERSION,
        generatedAt: new Date().toISOString(),
      },
      null,
      2,
    )}\n`,
  );
}

function main() {
  mkdirSync(bundleDir, { recursive: true });
  const buildPython = resolveBuildPython();
  if (!buildPython) {
    console.error(
      "[locus] headroom proxy bundle skipped: no Python found. On Windows run `bun run python:bundle` first, or install Python 3.10+.",
    );
    writeSkippedManifest("no-build-python");
    process.exit(0);
  }

  if (isBundleReady(buildPython)) {
    console.log(`[locus] Headroom proxy bundle already ready (${HEADROOM_PROXY_VERSION})`);
    return;
  }

  rmSync(libDir, { recursive: true, force: true });
  mkdirSync(libDir, { recursive: true });

  const pipArgs = buildPython.pip
    ? [...buildPython.pip, "install", "--target", libDir, `headroom-ai[proxy]==${HEADROOM_PROXY_VERSION}`, "--no-cache-dir"]
    : ["-m", "pip", "install", "--target", libDir, `headroom-ai[proxy]==${HEADROOM_PROXY_VERSION}`, "--no-cache-dir"];

  console.log(
    `[locus] Installing headroom-ai[proxy]==${HEADROOM_PROXY_VERSION} into ${path.relative(repoRoot, libDir)} via ${buildPython.label}...`,
  );
  runPython(buildPython.python, buildPython.pythonHome, pipArgs);
  verifyBundle(buildPython.python, buildPython.pythonHome);

  writeFileSync(path.join(bundleDir, "version.txt"), `${HEADROOM_PROXY_VERSION}\n`);
  writeFileSync(
    path.join(bundleDir, "manifest.json"),
    `${JSON.stringify(
      {
        headroomVersion: HEADROOM_PROXY_VERSION,
        generatedAt: new Date().toISOString(),
        buildPython: buildPython.label,
        lib: "lib",
        entryModule: "headroom.cli",
      },
      null,
      2,
    )}\n`,
  );

  console.log(`[locus] Headroom proxy bundle ready at ${path.relative(repoRoot, bundleDir)}`);
}

main();
