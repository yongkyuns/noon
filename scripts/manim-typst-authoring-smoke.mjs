import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4187;
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
  throw new Error(`retained Typst authoring smoke server did not start: ${lastError}\n${serverOutput}`);
}

// Pinned ManimCE v0.21 examples from parity/manim-v0.21/typst_scenes.py.
// As with Noon's existing parity corpus, only the import is substituted.
const helloTypstSource = `from noon import *


class HelloTypst(Scene):
    def construct(self):
        text = Typst(r"*Hello* from _Typst!_", font_size=96)
        self.add(text)
`;

const helloMathTypstSource = `from noon import *


class HelloMathTypst(Scene):
    def construct(self):
        equation = MathTypst(r"sum_(k=1)^n k = (n(n + 1)) / 2", font_size=72)
        self.add(equation)
`;

const mixedPainterSource = `from noon import *


class MixedPainterOrder(Scene):
    def construct(self):
        self.add(Circle(radius=0.25))
        self.add(Typst("middle", font_size=48))
        self.add(Square(side_length=0.5))
`;

function assertSourceLevelSidecar(result, { source, math, fontSize, order }) {
  assert.equal(result.kind, "scene_document");
  assert.ok(result.retained_document, "scene result must include a retained authoring document");
  assert.equal(result.retained_document.channel, "noon.authoring.retained");
  assert.equal(result.retained_document.protocol_version, 1);
  assert.equal(result.retained_document.objects.length, 1);
  const object = result.retained_document.objects[0];
  assert.ok(Number.isSafeInteger(object.object), "retained object identity must survive JSON exactly");
  assert.ok(object.object >= 2 ** 52 && object.object < Number.MAX_SAFE_INTEGER);
  assert.equal(object.order, order);
  assert.equal(object.text.source, source);
  assert.equal(object.text.math, math);
  assert.equal(object.text.font_size, fontSize);

  const wire = JSON.stringify(result.retained_document);
  for (const forbidden of ["glyph", "font_bytes", "svg", "geometry", "atlas"]) {
    assert.ok(!wire.includes(forbidden), `retained authoring wire must not contain ${forbidden}`);
  }
}

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
      name: "noon-retained-typst-authoring-smoke",
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
      const error = new Error(event.message || "retained Typst worker crashed");
      rejectReady(error);
      for (const { reject } of pending.values()) reject(error);
      pending.clear();
    });
    worker.addEventListener("message", (event) => {
      const message = event.data;
      if (message?.channel !== "noon.authoring" || message?.protocolVersion !== 5) {
        const error = new Error("invalid retained Typst worker envelope");
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
        const error = new Error(String(message.message || "retained Typst authoring failed"));
        if (message.requestId === null) {
          rejectReady(error);
          for (const { reject } of pending.values()) reject(error);
          pending.clear();
          return;
        }
        const request = pending.get(message.requestId);
        if (request) {
          pending.delete(message.requestId);
          request.reject(error);
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

    window.noonRetainedTypstSmoke = {
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
  await page.evaluate(() => window.noonRetainedTypstSmoke.ready());

  const helloTypst = await page.evaluate(
    (source) => window.noonRetainedTypstSmoke.run(source),
    helloTypstSource,
  );
  assert.equal(helloTypst.document.objects.length, 0, "Typst must not create placeholder geometry");
  assertSourceLevelSidecar(helloTypst, {
    source: "*Hello* from _Typst!_",
    math: false,
    fontSize: 96,
    order: 0,
  });

  const helloMathTypst = await page.evaluate(
    (source) => window.noonRetainedTypstSmoke.run(source),
    helloMathTypstSource,
  );
  assert.equal(
    helloMathTypst.document.objects.length,
    0,
    "MathTypst must not create placeholder geometry",
  );
  assertSourceLevelSidecar(helloMathTypst, {
    source: "sum_(k=1)^n k = (n(n + 1)) / 2",
    math: true,
    fontSize: 72,
    order: 0,
  });

  const mixed = await page.evaluate(
    (source) => window.noonRetainedTypstSmoke.run(source),
    mixedPainterSource,
  );
  assert.equal(mixed.document.objects.length, 2, "only the circle and square belong to legacy geometry");
  assert.equal(mixed.document.objects[0].id, 0);
  assert.equal(mixed.document.objects[1].id, 1);
  assertSourceLevelSidecar(mixed, {
    source: "middle",
    math: false,
    fontSize: 48,
    order: 1,
  });

  await page.evaluate(() => window.noonRetainedTypstSmoke.stop());
  assert.deepEqual(errors, [], `browser errors while testing retained Typst authoring:\n${errors.join("\n")}`);
  console.log(
    "Retained Typst authoring smoke passed: pinned Manim v0.21 Typst/MathTypst sources (import-only substitution) emit source-only retained sidecars, zero placeholder geometry, exact JS-safe identities, and deterministic mixed painter order.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
