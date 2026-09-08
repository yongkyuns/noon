import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs, compareEffectiveFrames, MAX_EFFECTIVE_ABSOLUTE_ERROR } from "./manim-raster-support.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// Keep the workspace feature union, so this check reuses the normal validation build.
execFileSync("cargo", ["build", "--quiet", "--workspace", "--all-features", "--example", "cross_language_parity"],
  { cwd: repoRoot, stdio: "inherit" });
const binary = path.resolve(repoRoot, process.env.CARGO_TARGET_DIR ?? "target", "debug/examples/cross_language_parity");
const corpus = execFileSync(binary, [], { cwd: repoRoot, encoding: "utf8" }).trim().split("\n").map(JSON.parse);
assert.equal(corpus.length, 7, "paired live program inventory changed");
const artifacts = path.resolve(repoRoot, process.env.NOON_PARITY_ARTIFACTS ?? "browser-smoke-artifacts/cross-language");
await mkdir(artifacts, { recursive: true });
await writeFile(path.join(artifacts, "rust.json"), `${JSON.stringify(corpus, null, 2)}\n`);

function comparable(frame) {
  assert.equal(frame?.engine, "noon", "missing current-runtime diagnostic");
  assert.ok(frame.publication, "missing runtime publication provenance");
  // Independent scenes allocate their own identities and publication epochs.
  // Compare painter-ordered effective state; never feed the artifact into an engine.
  const { publication, ...effective } = frame;
  return { ...effective, objects: frame.objects.map(({ id, ...object }) => object) };
}

const server = await serveRepository(repoRoot, Number(process.env.NOON_PARITY_PORT ?? "4180"));
let browser;
const report = [];
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  for (const fixture of corpus) {
    const page = await browser.newPage();
    const frames = [];
    const identities = new Map();
    const reverseIdentities = new Map();
    let deadline;
    try {
      await Promise.race([(async () => {
        const errors = [];
        page.on("pageerror", (error) => errors.push(String(error)));
        await page.goto(`${server.baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
        await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
        await page.evaluate(() => window.noonHostRaster.ready());
        const source = await readFile(path.join(repoRoot, "web/python/examples", `${fixture.name}.py`), "utf8");
        const loaded = await page.evaluate(({ source, duration }) => window.noonHostRaster.load(source, duration),
          { source, duration: fixture.times.at(-1) + 1 });
        assert.equal(loaded.kind, "semantic_execution");
        assert.equal(loaded.rendererBackend, "WebGPU");
        let maximumAbsoluteError = 0;
        for (const [index, time] of fixture.times.entries()) {
          const metrics = await page.evaluate(({ index, times }) => window.noonHostRaster.renderThrough(index, times),
            { index, times: fixture.times });
          assert.equal(metrics.time, time);
          assert.equal(metrics.presented, true);
          const frame = await page.evaluate(() => window.noonHostRaster.debugFrame());
          frames.push(frame);
          assert.equal(frame.objects.length, fixture.frames[index].objects.length);
          for (const [slot, object] of frame.objects.entries()) {
            const rustId = fixture.frames[index].objects[slot].id;
            if (!identities.has(rustId)) identities.set(rustId, object.id);
            if (!reverseIdentities.has(object.id)) reverseIdentities.set(object.id, rustId);
            assert.equal(identities.get(rustId), object.id, "Python identity changed across frames");
            assert.equal(reverseIdentities.get(object.id), rustId, "Rust identity changed across frames");
          }
          maximumAbsoluteError = Math.max(maximumAbsoluteError,
            compareEffectiveFrames(comparable(frame), comparable(fixture.frames[index])));
        }
        assert.deepEqual(errors, []);
        report.push({ name: fixture.name, samples: frames.length, maximumAbsoluteError });
        console.log(`[PASS] ${fixture.name}: ${frames.length} live Rust/Python frames`);
      })(), new Promise((_, reject) => {
        deadline = setTimeout(() => reject(new Error(`${fixture.name}: exceeded 60 seconds`)), 60_000);
      })]);
    } finally {
      clearTimeout(deadline);
      await writeFile(path.join(artifacts, `${fixture.name}-python.json`), `${JSON.stringify(frames, null, 2)}\n`);
      await page.close();
    }
  }
  await writeFile(path.join(artifacts, "report.json"), `${JSON.stringify({
    mode: "paired-current-runtime-frames", maximumEffectiveAbsoluteError: MAX_EFFECTIVE_ABSOLUTE_ERROR, fixtures: report,
  }, null, 2)}\n`);
} finally {
  await browser?.close();
  await server.close();
}
