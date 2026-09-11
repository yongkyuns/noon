import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { buildDistributionManifest } from "../src/distribution-manifest.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, "..");
const repoRoot = path.resolve(here, "../../..");
const python = execFileSync("python3", ["-I", "-S", "-c", "import sys; print(sys.executable)"], { encoding: "utf8" }).trim();

function assertSha(value, label) {
  assert.match(value, /^[0-9a-f]{64}$/, `${label} must be a SHA-256 identity`);
}

test("distribution manifest pins package, skill, capability and runner identities deterministically", { timeout: 30_000 }, async () => {
  const options = { packageRoot, repoRoot, pythonExecutable: python };
  const first = await buildDistributionManifest(options);
  const second = await buildDistributionManifest(options);
  assert.deepEqual(second, first, "manifest must not contain timestamps or other nondeterministic fields");

  assert.equal(first.schemaVersion, 1);
  assert.equal(first.kind, "noon-agent-distribution");
  assert.equal(first.package.name, "@noon-animation/mcp");
  assert.equal(first.package.version, "0.1.0");
  assert.equal(first.package.private, true);
  assert.equal(first.package.lockfileVersion, 3);
  assertSha(first.package.packageJsonSha256, "package.json");
  assertSha(first.package.packageLockSha256, "package-lock.json");

  const dependencies = Object.fromEntries(first.package.directDependencies.map((row) => [row.name, row]));
  assert.equal(dependencies["@modelcontextprotocol/server"].version, "2.0.0");
  assert.equal(dependencies["@modelcontextprotocol/server"].license, "MIT");
  assert.equal(dependencies["@modelcontextprotocol/client"].version, "2.0.0");
  assert.equal(dependencies["@modelcontextprotocol/client"].developmentOnly, true);
  assert.equal(dependencies.zod.version, "4.2.0");

  assert.equal(first.environment.node.required, ">=22");
  assert.equal(first.environment.python.required, ">=3.12");
  assert.equal(first.environment.preview.dockerDaemonRequired, true);
  assert.equal(first.environment.preview.implicitLifecycleHooks, false);
  assert.equal(first.environment.preview.installCommand, "npm ci --ignore-scripts --no-audit --no-fund");
  assert.equal(first.environment.trustedCheckoutRequired, true);

  assert.equal(first.skill.name, "noon-authoring");
  assert.equal(first.skill.version, "0.2.0");
  assertSha(first.skill.sha256, "skill");

  assert.equal(first.capabilities.schemaVersion, 1);
  assert.equal(first.capabilities.kind, "noon-agent-capabilities");
  assert.equal(first.capabilities.scope, "source-inventory");
  assert.equal(first.capabilities.behavioralTestsRun, false);
  assert.equal(first.capabilities.reference.package, "manim");
  assertSha(first.capabilities.exporterSha256, "capability exporter");
  for (const [name, digest] of Object.entries(first.capabilities.provenance.input_sha256)) assertSha(digest, `capability input ${name}`);

  assert.equal(first.runner.versions.playwrightVersion, "1.62.1");
  assert.match(first.runner.versions.playwrightImageDigest, /^sha256:[0-9a-f]{64}$/);
  assert.equal(first.runner.versions.pyodideVersion, "314.0.5");
  assert.equal(first.runner.versions.pyodideCoreSha256, "f528dccea95fa8ec54295fd65bf86dd61183d11f0e52563dc8eadda45e0f78d6");
  assert.equal(first.runner.versions.loadedBuildIdentity, null,
    "source package manifest must not pretend a runtime build was loaded");
  for (const [name, digest] of Object.entries(first.runner.sourceSha256)) assertSha(digest, `runner source ${name}`);
  for (const required of [
    "tools/noon-mcp/src/distribution-manifest.mjs",
    "tools/noon-mcp/src/server.mjs",
    "scripts/agent-preview-sessions.mjs",
    "scripts/agent-preview-artifacts.mjs",
    "skills/noon-authoring/SKILL.md",
  ]) assertSha(first.runner.sourceSha256[required], required);
  assertSha(first.notices.sha256, "third-party notices");
});
