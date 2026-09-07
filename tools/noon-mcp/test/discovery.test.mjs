import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, writeFile, rm, symlink } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { createDiscovery } from "../src/discovery.mjs";

export const python = execFileSync("python3", ["-c", "import sys; print(sys.executable)"], { encoding: "utf8" }).trim();
const sha = (text) => createHash("sha256").update(text).digest("hex");
async function fixture(t, options = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-discovery-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(path.join(root, "scripts"));
  await mkdir(path.join(root, "web/python/examples"), { recursive: true });
  const source = "raise RuntimeError('example source must not be executed')\n";
  const sourcePath = "web/python/examples/circle.py";
  await writeFile(path.join(root, sourcePath), source);
  const report = { schema_version: 1, kind: "noon-agent-capabilities", scope: "source-inventory",
    qualification: { behavioral_tests_run: false }, provenance: { revision: null },
    symbols: { Circle: { exported: true, runtime_verified: false } },
    examples: { circle: { status: "ready", repository_path: sourcePath, source_sha256: sha(source) } } };
  await writeFile(path.join(root, "report.json"), JSON.stringify(report));
  await writeFile(path.join(root, "scripts/noon-capabilities.py"),
    "import json, pathlib, os, sys\n" +
    "assert 'NOON_TEST_SECRET' not in os.environ\n" +
    "assert 'PYTHONPATH' not in os.environ\n" +
    "print(pathlib.Path(__file__).resolve().parents[1].joinpath('report.json').read_text())\n");
  const service = await createDiscovery({ repoRoot: root, pythonExecutable: python, ...options });
  t.after(() => service.close());
  return { root, service, report, source, sourcePath,
    save: () => writeFile(path.join(root, "report.json"), JSON.stringify(report)) };
}

test("source-only discovery is read-only and reference preserves exact source", async (t) => {
  const { service, source } = await fixture(t);
  assert.equal((await service.capabilities()).scope, "source-inventory");
  const result = await service.reference({ example: "circle" });
  assert.equal(result.source, source);
  assert.equal(result.behavioral_tests_run, false);
});
test("invalid model-supplied identifiers cannot become flags or paths", async (t) => {
  const { service } = await fixture(t);
  for (const symbols of [["--help"], ["../../file"], ["Circle;exit"], [1], "Circle", Array(33).fill("Circle")]) {
    await assert.rejects(service.capabilities({ symbols }), /invalid/);
  }
  await assert.rejects(service.reference({ example: "../secret" }), /invalid/);
  await assert.rejects(service.reference({ example: "missing" }), /not ready/);
});
test("malformed or runtime-qualified inventory is not accepted as offline evidence", async (t) => {
  const f = await fixture(t);
  f.report.qualification.behavioral_tests_run = true; await f.save();
  await assert.rejects(f.service.capabilities(), /incompatible/);
  f.report.qualification.behavioral_tests_run = false;
  f.report.schema_version = 2; await f.save();
  await assert.rejects(f.service.capabilities(), /incompatible/);
});
test("blocked examples cannot be read as ready recipes", async (t) => {
  const f = await fixture(t);
  f.report.examples.circle.status = "blocked"; await f.save();
  await assert.rejects(f.service.reference({ example: "circle" }), /not ready/);
});
test("path traversal and symlink escapes are rejected", async (t) => {
  const f = await fixture(t);
  f.report.examples.circle.repository_path = "web/python/examples/../../secret.py"; await f.save();
  await assert.rejects(f.service.reference({ example: "circle" }), /invalid source/);
  const outside = await mkdtemp(path.join(os.tmpdir(), "noon-outside-"));
  t.after(() => rm(outside, { recursive: true, force: true }));
  await writeFile(path.join(outside, "secret.py"), f.source);
  await symlink(path.join(outside, "secret.py"), path.join(f.root, "web/python/examples/link.py"));
  f.report.examples.circle.repository_path = "web/python/examples/link.py"; await f.save();
  await assert.rejects(f.service.reference({ example: "circle" }), /escapes/);
  await writeFile(path.join(f.root, "scripts/internal.py"), f.source);
  await symlink(path.join(f.root, "scripts/internal.py"), path.join(f.root, "web/python/examples/internal.py"));
  f.report.examples.circle.repository_path = "web/python/examples/internal.py"; await f.save();
  await assert.rejects(f.service.reference({ example: "circle" }), /escapes/);
});
test("changed or oversized example sources fail closed", async (t) => {
  const f = await fixture(t);
  await writeFile(path.join(f.root, f.sourcePath), "# changed");
  await assert.rejects(f.service.reference({ example: "circle" }), /changed since/);
  await writeFile(path.join(f.root, f.sourcePath), "#".repeat(65_537));
  await assert.rejects(f.service.reference({ example: "circle" }), /bounded regular/);
});
test("configured credentials and Python hooks are not inherited", async (t) => {
  const f = await fixture(t);
  const old = process.env.NOON_TEST_SECRET;
  process.env.NOON_TEST_SECRET = "do-not-forward";
  try { await f.service.capabilities(); }
  finally { if (old === undefined) delete process.env.NOON_TEST_SECRET; else process.env.NOON_TEST_SECRET = old; }
});
test("timeout kills a stuck exporter; subsequent calls remain usable", async (t) => {
  const f = await fixture(t, { timeoutMs: 500 });
  const exporter = path.join(f.root, "scripts/noon-capabilities.py");
  await writeFile(exporter, "while True: pass\n");
  await assert.rejects(f.service.capabilities());
  await writeFile(exporter, `print(${JSON.stringify(JSON.stringify(f.report))})\n`);
  assert.equal((await f.service.capabilities()).schema_version, 1);
});
test("in-flight cancellation, concurrent admission and permanent close are explicit", async (t) => {
  const f = await fixture(t);
  await writeFile(path.join(f.root, "scripts/noon-capabilities.py"), "while True: pass\n");
  const controller = new AbortController();
  const running = f.service.capabilities({}, { signal: controller.signal });
  const rejection = assert.rejects(running, /abort/i);
  await assert.rejects(f.service.capabilities(), /already running/);
  controller.abort();
  await rejection;
  f.service.close();
  await assert.rejects(f.service.capabilities(), /closed/);
});
test("excess output, invalid startup paths and invalid limits fail", async (t) => {
  const f = await fixture(t, { maxOutputBytes: 64 });
  await assert.rejects(f.service.capabilities());
  await assert.rejects(createDiscovery({ repoRoot: ".", pythonExecutable: python }), /absolute/);
  await assert.rejects(createDiscovery({ repoRoot: f.root, pythonExecutable: python, timeoutMs: 0 }), /limits/);
});
