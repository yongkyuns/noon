import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

import { foregroundBounds, foregroundMask } from "./browser-visual-parity-lib.mjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_RETAINED_TEXT_PLAYBACK_PORT ?? "4192");
const baseUrl = `http://127.0.0.1:${port}`;

const contentTypes = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".json", "application/json; charset=utf-8"],
  [".py", "text/x-python; charset=utf-8"],
]);

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, baseUrl);
    const relative = decodeURIComponent(url.pathname).replace(/^\/+/, "");
    const resolved = path.resolve(repoRoot, relative || "web/execution-worker-smoke.html");
    if (resolved !== repoRoot && !resolved.startsWith(`${repoRoot}${path.sep}`)) {
      response.writeHead(403).end("forbidden");
      return;
    }
    const info = await stat(resolved);
    if (!info.isFile()) {
      response.writeHead(404).end("not found");
      return;
    }
    response.setHeader("Cross-Origin-Opener-Policy", "same-origin");
    response.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
    response.setHeader("Cross-Origin-Resource-Policy", "same-origin");
    response.setHeader("Cache-Control", "no-store");
    response.setHeader(
      "Content-Type",
      contentTypes.get(path.extname(resolved)) ?? "application/octet-stream",
    );
    response.writeHead(200);
    createReadStream(resolved).pipe(response);
  } catch (error) {
    response.writeHead(error?.code === "ENOENT" ? 404 : 500).end(String(error));
  }
});
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(port, "127.0.0.1", resolve);
});

const shrinkSource = `from noon import *

class ShrinkToCenterExample(Scene):
    def construct(self):
        self.play(ShrinkToCenter(Text("Hello World!")))
`;

const browserArgs = [
  "--enable-unsafe-webgpu",
  "--enable-unsafe-swiftshader",
  "--use-webgpu-adapter=swiftshader",
  "--use-gpu-in-tests",
  "--ignore-gpu-blocklist",
  "--enable-features=Vulkan",
  "--use-gl=angle",
  "--use-angle=swiftshader",
  "--use-vulkan=swiftshader",
  "--disable-gpu-sandbox",
  "--disable-dev-shm-usage",
];

function visibleBounds(buffer) {
  const image = PNG.sync.read(buffer);
  const foreground = foregroundMask(image, 32);
  const bounds = foregroundBounds(foreground.mask, image.width, image.height);
  assert.ok(bounds, "rendered retained Text frame must contain visible foreground");
  return {
    ...bounds,
    width: bounds.maxX - bounds.minX + 1,
    height: bounds.maxY - bounds.minY + 1,
    centerX: (bounds.minX + bounds.maxX) / 2,
    centerY: (bounds.minY + bounds.maxY) / 2,
    foregroundPixels: foreground.count,
  };
}

async function seekAndWait(page, time, previousPresentedFrames) {
  return page.evaluate(
    async ({ time, previousPresentedFrames }) => {
      const execution = globalThis.__noonRetainedShrinkExecution;
      const seek = await execution.seek(time);
      let lastReport = null;
      for (let attempt = 0; attempt < 100; attempt += 1) {
        const report = await execution.metrics();
        lastReport = report;
        if (report.metrics.presentedFrames > previousPresentedFrames) {
          const state = await execution.state();
          return { seek, state, report };
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      const state = await execution.state();
      const diagnosticJson = (value) =>
        JSON.stringify(value, (_key, nested) =>
          typeof nested === "bigint" ? `${nested}n` : nested,
        );
      throw new Error(
        `retained renderer did not present seek to ${time}s; ` +
          `previousPresentedFrames=${previousPresentedFrames}; ` +
          `state=${diagnosticJson(state)}; metrics=${diagnosticJson(lastReport)}`,
      );
    },
    { time, previousPresentedFrames },
  );
}

let browser = null;
try {
  browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const started = await page.evaluate(async (source) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient, AUTHORING_EXECUTION_RETAINED } = await import(
      "./authoring-execution-client.js"
    );
    const authoring = new PythonAuthoringClient();
    const execution = new AuthoringExecutionClient(document.querySelector("#scene"));
    const authored = await authoring.run(source, {});
    if (!Number.isFinite(authored.duration) || authored.duration <= 0) {
      throw new Error(`ShrinkToCenter authored invalid duration ${authored.duration}`);
    }
    if ((authored.document?.objects?.length ?? 0) !== 0) {
      throw new Error("ShrinkToCenter(Text) must not synthesize legacy placeholder geometry");
    }
    if ((authored.retainedDocument?.objects?.length ?? 0) !== 1) {
      throw new Error("ShrinkToCenter must author exactly one retained Text object");
    }
    const tracks = authored.retainedDocument?.tracks ?? [];
    const scales = tracks.filter((track) => track.property === "scale");
    if (scales.length !== 1) {
      throw new Error(`ShrinkToCenter must author one scale track, got ${scales.length}`);
    }
    const ready = await execution.startRetained(
      JSON.stringify(authored.document),
      JSON.stringify(authored.retainedDocument),
      { loopDurationSeconds: authored.duration, transportMode: "transferable" },
    );
    await execution.pause();
    globalThis.__noonRetainedShrinkExecution = execution;
    globalThis.__noonRetainedShrinkAuthoring = authoring;
    const metrics = await execution.metrics();
    return {
      duration: authored.duration,
      track: scales[0],
      mode: execution.mode,
      expectedMode: AUTHORING_EXECUTION_RETAINED,
      ready,
      presentedFrames: metrics.metrics.presentedFrames,
    };
  }, shrinkSource);

  assert.equal(started.duration, 1);
  assert.equal(started.mode, started.expectedMode);
  assert.deepEqual(started.track.values.vec2, {
    from: { x: 1, y: 1 },
    to: { x: 0, y: 0 },
  });

  const early = await seekAndWait(page, 0.25, started.presentedFrames);
  assert.equal(early.seek.time, 0.25);
  assert.equal(early.state.time, 0.25);
  assert.equal(early.state.playing, false);
  const earlyBounds = visibleBounds(await page.locator("#scene").screenshot());

  const late = await seekAndWait(page, 0.65, early.report.metrics.presentedFrames);
  assert.equal(late.seek.time, 0.65);
  assert.equal(late.state.time, 0.65);
  assert.equal(late.state.playing, false);
  const lateBounds = visibleBounds(await page.locator("#scene").screenshot());

  assert.ok(
    lateBounds.width < earlyBounds.width * 0.65,
    `ShrinkToCenter must visibly shrink width: early=${earlyBounds.width}px late=${lateBounds.width}px`,
  );
  assert.ok(
    lateBounds.height < earlyBounds.height * 0.65,
    `ShrinkToCenter must visibly shrink height: early=${earlyBounds.height}px late=${lateBounds.height}px`,
  );
  assert.ok(
    lateBounds.foregroundPixels < earlyBounds.foregroundPixels * 0.65,
    `ShrinkToCenter must reduce foreground mass: early=${earlyBounds.foregroundPixels} late=${lateBounds.foregroundPixels}`,
  );
  // Foreground-mask bounds can move by a few pixels as glyph stems cross the
  // threshold at different scales. This still rejects meaningful translation
  // while avoiding false failures caused purely by rasterization/antialiasing.
  const centerTolerancePx = 5;
  assert.ok(
    Math.abs(lateBounds.centerX - earlyBounds.centerX) <= centerTolerancePx &&
      Math.abs(lateBounds.centerY - earlyBounds.centerY) <= centerTolerancePx,
    `ShrinkToCenter must remain visually centered within ${centerTolerancePx}px: ` +
      `early=(${earlyBounds.centerX},${earlyBounds.centerY}) ` +
      `late=(${lateBounds.centerX},${lateBounds.centerY})`,
  );
  assert.deepEqual(errors, [], `browser errors during retained ShrinkToCenter playback:\n${errors.join("\n")}`);

  await page.evaluate(() => {
    globalThis.__noonRetainedShrinkExecution?.terminate();
    globalThis.__noonRetainedShrinkAuthoring?.terminate();
    delete globalThis.__noonRetainedShrinkExecution;
    delete globalThis.__noonRetainedShrinkAuthoring;
  });

  console.log(
    `Retained ShrinkToCenter playback passed: ${earlyBounds.width}x${earlyBounds.height} at 0.25s -> ` +
      `${lateBounds.width}x${lateBounds.height} at 0.65s.`,
  );
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
