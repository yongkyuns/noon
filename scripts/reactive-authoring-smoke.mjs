import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { createServer } from "node:http";
import { stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_REACTIVE_AUTHORING_SMOKE_PORT ?? "4178");
const baseUrl = `http://127.0.0.1:${port}`;
const contentTypes = new Map([
  [".html", "text/html; charset=utf-8"], [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"], [".wasm", "application/wasm"],
]);
const server = createServer(async (request, response) => {
  try {
    const relative = decodeURIComponent(new URL(request.url, baseUrl).pathname).replace(/^\/+/, "");
    const resolved = path.resolve(repoRoot, relative || "web/execution-worker-smoke.html");
    if (resolved !== repoRoot && !resolved.startsWith(`${repoRoot}${path.sep}`)) {
      response.writeHead(403).end("forbidden"); return;
    }
    const info = await stat(resolved);
    if (!info.isFile()) { response.writeHead(404).end("not found"); return; }
    response.setHeader("Cross-Origin-Opener-Policy", "same-origin");
    response.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
    response.setHeader("Cross-Origin-Resource-Policy", "same-origin");
    response.setHeader("Cache-Control", "no-store");
    response.setHeader("Content-Type", contentTypes.get(path.extname(resolved)) ?? "application/octet-stream");
    response.writeHead(200); createReadStream(resolved).pipe(response);
  } catch (error) {
    response.writeHead(error?.code === "ENOENT" ? 404 : 500).end(String(error));
  }
});

const source = `
from noon import *

class NativeTrackers(Scene):
    async def construct(self):
        square = Square(side_length=0.8, color=BLUE)
        circle = Circle(radius=0.3, color=PINK)
        mover = Square(side_length=0.4, color=GREEN)
        self.add(square, circle, mover)
        angle = ValueTracker(0.25)
        angle.increment_value(0.5).set_value(1.5)
        self.bind_rotation(square, angle)
        assert abs(angle.get_value() - 1.5) < 1e-9
        progress = self.value_tracker(0.0)
        self.bind_position(circle, progress, direction=RIGHT, offset=UP)
        assert abs(progress.get_value() - 0.0) < 1e-9
        await self.play(angle.animate(run_time=2.0, rate_func=linear).set_value(3.5))
        assert abs(angle.get_value() - 3.5) < 1e-9
        progress.set_value(1.0)
        assert abs(progress.get_value() - 1.0) < 1e-9
        await self.wait(1.0)
        # The mixed transform uses an independently authored leaf. Capturing
        # a reactive-bound target into a detached transform remains deferred.
        await self.play(
            progress.animate(run_time=5.0, rate_func=linear).set_value(2.0),
            mover.animate.shift(UP), run_time=1.0, rate_func=smooth,
        )
        assert abs(progress.get_value() - 2.0) < 1e-9
        center = mover.get_center()
        assert abs(center.y - 1.0) < 1e-9
`;

await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
let browser = null;
try {
  browser = await chromium.launch({ channel: "chromium", headless: true, args: ["--disable-dev-shm-usage"] });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => { if (message.type() === "error") errors.push(`console: ${message.text()}`); });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
  await page.evaluate(async () => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient(); await authoring.ready();
    window.reactiveSmoke = { authoring, AuthoringExecutionClient };
  });
  const result = await page.evaluate(async (pythonSource) => {
    const harness = window.reactiveSmoke; const canvas = document.querySelector("#scene");
    let execution = null; let registration = null; let sourceError = null;
    const authoredPromise = harness.authoring.run(pythonSource, {}, {
      async onSemanticContinuation(next) {
        if (registration !== null) throw new Error("source registered more than one continuation");
        registration = next; execution = new harness.AuthoringExecutionClient(canvas);
        await execution.startSemanticExecution(next.semanticExecution, {
          authoringClient: harness.authoring, loopDurationSeconds: Math.max(1, next.duration),
          transportMode: "transferable",
        });
      },
    });
    authoredPromise.catch((error) => { sourceError = String(error); });
    for (let attempt = 0; attempt < 300; attempt += 1) {
      if (sourceError !== null) throw new Error(sourceError);
      if (execution !== null && registration !== null) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (execution === null || registration === null) throw new Error("reactive source did not register shared continuation");
    const authored = await authoredPromise; const metrics = (await execution.metrics()).metrics;
    execution.terminate(); return { authored, metrics };
  }, source);
  assert.equal(result.authored.duration, 4);
  assert.ok(result.authored.semanticExecution, "tracker source should publish shared execution");
  assert.equal(result.metrics.objectCount, 3);
  assert.ok(result.metrics.drawCalls > 0, "mixed tracker/object scene did not render");
  assert.equal(errors.length, 0, errors.join("\n"));
  console.log("reactive authoring smoke test passed");
} finally {
  await browser?.close(); await new Promise((resolve) => server.close(resolve));
}
