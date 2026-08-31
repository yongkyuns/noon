import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

import { foregroundMask } from "./browser-visual-parity-lib.mjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_RETAINED_TEXT_CREATION_PLAYBACK_PORT ?? "4198");
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

const source = `from noon import *

class RetainedTextCreationPlayback(Scene):
    def construct(self):
        label = Text("Create / Uncreate", font_size=96)
        self.play(Create(label), run_time=1.0, rate_func=linear)
        self.play(Uncreate(label), run_time=1.0, rate_func=linear)
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

function foregroundPixels(buffer) {
  const image = PNG.sync.read(buffer);
  return foregroundMask(image, 32).count;
}

async function seekAndWait(page, time, previousPresentedFrames) {
  return page.evaluate(
    async ({ time, previousPresentedFrames }) => {
      const execution = globalThis.__noonRetainedCreationExecution;
      const seek = await execution.seek(time);
      let lastReport = null;
      for (let attempt = 0; attempt < 100; attempt += 1) {
        const report = await execution.metrics();
        lastReport = report;
        if (report.metrics.presentedFrames > previousPresentedFrames) {
          return { seek, state: await execution.state(), report };
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
          `state=${diagnosticJson(state)}; metrics=${diagnosticJson(lastReport)}`,
      );
    },
    { time, previousPresentedFrames },
  );
}

let browser = null;
try {
  browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs });
  const page = await browser.newPage({ viewport: { width: 900, height: 520 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const started = await page.evaluate(async (sourceText) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient, AUTHORING_EXECUTION_RETAINED } = await import(
      "./authoring-execution-client.js"
    );
    const authoring = new PythonAuthoringClient();
    const execution = new AuthoringExecutionClient(document.querySelector("#scene"));
    const authored = await authoring.run(sourceText, {});
    const reveals = (authored.retainedDocument?.tracks ?? []).filter(
      (track) => track.property === "reveal" && track.timing.duration > 0,
    );
    if (authored.duration !== 2 || reveals.length !== 2) {
      throw new Error(
        `Create/Uncreate authored unexpected duration/tracks: duration=${authored.duration} reveals=${reveals.length}`,
      );
    }
    if ((authored.document?.objects?.length ?? 0) !== 0) {
      throw new Error("Create/Uncreate(Text) must remain retained-only");
    }
    const ready = await execution.startRetained(
      JSON.stringify(authored.document),
      JSON.stringify(authored.retainedDocument),
      { loopDurationSeconds: authored.duration, transportMode: "transferable" },
    );
    await execution.pause();
    globalThis.__noonRetainedCreationExecution = execution;
    globalThis.__noonRetainedCreationAuthoring = authoring;
    const metrics = await execution.metrics();
    return {
      mode: execution.mode,
      expectedMode: AUTHORING_EXECUTION_RETAINED,
      ready,
      reveals,
      presentedFrames: metrics.metrics.presentedFrames,
    };
  }, source);

  assert.equal(started.mode, started.expectedMode);
  assert.deepEqual(started.reveals[0].values.scalar, { from: 0, to: 1 });
  assert.deepEqual(started.reveals[1].values.scalar, { from: 1, to: 0 });

  const createEarly = await seekAndWait(page, 0.2, started.presentedFrames);
  const createEarlyPixels = foregroundPixels(await page.locator("#scene").screenshot());
  const createLate = await seekAndWait(
    page,
    0.8,
    createEarly.report.metrics.presentedFrames,
  );
  const createLatePixels = foregroundPixels(await page.locator("#scene").screenshot());
  assert.ok(
    createLatePixels > createEarlyPixels * 1.35,
    `Create(Text) must visibly reveal more ink: early=${createEarlyPixels} late=${createLatePixels}`,
  );

  const uncreateEarly = await seekAndWait(
    page,
    1.2,
    createLate.report.metrics.presentedFrames,
  );
  const uncreateEarlyPixels = foregroundPixels(await page.locator("#scene").screenshot());
  const uncreateLate = await seekAndWait(
    page,
    1.8,
    uncreateEarly.report.metrics.presentedFrames,
  );
  const uncreateLatePixels = foregroundPixels(await page.locator("#scene").screenshot());
  assert.ok(
    uncreateLatePixels < uncreateEarlyPixels * 0.75,
    `Uncreate(Text) must visibly remove ink: early=${uncreateEarlyPixels} late=${uncreateLatePixels}`,
  );

  assert.equal(createEarly.state.playing, false);
  assert.equal(createLate.state.playing, false);
  assert.equal(uncreateEarly.state.playing, false);
  assert.equal(uncreateLate.state.playing, false);
  assert.deepEqual(errors, [], `browser errors during retained creation playback:\n${errors.join("\n")}`);

  await page.evaluate(() => {
    globalThis.__noonRetainedCreationExecution?.terminate();
    globalThis.__noonRetainedCreationAuthoring?.terminate();
    delete globalThis.__noonRetainedCreationExecution;
    delete globalThis.__noonRetainedCreationAuthoring;
  });

  console.log(
    `Retained Text creation playback passed: Create ${createEarlyPixels}->${createLatePixels}, ` +
      `Uncreate ${uncreateEarlyPixels}->${uncreateLatePixels} foreground pixels.`,
  );
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
