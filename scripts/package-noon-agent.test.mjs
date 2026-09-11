import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  NOON_AGENT_PACKAGE_KIND,
  NOON_AGENT_PACKAGE_SCHEMA,
  buildAgentBundle,
  parsePackageArgs,
  verifyAgentBundle,
} from "./package-noon-agent.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");

async function fileBytes(root, relative) {
  return await readFile(path.join(root, relative));
}

test("argument parser requires one explicit package mode", () => {
  assert.deepEqual(parsePackageArgs(["--output", "out"]), { mode: "output", target: "out", help: false });
  assert.deepEqual(parsePackageArgs(["--verify", "bundle"]), { mode: "verify", target: "bundle", help: false });
  assert.equal(parsePackageArgs(["--help"]).help, true);
  assert.throws(() => parsePackageArgs([]), /exactly one/);
  assert.throws(() => parsePackageArgs(["--output", "a", "--verify", "b"]), /exactly one/);
  assert.throws(() => parsePackageArgs(["--unknown"]), /unknown argument/);
});

test("package is deterministic, checkout-bound and independently verifiable", { timeout: 30_000 }, async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-agent-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const first = path.join(root, "first");
  const second = path.join(root, "second");

  const a = await buildAgentBundle({ outputDir: first });
  const b = await buildAgentBundle({ outputDir: second });
  assert.equal(a.manifest.schema, NOON_AGENT_PACKAGE_SCHEMA);
  assert.equal(a.manifest.kind, NOON_AGENT_PACKAGE_KIND);
  assert.deepEqual(a.manifest, b.manifest);
  assert.equal(a.manifest.runner.package, "@noon-animation/mcp");
  assert.match(a.manifest.runner.version, /^\d+\.\d+\.\d+$/);
  assert.equal(a.manifest.runner.loadedBuildIdentity, "runtime-observed-per-artifact");
  assert.equal(a.manifest.environment.node, ">=22");
  assert.equal(a.manifest.environment.python, ">=3.12");
  assert.ok(Object.keys(a.manifest.runner.sourceSha256).some((name) => name === "tools/noon-mcp/src/server.mjs"));
  assert.ok(Object.keys(a.manifest.runner.sourceSha256).some((name) => name === "web/agent-preview-host.js"));
  assert.ok(Object.keys(a.manifest.runner.sourceSha256).some((name) => name === "scripts/agent-preview-sessions.mjs"));

  for (const relative of Object.keys(a.manifest.payload.files)) {
    assert.deepEqual(await fileBytes(first, relative), await fileBytes(second, relative), relative);
  }
  assert.deepEqual(await fileBytes(first, "manifest.json"), await fileBytes(second, "manifest.json"));
  const capabilities = JSON.parse(await readFile(path.join(first, "capabilities.json"), "utf8"));
  assert.equal(capabilities.kind, "noon-agent-capabilities");
  assert.equal(capabilities.scope, "source-inventory");
  assert.equal(capabilities.qualification.behavioral_tests_run, false);
  assert.equal(capabilities.runtime.host, null);
  assert.equal(capabilities.runtime.renderer_backend, null);

  const verified = await verifyAgentBundle({ bundleDir: first });
  assert.equal(verified.ok, true);
  assert.equal(verified.payloadSha256, a.manifest.payload.sha256);
  assert.equal(verified.runnerSourceSetSha256, a.manifest.runner.sourceSetSha256);

  await assert.rejects(buildAgentBundle({ outputDir: first }), /exist|EEXIST/i,
    "package builder must never overwrite an existing bundle");
});

test("verification rejects modified payload and symlink substitution", { timeout: 30_000 }, async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-agent-package-tamper-"));
  t.after(() => rm(root, { recursive: true, force: true }));

  const modified = path.join(root, "modified");
  await buildAgentBundle({ outputDir: modified });
  const skill = path.join(modified, "skill/noon-authoring/SKILL.md");
  await writeFile(skill, `${await readFile(skill, "utf8")}\n# tampered\n`);
  await assert.rejects(verifyAgentBundle({ bundleDir: modified }), /hash mismatch/);

  const linked = path.join(root, "linked");
  await buildAgentBundle({ outputDir: linked });
  const linkedSkill = path.join(linked, "skill/noon-authoring/SKILL.md");
  await rm(linkedSkill);
  await symlink(path.join(repoRoot, "skills/noon-authoring/SKILL.md"), linkedSkill);
  await assert.rejects(verifyAgentBundle({ bundleDir: linked }), /symbolic link|non-symlink/);
});
