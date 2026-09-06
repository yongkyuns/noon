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
  throw new Error(`retained text authoring smoke server did not start: ${lastError}\n${serverOutput}`);
}

// Pinned ManimCE v0.21 Typst examples plus the native Text surface that replaces
// Noon's temporary geometry-backed demo labels. Only the import is substituted.
const helloTextSource = `from noon import *


class HelloText(Scene):
    def construct(self):
        text = Text("Native Noon", font_size=48)
        self.add(text)
`;

const multilineTextSource = `from noon import *


class MultilineText(Scene):
    def construct(self):
        text = Text("first\\nsecond", font_size=36, line_spacing=0.5, color=YELLOW)
        self.add(text)
`;

const nativeTextLayoutSource = `from noon import *


class NativeTextLayout(Scene):
    def construct(self):
        box = Square(side_length=2)
        label = Text("Native Noon", font_size=48)
        if label.width <= 0 or label.height <= 0:
            raise RuntimeError("native Text layout metrics must be positive")
        label.width = 2
        label.next_to(box, RIGHT, buff=0.25)
        self.add(box, label)
`;

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
        self.add(Text("middle", font_size=48))
        self.add(Square(side_length=0.5))
`;

function canonicalTextObject(result, {
  source,
  fontSize,
  effectiveFontSize = null,
  order,
  objectId,
}) {
  assert.equal(result.kind, "scene_document");
  assert.equal(result.retained_document, null, "the canonical export carries mixed content");
  assert.ok(result.scene_spec, "scene result must include canonical SceneSpec");
  const object = result.scene_spec.objects[order];
  assert.equal(object.id, objectId, "text must use the scene-global object ID allocator");
  assert.equal(object.content.kind, "text");
  const text = object.content.value;
  assert.equal(text.source, source);
  assert.equal(text.font_size, fontSize);
  if (effectiveFontSize !== null) {
    assert.ok(
      Math.abs(text.font_size * object.transform.scale.x - effectiveFontSize) < 1e-5,
      "canonical text font size and transform scale must preserve the effective X presentation",
    );
    assert.ok(
      Math.abs(text.font_size * object.transform.scale.y - effectiveFontSize) < 1e-5,
      "canonical text font size and transform scale must preserve the effective Y presentation",
    );
  }

  const wire = JSON.stringify(text);
  for (const forbidden of ["glyph", "font_bytes", "svg", "geometry", "atlas"]) {
    assert.ok(!wire.includes(forbidden), `canonical text source must not contain ${forbidden}`);
  }
  return { object, text };
}

function assertNativeText(result, expected) {
  const text = canonicalTextObject(result, expected);
  assert.equal(text.text.kind, "plain");
  assert.equal(text.text.options.kind, "native_plain");
  assert.equal(text.text.options.font_family, expected.fontFamily ?? "DejaVu Sans Mono");
  assert.equal(text.text.options.line_spacing, expected.lineSpacing ?? -1);
  return text;
}

function assertTypst(result, expected) {
  const text = canonicalTextObject(result, expected);
  assert.equal(text.text.kind, expected.math ? "math_typst" : "typst");
  assert.deepEqual(text.object.style.fill, { red: 1, green: 1, blue: 1, alpha: 1 });
  assert.equal(text.object.style.opacity, 1);
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
      name: "noon-retained-text-authoring-smoke",
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
      const error = new Error(event.message || "retained text worker crashed");
      rejectReady(error);
      for (const { reject } of pending.values()) reject(error);
      pending.clear();
    });
    worker.addEventListener("message", (event) => {
      const message = event.data;
      if (message?.channel !== "noon.authoring" || message?.protocolVersion !== 6) {
        const error = new Error("invalid retained text worker envelope");
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
        const error = new Error(String(message.message || "retained text authoring failed"));
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

    window.noonRetainedTextSmoke = {
      ready: () => ready,
      run: async (source) => {
        await ready;
        const requestId = nextRequestId++;
        const result = new Promise((resolve, reject) => pending.set(requestId, { resolve, reject }));
        worker.postMessage({
          channel: "noon.authoring",
          protocolVersion: 6,
          type: "run",
          requestId,
          source,
          context: {},
          exportDocument: true,
        });
        return result;
      },
      stop: () => worker.terminate(),
    };
  });
  await page.evaluate(() => window.noonRetainedTextSmoke.ready());

  const helloText = await page.evaluate(
    (source) => window.noonRetainedTextSmoke.run(source),
    helloTextSource,
  );
  assert.equal(helloText.document.objects.length, 0, "Text must not create placeholder geometry");
  assertNativeText(helloText, {
    source: "Native Noon",
    fontSize: 48,
    order: 0,
    objectId: 0,
  });

  const multilineText = await page.evaluate(
    (source) => window.noonRetainedTextSmoke.run(source),
    multilineTextSource,
  );
  assert.equal(
    multilineText.document.objects.length,
    0,
    "multiline Text must not create placeholder geometry",
  );
  assertNativeText(multilineText, {
    source: "first\nsecond",
    fontSize: 36,
    lineSpacing: 0.5,
    order: 0,
    objectId: 0,
  });

  const nativeLayout = await page.evaluate(
    (source) => window.noonRetainedTextSmoke.run(source),
    nativeTextLayoutSource,
  );
  assert.equal(nativeLayout.document.objects.length, 1, "layout scene must retain only the Square as geometry");
  assert.equal(nativeLayout.document.objects[0].id, 0);
  const nativeLayoutText = assertNativeText(nativeLayout, {
    source: "Native Noon",
    fontSize: 48,
    order: 1,
    objectId: 1,
  });
  assert.ok(
    Math.abs(nativeLayoutText.object.transform.translation.x - 2.25) < 1e-4,
    "Text.next_to must use Rust-owned width/critical-point metrics",
  );
  assert.ok(Math.abs(nativeLayoutText.object.transform.translation.y) < 1e-5);
  assert.ok(Math.abs(nativeLayoutText.object.transform.scale.x - nativeLayoutText.object.transform.scale.y) < 1e-6);
  assert.ok(nativeLayoutText.object.transform.scale.x > 0);

  const helloTypst = await page.evaluate(
    (source) => window.noonRetainedTextSmoke.run(source),
    helloTypstSource,
  );
  assert.equal(helloTypst.document.objects.length, 0, "Typst must not create placeholder geometry");
  assertTypst(helloTypst, {
    source: "*Hello* from _Typst!_",
    math: false,
    fontSize: 48,
    effectiveFontSize: 96,
    order: 0,
    objectId: 0,
  });

  const helloMathTypst = await page.evaluate(
    (source) => window.noonRetainedTextSmoke.run(source),
    helloMathTypstSource,
  );
  assert.equal(
    helloMathTypst.document.objects.length,
    0,
    "MathTypst must not create placeholder geometry",
  );
  assertTypst(helloMathTypst, {
    source: "sum_(k=1)^n k = (n(n + 1)) / 2",
    math: true,
    fontSize: 48,
    effectiveFontSize: 72,
    order: 0,
    objectId: 0,
  });

  const mixed = await page.evaluate(
    (source) => window.noonRetainedTextSmoke.run(source),
    mixedPainterSource,
  );
  assert.equal(mixed.document.objects.length, 2, "only the circle and square belong to legacy geometry");
  assert.equal(mixed.document.objects[0].id, 0);
  assert.equal(mixed.document.objects[1].id, 2);
  assertNativeText(mixed, {
    source: "middle",
    fontSize: 48,
    order: 1,
    objectId: 1,
  });

  await page.evaluate(() => window.noonRetainedTextSmoke.stop());
  assert.deepEqual(errors, [], `browser errors while testing retained text authoring:\n${errors.join("\n")}`);
  console.log(
    "Text authoring smoke passed: native Text layout/placement and pinned Manim v0.21 Typst/MathTypst sources emit canonical source-only mixed content with zero placeholder geometry, exact JS-safe identities, and deterministic mixed painter order.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
