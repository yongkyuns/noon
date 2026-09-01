import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4191;
const baseUrl = `http://127.0.0.1:${port}`;

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
      const response = await fetch(`${baseUrl}/web/`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`namespace smoke server did not start: ${lastError}\n${serverOutput}`);
}

const coldSource = `from noon import *
import sys

assert "numpy" not in sys.modules
assert PURE_RED.red == 1.0 and PURE_RED.green == 0.0 and PURE_RED.blue == 0.0
assert PURE_GREEN.red == 0.0 and PURE_GREEN.green == 1.0 and PURE_GREEN.blue == 0.0

class ColdNamespace(Scene):
    def construct(self):
        self.add(Circle(radius=0.2, color=PURE_GREEN))
`;

const numpySource = `from noon import *
import sys

class LazyNumpyNamespace(Scene):
    def construct(self):
        assert np.__name__ == "numpy"
        assert "numpy" in sys.modules
        value = float(np.log(np.e))
        assert abs(value - 1.0) < 1e-12
        self.add(Circle(radius=0.2, color=PURE_RED).shift(RIGHT * value))
`;

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/`, { waitUntil: "load" });
  await page.evaluate(() => {
    const worker = new Worker(new URL("./python-worker.js", location.href), {
      name: "noon-manim-namespace-smoke",
      type: "module",
    });
    let nextRequestId = 0;
    const pending = new Map();
    let resolveReady;
    let rejectReady;
    const ready = new Promise((resolve, reject) => {
      resolveReady = resolve;
      rejectReady = reject;
    });

    worker.addEventListener("error", (event) => {
      const error = new Error(event.message || "namespace worker crashed");
      rejectReady(error);
      for (const { reject } of pending.values()) reject(error);
      pending.clear();
    });
    worker.addEventListener("message", (event) => {
      const message = event.data;
      if (message?.channel !== "noon.authoring" || message?.protocolVersion !== 5) {
        const error = new Error("invalid namespace worker envelope");
        rejectReady(error);
        for (const { reject } of pending.values()) reject(error);
        pending.clear();
        return;
      }
      if (message.type === "ready") {
        resolveReady();
        return;
      }
      if (message.type === "error") {
        const error = new Error(String(message.message || "namespace authoring failed"));
        const request = pending.get(message.requestId);
        if (request) {
          pending.delete(message.requestId);
          request.reject(error);
        } else {
          rejectReady(error);
        }
        return;
      }
      if (message.type === "result") {
        const request = pending.get(message.requestId);
        if (!request) return;
        pending.delete(message.requestId);
        request.resolve(JSON.parse(message.resultJson));
      }
    });

    window.noonNamespaceSmoke = {
      ready: () => ready,
      run: async (source) => {
        await ready;
        const requestId = nextRequestId++;
        const result = new Promise((resolve, reject) => pending.set(requestId, { resolve, reject }));
        worker.postMessage({
          channel: "noon.authoring",
          protocolVersion: 5,
          type: "run",
          requestId,
          source,
          context: {},
        });
        return result;
      },
      stop: () => worker.terminate(),
    };
  });
  await page.evaluate(() => window.noonNamespaceSmoke.ready());

  const cold = await page.evaluate(
    (source) => window.noonNamespaceSmoke.run(source),
    coldSource,
  );
  assert.equal(cold.kind, "scene_document");
  assert.equal(cold.document.objects.length, 1);

  const numpy = await page.evaluate(
    (source) => window.noonNamespaceSmoke.run(source),
    numpySource,
  );
  assert.equal(numpy.kind, "scene_document");
  assert.equal(numpy.document.objects.length, 1);

  await page.evaluate(() => window.noonNamespaceSmoke.stop());
  assert.deepEqual(errors, [], `browser errors while testing lazy namespace loading:\n${errors.join("\n")}`);
  console.log(
    "Manim namespace smoke passed: baseline authoring leaves NumPy unloaded, np resolves to real NumPy on demand, and pure-color aliases match the pinned namespace.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
