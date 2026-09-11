#!/usr/bin/env node
import { constants as fsConstants } from "node:fs";
import {
  copyFile,
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
  writeFile,
} from "node:fs/promises";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const NOON_AGENT_PACKAGE_SCHEMA = 1;
export const NOON_AGENT_PACKAGE_KIND = "noon-agent-package";

const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(scriptPath), "..");
const skillRoot = "skills/noon-authoring";
const packagedSkillRoot = "skill/noon-authoring";
const manifestName = "manifest.json";
const capabilitiesName = "capabilities.json";
const generatorRelative = "scripts/package-noon-agent.mjs";
const runnerEntrypoints = Object.freeze([
  "tools/noon-mcp/bin/noon-preview.mjs",
  "tools/noon-mcp/src/server.mjs",
  "tools/noon-mcp/src/preview-worker.mjs",
  "tools/noon-mcp/scripts/setup-preview-runtime.mjs",
  "web/agent-preview-host.js",
]);
const runnerStaticFiles = Object.freeze([
  "tools/noon-mcp/package.json",
  "tools/noon-mcp/package-lock.json",
  "tools/noon-mcp/preview.Dockerfile",
  "web/agent-preview-host.html",
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function sortedObject(value) {
  if (Array.isArray(value)) return value.map(sortedObject);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, sortedObject(value[key])]));
  }
  return value;
}

function stableJson(value) {
  return `${JSON.stringify(sortedObject(value), null, 2)}\n`;
}

function portable(relative) {
  return relative.split(path.sep).join("/");
}

function validateRelative(relative) {
  if (typeof relative !== "string" || relative.length === 0 || relative.includes("\\") ||
      path.posix.isAbsolute(relative) || relative.split("/").includes("..")) {
    throw new Error(`unsafe package path: ${JSON.stringify(relative)}`);
  }
  return relative;
}

function plainRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value) &&
    [Object.prototype, null].includes(Object.getPrototypeOf(value));
}

function sortedKeys(record) {
  return Object.keys(record).sort();
}

function sameStrings(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

async function confinedRegularFile(root, relative) {
  validateRelative(relative);
  const rootReal = await realpath(root);
  const target = path.resolve(root, relative);
  const metadata = await lstat(target);
  if (metadata.isSymbolicLink() || !metadata.isFile()) {
    throw new Error(`package source must be a regular non-symlink file: ${relative}`);
  }
  const targetReal = await realpath(target);
  if (targetReal !== rootReal && !targetReal.startsWith(`${rootReal}${path.sep}`)) {
    throw new Error(`package source escapes root: ${relative}`);
  }
  return target;
}

async function walkRegularFiles(root, relativeDir) {
  validateRelative(relativeDir);
  const start = path.resolve(root, relativeDir);
  const metadata = await lstat(start);
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    throw new Error(`package source must be a regular directory: ${relativeDir}`);
  }
  const files = [];
  async function visit(directory, prefix) {
    const entries = (await readdir(directory, { withFileTypes: true }))
      .sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const relative = portable(path.join(prefix, entry.name));
      if (entry.isSymbolicLink()) throw new Error(`symbolic links are not packageable: ${relative}`);
      if (entry.isDirectory()) await visit(path.join(directory, entry.name), relative);
      else if (entry.isFile()) files.push(relative);
      else throw new Error(`unsupported package source entry: ${relative}`);
    }
  }
  await visit(start, relativeDir);
  return Object.freeze(files);
}

function nodeMajor() {
  return Number.parseInt(process.versions.node.split(".", 1)[0], 10);
}

function requireNode22() {
  if (!Number.isSafeInteger(nodeMajor()) || nodeMajor() < 22) {
    throw new Error(`Noon agent packaging requires Node >=22; found ${process.versions.node}`);
  }
}

function cleanSubprocessEnv() {
  const env = {
    PATH: process.env.PATH ?? "",
    LANG: "C",
    LC_ALL: "C",
    PYTHONUTF8: "1",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_TERMINAL_PROMPT: "0",
  };
  for (const name of ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"]) {
    if (process.env[name]) env[name] = process.env[name];
  }
  return env;
}

function runText(executable, args, { maxBuffer = 64 * 1024 * 1024 } = {}) {
  const result = spawnSync(executable, args, {
    cwd: repoRoot,
    env: cleanSubprocessEnv(),
    encoding: "utf8",
    maxBuffer,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = String(result.stderr ?? "").trim().slice(0, 2000);
    throw new Error(`${executable} failed (${result.status}): ${detail}`);
  }
  return String(result.stdout ?? "");
}

function pythonExecutable() {
  const configured = process.env.NOON_PYTHON;
  if (configured !== undefined) {
    if (configured.trim() === "" || configured.includes("\0")) throw new Error("NOON_PYTHON must be non-empty text");
    return configured;
  }
  return "python3";
}

function generateCapabilities() {
  const python = pythonExecutable();
  const version = runText(python, ["-S", "-B", "-c", "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')"])
    .trim().split(".").map((part) => Number.parseInt(part, 10));
  if (version.length !== 2 || version.some((value) => !Number.isSafeInteger(value)) ||
      version[0] < 3 || (version[0] === 3 && version[1] < 12)) {
    throw new Error("Noon agent packaging requires Python >=3.12");
  }
  const text = runText(python, ["-S", "-B", "scripts/noon-capabilities.py"]);
  let report;
  try { report = JSON.parse(text); }
  catch (error) { throw new Error(`capability exporter returned invalid JSON: ${error.message}`); }
  if (report?.schema_version !== 1 || report?.kind !== "noon-agent-capabilities" ||
      report?.scope !== "source-inventory" || !report?.provenance?.input_sha256) {
    throw new Error("capability exporter returned an unsupported source inventory");
  }
  return Object.freeze({ report, bytes: Buffer.from(stableJson(report), "utf8") });
}

function relativeImports(source) {
  const found = new Set();
  const patterns = [
    /(?:import|export)\s+(?:[^"'()]*?\s+from\s+)?["'](\.[^"']+)["']/g,
    /import\s*\(\s*["'](\.[^"']+)["']\s*\)/g,
  ];
  for (const pattern of patterns) {
    for (const match of source.matchAll(pattern)) found.add(match[1]);
  }
  return [...found].sort();
}

async function runnerSourceFiles() {
  const pending = [...runnerEntrypoints];
  const files = new Set(runnerStaticFiles);
  while (pending.length > 0) {
    const relative = portable(pending.pop());
    if (files.has(relative)) continue;
    const filename = await confinedRegularFile(repoRoot, relative);
    files.add(relative);
    if (!/\.(?:mjs|js)$/u.test(relative)) continue;
    const source = await readFile(filename, "utf8");
    for (const specifier of relativeImports(source)) {
      const resolved = path.resolve(path.dirname(filename), specifier);
      const rel = portable(path.relative(repoRoot, resolved));
      validateRelative(rel);
      if (!files.has(rel)) pending.push(rel);
    }
  }
  return Object.freeze([...files].sort());
}

async function hashCheckoutFiles(files) {
  const output = {};
  for (const relative of files) {
    const filename = await confinedRegularFile(repoRoot, relative);
    output[relative] = sha256(await readFile(filename));
  }
  return Object.freeze(output);
}

function mapDigest(files) {
  const lines = Object.entries(files).sort(([left], [right]) => left.localeCompare(right))
    .map(([name, digest]) => `${name}\0${digest}\n`);
  return sha256(Buffer.from(lines.join(""), "utf8"));
}

async function skillSourceHashes() {
  const hashes = {};
  for (const sourceRelative of await walkRegularFiles(repoRoot, skillRoot)) {
    const relativeWithinSkill = portable(path.relative(skillRoot, sourceRelative));
    const destinationRelative = `${packagedSkillRoot}/${relativeWithinSkill}`;
    hashes[destinationRelative] = sha256(await readFile(await confinedRegularFile(repoRoot, sourceRelative)));
  }
  return Object.freeze(hashes);
}

async function copySkill(outputRoot) {
  const expected = await skillSourceHashes();
  for (const [destinationRelative, digest] of Object.entries(expected)) {
    const relativeWithinSkill = destinationRelative.slice(`${packagedSkillRoot}/`.length);
    const sourceRelative = `${skillRoot}/${relativeWithinSkill}`;
    const source = await confinedRegularFile(repoRoot, sourceRelative);
    const destination = path.resolve(outputRoot, destinationRelative);
    await mkdir(path.dirname(destination), { recursive: true });
    await copyFile(source, destination, fsConstants.COPYFILE_EXCL);
    if (sha256(await readFile(destination)) !== digest) throw new Error(`copied skill hash mismatch: ${destinationRelative}`);
  }
  return expected;
}

async function currentRevision() {
  try {
    const revision = runText("git", ["-C", repoRoot, "rev-parse", "HEAD"], { maxBuffer: 64 * 1024 }).trim();
    return /^[0-9a-f]{40}$/u.test(revision) ? revision : null;
  } catch {
    return null;
  }
}

async function enumerateBundleFiles(bundleRoot) {
  const entries = [];
  async function visit(directory, prefix = "") {
    const children = (await readdir(directory, { withFileTypes: true }))
      .sort((left, right) => left.name.localeCompare(right.name));
    for (const child of children) {
      const relative = prefix ? `${prefix}/${child.name}` : child.name;
      if (child.isSymbolicLink()) throw new Error(`bundle contains symbolic link: ${relative}`);
      if (child.isDirectory()) await visit(path.join(directory, child.name), relative);
      else if (child.isFile()) entries.push(relative);
      else throw new Error(`bundle contains unsupported entry: ${relative}`);
    }
  }
  await visit(bundleRoot);
  return Object.freeze(entries);
}

async function readBundleJson(bundleRoot, relative) {
  const filename = await confinedRegularFile(bundleRoot, relative);
  try { return JSON.parse(await readFile(filename, "utf8")); }
  catch (error) { throw new Error(`${relative} is invalid JSON: ${error.message}`); }
}

export async function buildAgentBundle({ outputDir }) {
  requireNode22();
  if (typeof outputDir !== "string" || outputDir.trim() === "" || outputDir.includes("\0")) {
    throw new TypeError("outputDir must be a non-empty path without NUL");
  }
  const outputRoot = path.resolve(outputDir);
  await mkdir(outputRoot, { recursive: false });

  const capabilities = generateCapabilities();
  const skillFiles = await copySkill(outputRoot);
  const capabilitiesPath = path.join(outputRoot, capabilitiesName);
  await writeFile(capabilitiesPath, capabilities.bytes, { flag: "wx" });

  const sourceFiles = await runnerSourceFiles();
  const runnerHashes = await hashCheckoutFiles(sourceFiles);
  const packageJsonBytes = await readFile(await confinedRegularFile(repoRoot, "tools/noon-mcp/package.json"));
  const packageLockBytes = await readFile(await confinedRegularFile(repoRoot, "tools/noon-mcp/package-lock.json"));
  const packageJson = JSON.parse(packageJsonBytes.toString("utf8"));
  if (typeof packageJson.name !== "string" || typeof packageJson.version !== "string") {
    throw new Error("MCP package metadata is incomplete");
  }

  const payloadFiles = Object.freeze({
    [capabilitiesName]: sha256(capabilities.bytes),
    ...skillFiles,
  });
  const generatorSha256 = sha256(await readFile(await confinedRegularFile(repoRoot, generatorRelative)));
  const manifest = {
    schema: NOON_AGENT_PACKAGE_SCHEMA,
    kind: NOON_AGENT_PACKAGE_KIND,
    source: {
      revision: capabilities.report.provenance.revision ?? null,
      dirty: capabilities.report.provenance.dirty ?? null,
    },
    capabilities: {
      path: capabilitiesName,
      schemaVersion: capabilities.report.schema_version,
      sha256: payloadFiles[capabilitiesName],
    },
    skill: {
      root: packagedSkillRoot,
      files: skillFiles,
    },
    runner: {
      package: packageJson.name,
      version: packageJson.version,
      packageJsonSha256: sha256(packageJsonBytes),
      packageLockSha256: sha256(packageLockBytes),
      sourceSha256: runnerHashes,
      sourceSetSha256: mapDigest(runnerHashes),
      loadedBuildIdentity: "runtime-observed-per-artifact",
    },
    generator: {
      path: generatorRelative,
      sha256: generatorSha256,
    },
    environment: {
      node: ">=22",
      python: ">=3.12",
      platform: "POSIX",
      docker: "required-for-isolated-rendering",
      buildBrowserPackage: "bash scripts/build-web-demo.sh",
      installMcpDependencies: "cd tools/noon-mcp && npm ci --ignore-scripts --no-audit --no-fund",
      preparePreviewRuntime: "cd tools/noon-mcp && node scripts/setup-preview-runtime.mjs",
    },
    payload: {
      files: payloadFiles,
      sha256: mapDigest(payloadFiles),
    },
  };
  await writeFile(path.join(outputRoot, manifestName), stableJson(manifest), { flag: "wx" });
  return Object.freeze({ outputDir: outputRoot, manifest: Object.freeze(manifest) });
}

export async function verifyAgentBundle({ bundleDir }) {
  requireNode22();
  if (typeof bundleDir !== "string" || bundleDir.trim() === "" || bundleDir.includes("\0")) {
    throw new TypeError("bundleDir must be a non-empty path without NUL");
  }
  const requestedRoot = path.resolve(bundleDir);
  const requestedMetadata = await lstat(requestedRoot);
  if (requestedMetadata.isSymbolicLink() || !requestedMetadata.isDirectory()) {
    throw new Error("bundleDir must be a regular non-symlink directory");
  }
  const bundleRoot = await realpath(requestedRoot);
  const manifest = await readBundleJson(bundleRoot, manifestName);
  if (manifest?.schema !== NOON_AGENT_PACKAGE_SCHEMA || manifest?.kind !== NOON_AGENT_PACKAGE_KIND) {
    throw new Error("unsupported Noon agent package manifest");
  }
  if (!plainRecord(manifest.payload?.files)) throw new Error("manifest payload file map is missing");

  const actualFiles = (await enumerateBundleFiles(bundleRoot)).filter((name) => name !== manifestName);
  const payloadFiles = sortedKeys(manifest.payload.files);
  if (!sameStrings(actualFiles, payloadFiles)) throw new Error("bundle payload file set does not match manifest");
  const actualPayload = {};
  for (const relative of payloadFiles) {
    const bytes = await readFile(await confinedRegularFile(bundleRoot, relative));
    const digest = sha256(bytes);
    if (digest !== manifest.payload.files[relative]) throw new Error(`bundle payload hash mismatch: ${relative}`);
    actualPayload[relative] = digest;
  }
  if (mapDigest(actualPayload) !== manifest.payload.sha256) throw new Error("bundle payload digest mismatch");

  if (manifest.capabilities?.path !== capabilitiesName) throw new Error("capability package path mismatch");
  const bundledCapabilitiesBytes = await readFile(await confinedRegularFile(bundleRoot, capabilitiesName));
  const capabilities = await readBundleJson(bundleRoot, capabilitiesName);
  if (capabilities?.schema_version !== manifest.capabilities?.schemaVersion ||
      capabilities?.kind !== "noon-agent-capabilities" || capabilities?.scope !== "source-inventory") {
    throw new Error("bundled capability inventory does not match manifest");
  }
  if (manifest.capabilities.sha256 !== manifest.payload.files[capabilitiesName] ||
      manifest.capabilities.sha256 !== sha256(bundledCapabilitiesBytes)) {
    throw new Error("capability hash is not bound to bundle payload");
  }
  const currentCapabilities = generateCapabilities();
  if (sha256(currentCapabilities.bytes) !== manifest.capabilities.sha256) {
    throw new Error("bundled capability inventory does not match current canonical inventory");
  }
  if ((capabilities.provenance?.revision ?? null) !== (manifest.source?.revision ?? null) ||
      (capabilities.provenance?.dirty ?? null) !== (manifest.source?.dirty ?? null)) {
    throw new Error("bundle source identity does not match capability provenance");
  }

  if (manifest.skill?.root !== packagedSkillRoot || !plainRecord(manifest.skill?.files)) {
    throw new Error("skill package root or file map is invalid");
  }
  const currentSkillHashes = await skillSourceHashes();
  const skillFiles = sortedKeys(manifest.skill.files);
  if (!sameStrings(skillFiles, sortedKeys(currentSkillHashes))) {
    throw new Error("skill package file set does not match current skill tree");
  }
  for (const relative of skillFiles) {
    if (manifest.skill.files[relative] !== currentSkillHashes[relative] ||
        manifest.payload.files[relative] !== currentSkillHashes[relative]) {
      throw new Error(`skill package hash mismatch: ${relative}`);
    }
  }

  if (manifest.generator?.path !== generatorRelative) throw new Error("package generator path mismatch");
  const generatorBytes = await readFile(await confinedRegularFile(repoRoot, generatorRelative));
  if (sha256(generatorBytes) !== manifest.generator?.sha256) throw new Error("package generator source hash mismatch");

  if (!plainRecord(manifest.runner?.sourceSha256)) throw new Error("runner source hash map is missing");
  const expectedRunnerFiles = await runnerSourceFiles();
  const runnerFiles = sortedKeys(manifest.runner.sourceSha256);
  if (!sameStrings(runnerFiles, [...expectedRunnerFiles])) {
    throw new Error("runner source file set does not match current entrypoints");
  }
  const currentRunnerHashes = await hashCheckoutFiles(expectedRunnerFiles);
  for (const relative of expectedRunnerFiles) {
    if (currentRunnerHashes[relative] !== manifest.runner.sourceSha256[relative]) {
      throw new Error(`runner source hash mismatch: ${relative}`);
    }
  }
  if (mapDigest(currentRunnerHashes) !== manifest.runner.sourceSetSha256) throw new Error("runner source-set digest mismatch");

  const packageJsonBytes = await readFile(await confinedRegularFile(repoRoot, "tools/noon-mcp/package.json"));
  const packageLockBytes = await readFile(await confinedRegularFile(repoRoot, "tools/noon-mcp/package-lock.json"));
  const packageJson = JSON.parse(packageJsonBytes.toString("utf8"));
  if (packageJson.name !== manifest.runner.package || packageJson.version !== manifest.runner.version ||
      sha256(packageJsonBytes) !== manifest.runner.packageJsonSha256 ||
      sha256(packageLockBytes) !== manifest.runner.packageLockSha256) {
    throw new Error("runner package identity mismatch");
  }
  if (manifest.runner.loadedBuildIdentity !== "runtime-observed-per-artifact") {
    throw new Error("package must not substitute source metadata for loaded runtime build identity");
  }

  const revision = await currentRevision();
  if (manifest.source?.revision !== null && revision !== manifest.source.revision) {
    throw new Error("bundle Git revision does not match current checkout");
  }
  return Object.freeze({ ok: true, bundleDir: bundleRoot, payloadSha256: manifest.payload.sha256,
    runnerSourceSetSha256: manifest.runner.sourceSetSha256, revision });
}

export function parsePackageArgs(argv) {
  if (!Array.isArray(argv) || argv.some((value) => typeof value !== "string")) throw new TypeError("arguments must be strings");
  let mode = null;
  let target = null;
  let help = false;
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") { help = true; continue; }
    if (arg !== "--output" && arg !== "--verify") throw new TypeError(`unknown argument: ${arg}`);
    if (mode !== null) throw new TypeError("choose exactly one of --output or --verify");
    index += 1;
    if (index >= argv.length || argv[index].trim() === "") throw new TypeError(`${arg} requires a path`);
    mode = arg === "--output" ? "output" : "verify";
    target = argv[index];
  }
  if (!help && mode === null) throw new TypeError("choose exactly one of --output or --verify");
  return Object.freeze({ mode, target, help });
}

const usage = `Usage:\n  node scripts/package-noon-agent.mjs --output <new-directory>\n  node scripts/package-noon-agent.mjs --verify <bundle-directory>\n\nBuilds or verifies a deterministic checkout-bound Noon authoring skill/capability package. Actual loaded worker/WASM identity remains runtime-observed in preview artifacts.\n`;

async function main() {
  const args = parsePackageArgs(process.argv.slice(2));
  if (args.help) { process.stdout.write(usage); return; }
  if (args.mode === "output") {
    const result = await buildAgentBundle({ outputDir: args.target });
    process.stdout.write(`${JSON.stringify({ ok: true, outputDir: result.outputDir, payloadSha256: result.manifest.payload.sha256 })}\n`);
  } else {
    const result = await verifyAgentBundle({ bundleDir: args.target });
    process.stdout.write(`${JSON.stringify(result)}\n`);
  }
}

if (path.resolve(process.argv[1] ?? "") === path.resolve(scriptPath)) {
  main().catch((error) => {
    process.stderr.write(`${String(error?.stack ?? error)}\n`);
    process.exitCode = 1;
  });
}
