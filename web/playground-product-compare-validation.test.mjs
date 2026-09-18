import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

const command = new URL("../scripts/playground-product-compare.mjs", import.meta.url);
const report = () => ({
  exampleId: "validation-fixture",
  runtime: { backend: "webgpu" },
  shellReadyMs: 100,
  coldRunMs: 200,
  warmRunMs: 50,
  editRunMs: 60,
  fps: { effectiveFps: 60 },
  screenshot: "intentionally-not-loaded.png",
});

// Exercise the real CLI. Malformed measurements must fail before PNG loading,
// so these input-contract tests need neither a renderer nor image dependencies.
async function rejectsReport(baseline, candidate, expected, overrides = {}) {
  const directory = await mkdtemp(path.join(tmpdir(), "noon-product-input-"));
  try {
    const directories = [path.join(directory, "baseline"), path.join(directory, "candidate")];
    for (const [index, input] of [baseline, candidate].entries()) {
      await mkdir(directories[index]);
      await writeFile(path.join(directories[index], "report.json"), JSON.stringify(input));
    }
    const env = { ...process.env };
    for (const key of Object.keys(env)) {
      if (key.startsWith("NOON_PRODUCT_")) delete env[key];
    }
    const result = spawnSync(process.execPath, [command.pathname, ...directories], {
      encoding: "utf8", timeout: 10_000, env: { ...env, ...overrides },
    });
    assert.ifError(result.error);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, expected);
    assert.doesNotMatch(result.stderr, /ERR_MODULE_NOT_FOUND|ENOENT/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

for (const side of ["baseline", "candidate"]) {
  for (const value of [undefined, null, "", "60", false, 0, -1]) {
    test(`${side} FPS rejects ${String(value)} (${typeof value})`, async () => {
      const inputs = { baseline: report(), candidate: report() };
      inputs[side].fps.effectiveFps = value;
      await rejectsReport(inputs.baseline, inputs.candidate, /effective FPS must be a finite positive number/);
    });
  }
  for (const key of ["shellReadyMs", "coldRunMs", "warmRunMs", "editRunMs"]) {
    test(`${side} ${key} rejects negative latency`, async () => {
      const inputs = { baseline: report(), candidate: report() };
      inputs[side][key] = -1;
      await rejectsReport(inputs.baseline, inputs.candidate, /latency must be finite and non-negative/);
    });
  }
}

for (const [key, values] of Object.entries({
  NOON_PRODUCT_MAX_LATENCY_RATIO: ["NaN", "Infinity", "0", "-1"],
  NOON_PRODUCT_LATENCY_SLACK_MS: ["NaN", "Infinity", "-1"],
  NOON_PRODUCT_MIN_FPS_RATIO: ["NaN", "Infinity", "0", "-1"],
  NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO: ["NaN", "Infinity", "-1", "1.01"],
})) {
  for (const value of values) {
    test(`${key} rejects ${value}`, async () => {
      await rejectsReport(report(), report(), new RegExp(key), { [key]: value });
    });
  }
}
