import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const command = fileURLToPath(new URL("./playground-product-compare.mjs", import.meta.url));
const thresholdNames = [
  "NOON_PRODUCT_MAX_LATENCY_RATIO",
  "NOON_PRODUCT_LATENCY_SLACK_MS",
  "NOON_PRODUCT_MIN_FPS_RATIO",
  "NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO",
];

function report() {
  return {
    schemaVersion: 1,
    exampleId: "basic-shapes",
    runtime: { backend: "WebGL2" },
    shellReadyMs: 100,
    coldRunMs: 100,
    warmRunMs: 100,
    editRunMs: 100,
    fps: { effectiveFps: 60 },
    screenshot: "frame-final.png",
  };
}

test("product comparison rejects malformed reports and thresholds before image decoding", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "noon-product-compare-"));
  const baselineDir = path.join(root, "baseline");
  const candidateDir = path.join(root, "candidate");
  await Promise.all([mkdir(baselineDir), mkdir(candidateDir)]);
  const cleanEnv = { ...process.env };
  for (const name of thresholdNames) delete cleanEnv[name];

  try {
    async function rejects(name, mutate, expected, env = {}) {
      await t.test(name, async () => {
        const baseline = report();
        const candidate = report();
        mutate(baseline, candidate);
        await Promise.all([
          writeFile(path.join(baselineDir, "report.json"), JSON.stringify(baseline)),
          writeFile(path.join(candidateDir, "report.json"), JSON.stringify(candidate)),
        ]);
        const result = spawnSync(process.execPath, [command, baselineDir, candidateDir], {
          encoding: "utf8", env: { ...cleanEnv, ...env },
        });
        assert.notEqual(result.status, 0, `${name} unexpectedly passed`);
        assert.match(result.stderr, expected);
      });
    }

    for (const [name, value] of [
      ["null", null], ["empty string", ""], ["numeric string", "60"],
      ["zero", 0], ["negative", -1], ["non-finite", Number.NaN],
    ]) {
      await rejects(`baseline FPS ${name}`, (baseline) => {
        baseline.fps.effectiveFps = value;
      }, /effective FPS must be a finite positive number/);
      await rejects(`candidate FPS ${name}`, (_baseline, candidate) => {
        candidate.fps.effectiveFps = value;
      }, /effective FPS must be a finite positive number/);
    }

    for (const [name, value] of [
      ["null", null], ["numeric string", "100"], ["negative", -1],
      ["non-finite", Number.POSITIVE_INFINITY],
    ]) {
      await rejects(`cold latency ${name}`, (_baseline, candidate) => {
        candidate.coldRunMs = value;
      }, /cold Run → applied: latency must be finite and non-negative/);
    }

    await rejects("missing report schema", (baseline) => {
      delete baseline.schemaVersion;
    }, /baseline product report has an unsupported schema/);
    await rejects("missing example identity", (baseline) => {
      baseline.exampleId = null;
    }, /baseline product report must name an example/);
    await rejects("missing backend identity", (_baseline, candidate) => {
      candidate.runtime = {};
    }, /candidate product report must name a renderer backend/);

    for (const [name, value, expected] of [
      ["NOON_PRODUCT_MAX_LATENCY_RATIO", "", /NOON_PRODUCT_MAX_LATENCY_RATIO must not be empty/],
      ["NOON_PRODUCT_LATENCY_SLACK_MS", " ", /NOON_PRODUCT_LATENCY_SLACK_MS must not be empty/],
      ["NOON_PRODUCT_MIN_FPS_RATIO", "NaN", /NOON_PRODUCT_MIN_FPS_RATIO must be finite and positive/],
      ["NOON_PRODUCT_MIN_FPS_RATIO", "0", /NOON_PRODUCT_MIN_FPS_RATIO must be finite and positive/],
      ["NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO", "-1", /NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO must be between zero and one/],
    ]) {
      await rejects(`${name}=${JSON.stringify(value)}`, () => {}, expected, { [name]: value });
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
