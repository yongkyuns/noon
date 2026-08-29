import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_AUTHORING_EXECUTION_LIFECYCLE_PORT ?? "4183");
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

const mixedSource = `from noon import *

class MixedLifecycleScene(Scene):
    def construct(self):
        self.add(Circle(radius=0.4))
        self.add(Typst("middle", font_size=56))
        self.add(Square(side_length=0.8))
`;

const legacySource = `from noon import *

class LegacyLifecycleScene(Scene):
    def construct(self):
        self.add(Circle(radius=0.5))
        self.add(Square(side_length=0.7).shift(RIGHT * 1.5))
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

let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs,
  });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const result = await page.evaluate(async ({ mixedSource, legacySource }) => {
    const NativeWorker = globalThis.Worker;
    const workerTerminateCounts = [];
    globalThis.Worker = new Proxy(NativeWorker, {
      construct(Target, args, newTarget) {
        const worker = Reflect.construct(Target, args, newTarget);
        const index = workerTerminateCounts.length;
        workerTerminateCounts.push(0);
        const nativeTerminate = worker.terminate.bind(worker);
        worker.terminate = () => {
          workerTerminateCounts[index] += 1;
          nativeTerminate();
        };
        return worker;
      },
    });

    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient, AUTHORING_EXECUTION_RETAINED } = await import(
      "./authoring-execution-client.js"
    );

    const authoring = new PythonAuthoringClient();
    const emptySceneJson = '{"version":1,"objects":[],"tracks":[]}';
    const mixed = await authoring.run(mixedSource, {});
    const legacy = await authoring.run(legacySource, {});

    function freshCanvas() {
      const canvas = document.createElement("canvas");
      canvas.width = 640;
      canvas.height = 360;
      canvas.style.width = "640px";
      canvas.style.height = "360px";
      document.body.append(canvas);
      return canvas;
    }

    async function expectTerminated(execution, operation) {
      const promise = operation();
      execution.terminate();
      let operationError = null;
      try {
        await promise;
      } catch (error) {
        operationError = String(error);
      }
      let stateError = null;
      let metricsError = null;
      try {
        await execution.state();
      } catch (error) {
        stateError = String(error);
      }
      try {
        await execution.metrics();
      } catch (error) {
        metricsError = String(error);
      }
      return {
        operationError,
        mode: execution.mode,
        rendererBackend: execution.rendererBackend,
        stateError,
        metricsError,
      };
    }

    const startupExecution = new AuthoringExecutionClient(freshCanvas());
    const startup = await expectTerminated(startupExecution, () =>
      startupExecution.start(emptySceneJson, {
        loopDurationSeconds: 4,
        transportMode: "transferable",
      }),
    );

    const retainedExecution = new AuthoringExecutionClient(freshCanvas());
    await retainedExecution.start(emptySceneJson, {
      loopDurationSeconds: 4,
      transportMode: "transferable",
    });
    const toRetained = await expectTerminated(retainedExecution, () =>
      retainedExecution.reconcileScene(JSON.stringify(mixed.document), {
        retainedDocumentJson: JSON.stringify(mixed.retainedDocument),
        callbacks: mixed.callbacks,
        authoringClient: authoring,
        loopDurationSeconds: mixed.duration > 0 ? mixed.duration : null,
      }),
    );

    const legacyExecution = new AuthoringExecutionClient(freshCanvas());
    await legacyExecution.start(emptySceneJson, {
      loopDurationSeconds: 4,
      transportMode: "transferable",
    });
    const retainedReady = await legacyExecution.reconcileScene(JSON.stringify(mixed.document), {
      retainedDocumentJson: JSON.stringify(mixed.retainedDocument),
      callbacks: mixed.callbacks,
      authoringClient: authoring,
      loopDurationSeconds: mixed.duration > 0 ? mixed.duration : null,
    });
    const toLegacy = await expectTerminated(legacyExecution, () =>
      legacyExecution.reconcileScene(JSON.stringify(legacy.document), {
        retainedDocumentJson: JSON.stringify(legacy.retainedDocument),
        callbacks: legacy.callbacks,
        authoringClient: authoring,
        loopDurationSeconds: legacy.duration > 0 ? legacy.duration : null,
      }),
    );

    authoring.terminate();
    globalThis.Worker = NativeWorker;
    return {
      startup,
      toRetained,
      retainedReadyMode: retainedReady.mode,
      retainedMode: AUTHORING_EXECUTION_RETAINED,
      toLegacy,
      workerTerminateCounts,
    };
  }, { mixedSource, legacySource });

  for (const scenario of [result.startup, result.toRetained, result.toLegacy]) {
    assert.match(
      scenario.operationError,
      /AuthoringExecutionClient was terminated during an asynchronous operation/,
    );
    assert.equal(scenario.mode, null);
    assert.equal(scenario.rendererBackend, "");
    assert.match(scenario.stateError, /AuthoringExecutionClient has not been started/);
    assert.match(scenario.metricsError, /AuthoringExecutionClient has not been started/);
  }
  assert.equal(result.retainedReadyMode, result.retainedMode);
  assert.ok(result.workerTerminateCounts.length >= 6, "all three scenarios should create worker pairs");
  assert.ok(
    result.workerTerminateCounts.every((count) => count <= 1),
    `workers must be terminated at most once: ${result.workerTerminateCounts.join(",")}`,
  );
  assert.deepEqual(browserErrors, []);
  console.log("✓ authoring execution termination invalidates unpublished worker generations");
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
