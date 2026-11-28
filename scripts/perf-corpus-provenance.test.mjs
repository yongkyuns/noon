import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const script = path.join(root, "scripts/perf-corpus.mjs");

for (const [name, env, expected] of [
  ["diagnostic flags", { NOON_CORPUS_INCLUDE_SAMPLES: "true" }, "NOON_CORPUS_INCLUDE_SAMPLES must be 0 or 1"],
  ["browser mode", { NOON_CORPUS_BROWSER_MODE: "physical" }, "NOON_CORPUS_BROWSER_MODE must be headless or headful"],
  ["GPU mode", { NOON_CORPUS_GPU_MODE: "automatic" }, "NOON_CORPUS_GPU_MODE must be hardware or software"],
]) {
  test(`rejects invalid ${name} before starting the browser`, () => {
    const result = spawnSync(process.execPath, [script], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, ...env },
    });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, new RegExp(expected.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
    assert.doesNotMatch(result.stderr, /Corpus .*…/);
  });
}

test("retains the first workload and the rejected report when a later workload fails", async t => {
  const directory = await mkdtemp(path.join(tmpdir(), "noon-corpus-failure-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const artifact = path.join(directory, "corpus.json");
  const modules = {
    "node:child_process": `
      export { spawnSync } from "node:child_process";
      export function spawn() {
        return { stdout: { on() {} }, stderr: { on() {} }, kill() {} };
      }
    `,
    playwright: `
      let pageCount = 0;
      export default { chromium: { async launch() {
        return {
          version: () => "fixture",
          close() {},
          newPage() {
            const objects = ++pageCount === 1 ? 4 : 0;
            return {
              goto() {}, waitForFunction() {}, close() {},
              locator: () => ({ getAttribute: () => "ready" }),
              evaluate(query) {
                return query.toString().includes("__NOON_SCENE_PERF__") ? {
                  scene: { objects }, execution: { sourceContinuation: true },
                  environment: { rendererBackend: "WebGPU" },
                  cadence: { effective: { effectiveFps: 60 }, frameIntervalMs: { p95: 17 } },
                } : { api: "webgpu", available: false, adapter: null };
              },
            };
          },
        };
      } } };
    `,
  };
  const preload = `
    import { registerHooks } from "node:module";
    const modules = ${JSON.stringify(modules)};
    registerHooks({ resolve(specifier, context, next) {
      if (context.parentURL === ${JSON.stringify(pathToFileURL(script).href)} &&
          Object.hasOwn(modules, specifier)) {
        return { shortCircuit: true, url: "data:text/javascript," + encodeURIComponent(modules[specifier]) };
      }
      return next(specifier, context);
    } });
    globalThis.fetch = async () => ({ ok: true });
  `;
  const result = spawnSync(process.execPath, ["--import", `data:text/javascript,${encodeURIComponent(preload)}`, script], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, NOON_CORPUS_ARTIFACT: artifact, NOON_CORPUS_GPU_MODE: undefined,
      NOON_CORPUS_BACKEND: "webgpu", NOON_CORPUS_CASES: "spatial-camera-only,spatial-moving-mesh-600-static" },
  });
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /objects 0 < 601/);
  const saved = JSON.parse(await readFile(artifact, "utf8"));
  assert.deepEqual(saved.results.map(entry => entry.definition.id), ["spatial-camera-only"]);
  assert.equal(saved.failure.id, "spatial-moving-mesh-600-static");
  assert.match(saved.failure.sourceSha256, /^[0-9a-f]{64}$/);
  assert.equal(saved.failure.report.scene.objects, 0);
});
