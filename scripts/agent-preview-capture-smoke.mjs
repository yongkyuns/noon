// Trusted browser qualification for capture→artifact ownership. Not a sandbox or agent CLI.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { once } from "node:events";
import { chromium } from "playwright";
import { FrameArtifactStore } from "./agent-preview-artifacts.mjs";
import { PreviewFrameArtifactSession } from "./agent-preview-capture.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const evidence = path.join(root, "browser-smoke-artifacts/semantic-preview");
const port = 4188;
const url = `http://127.0.0.1:${port}/manim-raster-host.html`;
const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", path.join(root, "web")], {
  stdio: ["ignore", "ignore", "pipe"],
});
let browser;
try {
  await mkdir(evidence, { recursive: true });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try { ready = (await fetch(url)).ok; } catch {}
    if (ready) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.equal(ready, true, "capture smoke server did not start");
  browser = await chromium.launch({ headless: true, args: [
    "--disable-features=WebGPU", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist",
    "--use-gl=angle", "--use-angle=swiftshader",
  ] });
  const page = await browser.newPage({ viewport: { width: 960, height: 540 }, deviceScaleFactor: 1 });
  await page.goto(url);
  await page.waitForFunction(() => window.noonHostRaster !== undefined);
  const source = await readFile(path.join(root, "web/python/examples/manim_parity_square_to_circle.py"), "utf8");
  const loaded = await page.evaluate((code) => window.noonHostRaster.load(code, 4), source);
  assert.equal(loaded.rendererBackend, "WebGL2");
  const frameTimes = Array.from({ length: 46 }, (_, frame) => frame / 30);
  const sample = await page.evaluate(({ frameTimes }) => window.noonHostRaster.renderThrough(45, frameTimes), { frameTimes });
  assert.equal(sample.requestedTime, 1.5);
  assert.equal(sample.publishedTime, 1.5);
  assert.equal(sample.time, sample.publishedTime);
  assert.equal(sample.rendererBackend, "WebGL2");
  const captured = await page.locator("#scene").screenshot();

  const store = new FrameArtifactStore();
  const artifacts = new PreviewFrameArtifactSession({ store, sessionId: "browser-webgl-middle", source });
  const descriptor = artifacts.retain(sample, captured);
  const retained = artifacts.read(descriptor.id);
  assert.deepEqual(retained.png, captured);
  assert.equal(retained.descriptor.provenance.requestedTime, sample.requestedTime);
  assert.equal(retained.descriptor.provenance.publishedTime, sample.publishedTime);
  assert.equal(retained.descriptor.provenance.backend, sample.rendererBackend);
  assert.deepEqual(retained.descriptor.provenance.build, {
    engineRevision: null, wasmSha256: null, workerSha256: null, buildId: null,
  });
  await writeFile(path.join(evidence, "capture-store-frame.png"), retained.png);
  await writeFile(path.join(evidence, "capture-store.json"), JSON.stringify(retained.descriptor, null, 2));
  assert.equal(artifacts.close(), true);
  assert.equal(store.stats().artifacts, 0);
  await page.evaluate(() => window.noonHostRaster.close());
  await page.close();
  console.log("Actual WebGL2 preview PNG retained and retrieved through FrameArtifactStore");
} finally {
  try { await browser?.close(); }
  finally {
    if (server.pid && server.exitCode === null && server.signalCode === null) {
      const exited = once(server, "exit");
      server.kill("SIGKILL");
      await exited;
    }
  }
}
