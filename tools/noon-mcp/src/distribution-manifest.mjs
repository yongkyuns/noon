import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const moduleDir = path.dirname(fileURLToPath(import.meta.url));
const defaultPackageRoot = path.resolve(moduleDir, "..");
const defaultRepoRoot = path.resolve(defaultPackageRoot, "../..");

export const DISTRIBUTION_MANIFEST_SCHEMA_VERSION = 1;

const PACKAGE_SOURCES = Object.freeze([
  "package.json",
  "package-lock.json",
  "README.md",
  "preview.Dockerfile",
  "THIRD_PARTY_NOTICES.md",
  "bin/noon-preview.mjs",
  "src/discovery.mjs",
  "src/distribution-manifest.mjs",
  "src/preview-cli.mjs",
  "src/preview-isolation.mjs",
  "src/preview-pyodide.mjs",
  "src/preview-runner.mjs",
  "src/preview-seccomp.mjs",
  "src/preview-service.mjs",
  "src/preview-worker.mjs",
  "src/rendering-contract.mjs",
  "src/rendering-tools.mjs",
  "src/server.mjs",
  "scripts/clean-setup-smoke.mjs",
  "scripts/package-manifest.mjs",
  "scripts/setup-preview-runtime.mjs",
]);

const CHECKOUT_RUNNER_SOURCES = Object.freeze([
  "scripts/agent-preview-artifacts.mjs",
  "scripts/agent-preview-sessions.mjs",
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function regularFile(root, relative) {
  if (typeof relative !== "string" || relative.length === 0 || path.isAbsolute(relative) || relative.includes("\0")) {
    throw new TypeError(`invalid relative file path: ${String(relative)}`);
  }
  const rootReal = await realpath(root);
  const resolved = await realpath(path.join(rootReal, relative));
  const metadata = await stat(resolved);
  if (!resolved.startsWith(`${rootReal}${path.sep}`) || !metadata.isFile()) {
    throw new Error(`unconfined or non-file distribution input: ${relative}`);
  }
  return resolved;
}

async function fileSha256(root, relative) {
  return sha256(await readFile(await regularFile(root, relative)));
}

async function jsonFile(root, relative) {
  const value = JSON.parse(await readFile(await regularFile(root, relative), "utf8"));
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${relative} must contain a JSON object`);
  }
  return value;
}

async function command(executable, args, { cwd, timeout = 30_000, maxBuffer = 16 * 1024 * 1024 } = {}) {
  const { stdout } = await execFileAsync(executable, args, {
    cwd,
    timeout,
    maxBuffer,
    encoding: "utf8",
    env: {
      PATH: process.env.PATH ?? "",
      HOME: process.env.HOME ?? "",
      LANG: "C.UTF-8",
      LC_ALL: "C.UTF-8",
      PYTHONNOUSERSITE: "1",
      PYTHONDONTWRITEBYTECODE: "1",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_CONFIG_GLOBAL: process.platform === "win32" ? "NUL" : "/dev/null",
    },
  });
  return stdout.trim();
}

async function pythonIdentity(pythonExecutable, repoRoot) {
  const output = await command(pythonExecutable, [
    "-I", "-S", "-c",
    "import json,sys; print(json.dumps({'executable':sys.executable,'version':list(sys.version_info[:3])}))",
  ], { cwd: repoRoot, timeout: 10_000, maxBuffer: 64 * 1024 });
  const identity = JSON.parse(output);
  const version = identity?.version;
  if (!Array.isArray(version) || version.length !== 3 || version.some((part) => !Number.isInteger(part))) {
    throw new Error("Python returned an invalid version identity");
  }
  if (version[0] < 3 || (version[0] === 3 && version[1] < 12)) {
    throw new Error(`Noon agent tooling requires Python 3.12+, found ${version.join(".")}`);
  }
  return Object.freeze({ executable: identity.executable, version: version.join(".") });
}

async function repositoryIdentity(repoRoot) {
  try {
    const top = await command("git", ["-C", repoRoot, "rev-parse", "--show-toplevel"], { timeout: 5_000, maxBuffer: 64 * 1024 });
    if (await realpath(top) !== await realpath(repoRoot)) return { revision: null, dirty: null };
    const revision = await command("git", ["-C", repoRoot, "rev-parse", "HEAD"], { timeout: 5_000, maxBuffer: 64 * 1024 });
    const dirty = Boolean(await command("git", ["-C", repoRoot, "status", "--porcelain", "--untracked-files=normal"], {
      timeout: 5_000,
      maxBuffer: 1024 * 1024,
    }));
    return { revision, dirty };
  } catch {
    return { revision: null, dirty: null };
  }
}

function requiredMatch(text, expression, label) {
  const match = text.match(expression);
  if (!match?.[1]) throw new Error(`cannot derive ${label} from pinned runtime source`);
  return match[1];
}

function directDependencies(packageJson, lock) {
  const requested = { ...(packageJson.dependencies ?? {}), ...(packageJson.devDependencies ?? {}) };
  return Object.keys(requested).sort().map((name) => {
    const row = lock.packages?.[`node_modules/${name}`];
    if (!row || typeof row.version !== "string" || typeof row.integrity !== "string" || typeof row.license !== "string") {
      throw new Error(`lockfile lacks complete identity for direct dependency ${name}`);
    }
    return {
      name,
      requested: requested[name],
      version: row.version,
      integrity: row.integrity,
      license: row.license,
      developmentOnly: packageJson.devDependencies?.[name] !== undefined,
    };
  });
}

export async function buildDistributionManifest({
  packageRoot = defaultPackageRoot,
  repoRoot = process.env.NOON_REPO || defaultRepoRoot,
  pythonExecutable = process.env.NOON_PYTHON || "python3",
} = {}) {
  const packageReal = await realpath(packageRoot);
  const repoReal = await realpath(repoRoot);

  const packageJson = await jsonFile(packageReal, "package.json");
  const lock = await jsonFile(packageReal, "package-lock.json");
  if (packageJson.private !== true || packageJson.name !== lock.name || packageJson.version !== lock.version) {
    throw new Error("package and lockfile identity disagree or package is not checkout-bound/private");
  }
  if (lock.lockfileVersion !== 3) throw new Error(`unsupported npm lockfile version: ${lock.lockfileVersion}`);
  if (packageJson.engines?.node !== ">=22") throw new Error("package must declare Node >=22");

  const nodeMajor = Number(process.versions.node.split(".", 1)[0]);
  if (!Number.isInteger(nodeMajor) || nodeMajor < 22) {
    throw new Error(`Noon MCP packaging requires Node 22+, found ${process.versions.node}`);
  }
  const python = await pythonIdentity(pythonExecutable, repoReal);

  const capabilityText = await command(pythonExecutable, ["-B", path.join(repoReal, "scripts", "noon-capabilities.py")], {
    cwd: repoReal,
  });
  const capability = JSON.parse(capabilityText);
  if (capability?.kind !== "noon-agent-capabilities" || capability?.schema_version !== 1 || capability?.scope !== "source-inventory") {
    throw new Error("capability exporter returned an incompatible schema");
  }

  const skillPath = "skills/noon-authoring/SKILL.md";
  const skillText = await readFile(await regularFile(repoReal, skillPath), "utf8");
  const skillVersion = requiredMatch(skillText, /^\s{2}version:\s*["']([^"']+)["']\s*$/m, "skill version");

  const dockerfile = await readFile(await regularFile(packageReal, "preview.Dockerfile"), "utf8");
  const runtime = {
    playwrightVersion: requiredMatch(dockerfile, /playwright@([0-9]+\.[0-9]+\.[0-9]+)/, "Playwright version"),
    playwrightImageDigest: requiredMatch(dockerfile, /^FROM\s+mcr\.microsoft\.com\/playwright@(sha256:[0-9a-f]{64})\s*$/m, "Playwright image digest"),
    pyodideVersion: requiredMatch(dockerfile, /^ARG\s+PYODIDE_VERSION=([^\s]+)\s*$/m, "Pyodide version"),
    pyodideCoreSha256: requiredMatch(dockerfile, /^ARG\s+PYODIDE_CORE_SHA256=([0-9a-f]{64})\s*$/m, "Pyodide core SHA-256"),
    loadedBuildIdentity: null,
  };

  const sourceSha256 = {};
  for (const relative of PACKAGE_SOURCES) sourceSha256[`tools/noon-mcp/${relative}`] = await fileSha256(packageReal, relative);
  for (const relative of CHECKOUT_RUNNER_SOURCES) sourceSha256[relative] = await fileSha256(repoReal, relative);
  sourceSha256[skillPath] = await fileSha256(repoReal, skillPath);
  sourceSha256["scripts/noon-capabilities.py"] = await fileSha256(repoReal, "scripts/noon-capabilities.py");

  return Object.freeze({
    schemaVersion: DISTRIBUTION_MANIFEST_SCHEMA_VERSION,
    kind: "noon-agent-distribution",
    package: {
      name: packageJson.name,
      version: packageJson.version,
      private: true,
      lockfileVersion: lock.lockfileVersion,
      packageJsonSha256: await fileSha256(packageReal, "package.json"),
      packageLockSha256: await fileSha256(packageReal, "package-lock.json"),
      directDependencies: directDependencies(packageJson, lock),
    },
    environment: {
      node: { required: packageJson.engines.node, observed: process.versions.node },
      python: { required: ">=3.12", observed: python.version },
      preview: {
        dockerDaemonRequired: true,
        supportedPlatform: "Linux container runtime through Docker; host setup is POSIX-oriented",
        installCommand: "npm ci --ignore-scripts --no-audit --no-fund",
        implicitLifecycleHooks: false,
      },
      trustedCheckoutRequired: true,
    },
    repository: await repositoryIdentity(repoReal),
    skill: {
      name: "noon-authoring",
      version: skillVersion,
      path: skillPath,
      sha256: sourceSha256[skillPath],
    },
    capabilities: {
      schemaVersion: capability.schema_version,
      kind: capability.kind,
      scope: capability.scope,
      reference: capability.reference,
      provenance: capability.provenance,
      behavioralTestsRun: capability.qualification?.behavioral_tests_run === true,
      exporterSha256: sourceSha256["scripts/noon-capabilities.py"],
    },
    runner: {
      versions: runtime,
      sourceSha256,
    },
    notices: {
      path: "tools/noon-mcp/THIRD_PARTY_NOTICES.md",
      sha256: sourceSha256["tools/noon-mcp/THIRD_PARTY_NOTICES.md"],
    },
  });
}
