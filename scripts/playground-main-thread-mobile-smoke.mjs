import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { PNG } from "pngjs";

const { webkit, devices } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_PLAYGROUND_MAIN_THREAD_PORT ?? "4186");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
    "browser-smoke-artifacts/playground-matrix/webkit-mobile-dpr2",
);

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/index.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Mobile fallback server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForIdle(page) {
  await page.waitForFunction(
    () => window.__noonExampleGallery && !window.__noonExampleGallery.runInFlight,
    null,
    { timeout: 60_000 },
  );
}

async function assertApplied(page, expectedId) {
  await page.waitForFunction(
    (id) => {
      const patch = document.querySelector("#patch-status");
      if (patch?.dataset.state === "error") return true;
      return patch?.dataset.state === "applied" && patch?.dataset.exampleId === id;
    },
    expectedId,
    { timeout: 60_000 },
  );
  const result = await page.evaluate(() => ({
    patchState: document.querySelector("#patch-status")?.dataset.state ?? null,
    patchText: document.querySelector("#patch-status")?.value ?? "",
    statusText: document.querySelector("#status-text")?.textContent ?? "",
    selectedExampleId: window.__noonExampleGallery?.selectedExampleId ?? null,
  }));
  assert.equal(
    result.patchState,
    "applied",
    `${expectedId} failed: ${result.patchText} ${result.statusText}`,
  );
  assert.equal(result.selectedExampleId, expectedId);
}

function visiblePixels(bytes) {
  const image = PNG.sync.read(bytes);
  const background = image.data.subarray(0, 3);
  let count = 0;
  for (let i = 0; i < image.data.length; i += 4) {
    if (image.data[i + 3] && Math.max(
      Math.abs(image.data[i] - background[0]),
      Math.abs(image.data[i + 1] - background[1]),
      Math.abs(image.data[i + 2] - background[2]),
    ) > 12) count += 1;
  }
  return count;
}

let browser = null;
let activePage = null;
let activeErrors = [];
try {
  await waitForServer();
  browser = await webkit.launch({ headless: true });
  for (const variant of [
    { name: "automatic", host: null, disableJspi: false },
    { name: "forced-main", host: "main-thread", disableJspi: false },
    { name: "automatic-no-jspi", host: null, disableJspi: true },
  ]) {
    const context = await browser.newContext({ ...devices["iPhone 13"] });
    if (variant.disableJspi) {
      await context.route("**/python-worker.js", async (route) => {
        const response = await route.fetch();
        await route.fulfill({ response, body:
          "delete WebAssembly.promising; delete WebAssembly.Suspending;\n" +
          "if (typeof WebAssembly.promising !== 'undefined' || typeof WebAssembly.Suspending !== 'undefined') throw new Error('JSPI test precondition failed');\n" + await response.text(),
        });
      });
    }
    const page = await context.newPage();
    activePage = page;
    const pageErrors = [];
    const consoleErrors = [];
    activeErrors = [pageErrors, consoleErrors];
    page.on("pageerror", (error) => pageErrors.push(String(error)));
    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(message.text());
    });

    await page.goto(
      `${baseUrl}/web/index.html?example=parity-square-to-circle${variant.host ? `&renderHost=${variant.host}` : ""}`,
      { waitUntil: "load" },
    );
    // Neutralize only CSS chrome, keeping border widths and canvas dimensions.
    // Otherwise a rounded decorative border is miscounted as engine geometry.
    await page.addStyleTag({ content:
      "#scene { border-color: #000 !important; border-radius: 0 !important; " +
      "box-shadow: none !important; background: #000 !important; }",
    });
    await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
    await waitForIdle(page);

    await page.locator("#scene").scrollIntoViewIfNeeded();

    // Observe the first execution, not a metrics call blocked by retirement of a
    // previous Python context. Context retirement is serialized behind an active
    // construct, even though its replacement renderer is already presenting.
    // The unmodified SquareToCircle must visibly animate and then FadeOut.
    await page.evaluate(() => {
      window.__mobileRun = window.__noonExampleGallery.run();
      window.__mobileRun.catch(() => {});
    });
    let metrics;
    const samples = [];
    let observation = null;
    const sampleDeadline = Date.now() + 60_000;
    while (Date.now() < sampleDeadline) {
      // Retain the observations so a missed frame has actionable evidence.
      const sample = await page.evaluate(async () => {
        const gallery = window.__noonExampleGallery;
        const patch = document.querySelector("#patch-status");
        if (gallery.selectedExampleId !== "parity-square-to-circle" ||
            patch?.dataset.exampleId !== "parity-square-to-circle") return null;
        if (patch.dataset.state === "error") return { error: patch.value };
        const result = await gallery.executionMetrics();
        return result?.metrics ? {
          time: result.metrics.time,
          backend: result.metrics.backend,
          objectCount: result.metrics.objectCount,
        } : null;
      });
      if (samples.length < 1000) samples.push(sample);
      assert.ok(!sample?.error, sample?.error);
      if (sample?.time > 1.15 && sample.time < 1.85 && sample.objectCount === 1) {
        observation = sample;
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    await writeFile(path.join(artifactDir, `${variant.name}-samples.json`), JSON.stringify(samples, null, 2));
    assert.ok(observation, "mobile source did not publish its intermediate transformation");
    const intermediate = await page.locator("#scene").screenshot();
    assert.ok(visiblePixels(intermediate) > 20, "mobile intermediate frame is blank");
    await page.evaluate(() => window.__mobileRun);
    await assertApplied(page, "parity-square-to-circle");
    const final = await page.locator("#scene").screenshot();
    metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    assert.equal(metrics?.metrics?.objectCount, 0, "FadeOut did not remove the object");
    assert.ok(Math.abs(metrics.metrics.time - 3) < 1e-6, "source did not complete at its authored time");
    assert.equal(visiblePixels(final), 0, "mobile final FadeOut frame retained visible geometry");
    await writeFile(path.join(artifactDir, `${variant.name}-intermediate.png`), intermediate);
    await writeFile(path.join(artifactDir, `${variant.name}-final.png`), final);

    await page.evaluate(() => window.__noonExampleGallery.select("parity-create-circle"));
    await assertApplied(page, "parity-create-circle");
    metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    assert.equal(metrics?.metrics?.objectCount, 1, "CreateCircle was not rendered after switching");
    if (variant.host) assert.equal(metrics?.renderHost, variant.host);
    else assert.ok(["worker", "main-thread"].includes(metrics?.renderHost));

    await page.evaluate(() => window.__noonExampleGallery.select("compatible-text-write"));
    await assertApplied(page, "compatible-text-write");

    const marker = "# mobile main-thread render fallback rerun";
    await page.evaluate((value) => {
      const editor = document.querySelector("#python-scene-source");
      if (!(editor instanceof HTMLTextAreaElement)) throw new Error("Python editor is unavailable");
      editor.value = `${editor.value.trimEnd()}\n\n${value}\n`;
      editor.dispatchEvent(new Event("input", { bubbles: true }));
    }, marker);
    await page.evaluate(() => window.__noonExampleGallery.run());
    await assertApplied(page, "compatible-text-write");

    for (const viewport of [
      { width: 430, height: 760 },
      { width: 360, height: 780 },
      { width: 390, height: 844 },
    ]) {
      await page.setViewportSize(viewport);
    }
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));

    metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    if (variant.host) assert.equal(metrics?.renderHost, variant.host);
    assert.ok(metrics?.metrics?.presentedFrames > 0);
    assert.equal(pageErrors.length, 0, pageErrors.join("\n"));
    assert.equal(consoleErrors.length, 0, consoleErrors.join("\n"));

    // Exercise the public source loader, not a mocked legacy module. Ineligible
    // callables must retain their Python behavior on both JSPI and no-JSPI hosts.
    for (const [name, source] of [
      ["instance overrides", `from noon import *
events = []
class InstanceOverride(Scene):
    def setup(self):
        self.wait = lambda d: events.append(('wait', d))
        self.play = lambda x: events.append(('play', x))
    def construct(self):
        self.wait(0.5)
        self.play(2)
        assert events == [('wait', 0.5), ('play', 2)], events
        self.add(Circle())
`],
      ["descriptor lookup", `from noon import *
events = []
class DescriptorOverride(Scene):
    @property
    def wait(self):
        events.append('get')
        return lambda d: events.append(d)
    def construct(self):
        self.wait(0.5)
        assert events == ['get', 0.5], events
        self.add(Circle())
`],
      ["dynamic lookup", `from noon import *
events = []
class DynamicOverride(Scene):
    def __getattribute__(self, name):
        if name == 'wait':
            events.append('get')
            return lambda d: events.append(d)
        return super().__getattribute__(name)
    def construct(self):
        self.wait(0.5)
        assert events == ['get', 0.5], events
        self.add(Circle())
`],
      ["explicit async", `from noon import *
class ExplicitAsync(Scene):
    async def construct(self):
        circle = Circle()
        await self.play(Create(circle), run_time=0.1)
        await self.wait(0.1)
        assert abs(self.time - 0.2) < 1e-8
`],
    ]) {
      await page.evaluate((source) => {
        const editor = document.querySelector("#python-scene-source");
        editor.value = source;
        editor.dispatchEvent(new Event("input", { bubbles: true }));
      }, source);
      await page.evaluate(() => window.__noonExampleGallery.run());
      await assertApplied(page, "compatible-text-write");
      const report = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
      assert.equal(report?.metrics?.objectCount, 1, `${name}: source contract did not complete`);
    }

    assert.equal(pageErrors.length, 0, pageErrors.join("\n"));
    assert.equal(consoleErrors.length, 0, consoleErrors.join("\n"));

    const diagnostics = {
      variant,
      intermediate: observation,
      renderHost: metrics.renderHost,
      rendererBackend: metrics.metrics?.backend ?? null,
      presentedFrames: metrics.metrics?.presentedFrames ?? null,
      publicSourceContracts: ["instance overrides", "descriptor lookup", "dynamic lookup", "explicit async"],
      selectedExampleId: await page.evaluate(() => window.__noonExampleGallery.selectedExampleId),
      pageErrors,
      consoleErrors,
    };
    await writeFile(
      path.join(artifactDir, `${variant.name}.json`),
      `${JSON.stringify(diagnostics, null, 2)}\n`,
      "utf8",
    );
    await page.screenshot({
      path: path.join(artifactDir, `${variant.name}.png`),
      fullPage: false,
    });
    console.log(
      `✓ mobile WebKit ${variant.name}: ${diagnostics.rendererBackend}, ${diagnostics.presentedFrames} frames`,
    );
    await context.close();
  }
} catch (error) {
  const state = await activePage?.evaluate(() => ({
    status: document.querySelector("#status-text")?.textContent,
    patch: document.querySelector("#patch-status")?.value,
    patchState: document.querySelector("#patch-status")?.dataset.state,
    selectedExampleId: window.__noonExampleGallery?.selectedExampleId,
  })).catch(() => null);
  await writeFile(path.join(artifactDir, "failure.json"), JSON.stringify({ error: String(error), state, errors: activeErrors }, null, 2));
  await activePage?.screenshot({ path: path.join(artifactDir, "failure.png"), timeout: 5_000 }).catch(() => {});
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
