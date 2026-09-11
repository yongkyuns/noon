import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, realpath } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { loadEvaluationCorpus } from "../eval/corpus.mjs";
import { createDiscovery } from "../src/discovery.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, "..");
const repoRoot = path.resolve(packageRoot, "../..");
const python = execFileSync("python3", ["-I", "-S", "-c", "import sys; print(sys.executable)"], { encoding: "utf8" }).trim();
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

function requireFeatures(record, required, label) {
  const features = new Set(record.features ?? []);
  for (const feature of required) assert.ok(features.has(feature), `${label} lost required feature ${feature}`);
}

test("fixed agent evaluation corpus is grounded in the maintained capability/example inventory", { timeout: 30_000 }, async (t) => {
  const corpus = await loadEvaluationCorpus();
  assert.equal(corpus.reference.compatibility, "Manim Community v0.21.0");
  assert.equal(corpus.reference.renderBackend, "WebGL2");

  const discovery = await createDiscovery({ repoRoot, pythonExecutable: python });
  t.after(() => discovery.close());
  const examples = corpus.tasks.filter((task) => task.kind !== "cancellation").map((task) => task.example);
  const report = await discovery.capabilities({ examples });
  assert.equal(report.qualification.behavioral_tests_run, false,
    "source inventory must not be mislabeled as deterministic render qualification");

  for (const task of corpus.tasks) {
    if (task.kind === "cancellation") {
      const evalRoot = await realpath(path.join(packageRoot, "eval"));
      const sourcePath = await realpath(path.join(packageRoot, task.sourcePath));
      assert.ok(sourcePath.startsWith(`${evalRoot}${path.sep}`), `${task.id} escaped the eval fixture root`);
      const source = await readFile(sourcePath);
      assert.match(source.toString("utf8"), /while True:/, `${task.id} must remain the intentional stuck-source fixture`);
      assert.match(hash(source), /^[0-9a-f]{64}$/);
      continue;
    }

    const record = report.examples[task.example];
    assert.ok(record, `${task.id} missing example ${task.example}`);
    requireFeatures(record, task.requiredFeatures, task.id);
    assert.equal(record.runtime_verified, false);

    if (task.kind === "render") {
      assert.equal(record.status, "ready", `${task.id} render source is no longer ready`);
      assert.match(record.source_sha256, /^[0-9a-f]{64}$/);
      const reference = await discovery.reference({ example: task.example });
      assert.equal(reference.example.source_sha256, record.source_sha256);
      assert.equal(reference.behavioral_tests_run, false);
      assert.match(reference.source, /from noon import \*/);
    } else {
      assert.equal(record.status, task.expectedStatus, `${task.id} support classification changed`);
      assert.equal(record.repository_path, undefined, `${task.id} must not masquerade as a ready source`);
      await assert.rejects(discovery.reference({ example: task.example }), /not ready/);
    }
  }
});
