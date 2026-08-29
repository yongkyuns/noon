import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4186;
const baseUrl = `http://127.0.0.1:${port}`;

const pythonSource = `
from noon import *

class PolygramAdapters(Scene):
    def construct(self):
        polygon = Polygon((-1.0, -1.0, 0.0), (2.0, -1.0, 0.0), (0.0, 3.0, 0.0)).shift(RIGHT)
        vertices = polygon.get_vertices()
        assert len(vertices) == 3
        assert abs(vertices[0][0] - 0.0) < 1e-6
        assert abs(vertices[0][1] + 1.0) < 1e-6
        assert abs(vertices[2][0] - 1.0) < 1e-6
        assert abs(vertices[2][1] - 3.0) < 1e-6

        regular = RegularPolygon(4, radius=2.0, start_angle=0.0)
        regular_vertices = regular.get_vertices()
        assert len(regular_vertices) == 4
        assert abs(regular_vertices[0][0] - 2.0) < 1e-6
        assert abs(regular_vertices[0][1]) < 1e-6

        disconnected = RegularPolygram(6, density=2)
        groups = disconnected.get_vertex_groups()
        assert [len(group) for group in groups] == [3, 3]

        triangle = Triangle()
        assert len(triangle.get_vertices()) == 3
        assert isinstance(triangle, RegularPolygon)
        assert isinstance(triangle, Polygram)

        star = Star(n=5, outer_radius=1.5, density=2)
        assert len(star.get_vertices()) == 10
        assert isinstance(star, Polygon)

        explicit = Polygram(
            ((0.0, 2.0, 0.0), (-1.0, -1.0, 0.0), (1.0, -1.0, 0.0)),
            ((0.0, -2.0, 0.0), (-1.0, 1.0, 0.0), (1.0, 1.0, 0.0)),
        )
        assert [len(group) for group in explicit.get_vertex_groups()] == [3, 3]

        self.add(polygon, regular, disconnected, triangle, star, explicit)
`;

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
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Polygon adapter smoke server did not start: ${lastError}\n${serverOutput}`);
}

function assertClose(actual, expected, message) {
  assert.ok(Math.abs(actual - expected) <= 1e-6, `${message}: ${actual} != ${expected}`);
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

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());

  const python = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    pythonSource,
  );
  assert.equal(python.kind, "scene_document");
  assert.equal(python.document.objects.length, 6);

  const javascript = await page.evaluate(async () => {
    const wasm = await import("/web/pkg/noon_web.js");
    await wasm.default();
    const store = new wasm.WasmAuthoringStore();

    const readGroups = (handle) => {
      const encoded = handle.manimVertexGroups();
      try {
        return {
          coordinates: Array.from(encoded.coordinates()),
          lengths: Array.from(encoded.groupLengths()),
        };
      } finally {
        encoded.free();
      }
    };

    const polygon = store.createManimPolygon(
      new Float64Array([-1, -1, 2, -1, 0, 3]),
    );
    polygon.shift(1, 0);

    const regular = store.createManimRegularPolygon(4, 2, 0);
    const disconnected = store.createManimRegularPolygram(6, 2, 1, undefined);
    const triangle = store.createManimTriangle();
    const star = store.createManimStar(5, 1.5, undefined, 2, Math.PI / 2);
    const explicit = store.createManimPolygram(
      new Float64Array([0, 2, -1, -1, 1, -1, 0, -2, -1, 1, 1, 1]),
      new Uint32Array([3, 3]),
    );

    let rejectedInvalidDensity = false;
    try {
      store.createManimRegularPolygram(5, 0, 1, undefined);
    } catch {
      rejectedInvalidDensity = true;
    }

    return {
      polygon: readGroups(polygon),
      regular: readGroups(regular),
      disconnected: readGroups(disconnected),
      triangle: readGroups(triangle),
      star: readGroups(star),
      explicit: readGroups(explicit),
      rejectedInvalidDensity,
    };
  });

  assert.deepEqual(javascript.polygon.lengths, [3]);
  assert.deepEqual(javascript.polygon.coordinates, [0, -1, 3, -1, 1, 3]);
  assert.deepEqual(javascript.regular.lengths, [4]);
  assertClose(javascript.regular.coordinates[0], 2, "regular polygon first x");
  assertClose(javascript.regular.coordinates[1], 0, "regular polygon first y");
  assert.deepEqual(javascript.disconnected.lengths, [3, 3]);
  assert.deepEqual(javascript.triangle.lengths, [3]);
  assert.deepEqual(javascript.star.lengths, [10]);
  assert.deepEqual(javascript.explicit.lengths, [3, 3]);
  assert.equal(javascript.rejectedInvalidDensity, true);
  assert.equal(errors.length, 0, errors.join("\n"));

  console.log("Shared Python/JavaScript polygon and polygram adapter smoke passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
