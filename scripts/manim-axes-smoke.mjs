import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4185;
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
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Axes smoke server did not start: ${lastError}\n${serverOutput}`);
}

const axesSource = `
from noon import *
import math

class RetainedAxesPlot(Scene):
    def construct(self):
        axes = Axes(
            x_range=[-10, 10.3, 1],
            y_range=[-1.5, 1.5, 1],
            x_length=10,
            y_length=6,
            axis_config={"color": GREEN},
            x_axis_config={
                "numbers_with_elongated_ticks": [-10, -8, -6, -4, -2, 0, 2, 4, 6, 8, 10],
            },
            tips=False,
        )
        assert len(axes.x_axis) == 21
        assert len(axes.y_axis) == 3
        assert len(axes.get_axes()) == 2

        point = axes.c2p(3.25, -0.75)
        recovered = axes.p2c(point)
        assert abs(recovered[0] - 3.25) < 1e-5
        assert abs(recovered[1] + 0.75) < 1e-5

        probe_calls = []
        axes.plot(
            lambda x: (probe_calls.append(x), x * x)[1],
            x_range=[-1, 1, 1],
            use_smoothing=False,
        )
        assert probe_calls == [-1.0, 0.0, 1.0]

        sin_graph = axes.plot(lambda x: math.sin(x), color=BLUE)
        cos_graph = axes.plot(lambda x: math.cos(x), color=RED)
        assert isinstance(sin_graph, ParametricFunction)
        assert isinstance(cos_graph, ParametricFunction)

        try:
            axes.shift(RIGHT)
        except NotImplementedError as error:
            assert "shared affine coordinate-state synchronization" in str(error)
        else:
            raise AssertionError("Axes.shift must not desynchronize c2p from visible geometry")

        self.add(axes, sin_graph, cos_graph)
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

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());

  const result = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    axesSource,
  );
  assert.equal(result.kind, "scene_document");
  assert.equal(result.document.objects.length, 26);
  assert.equal(result.document.tracks.length, 0);

  const geometryKinds = result.document.objects.map(
    (object) => Object.keys(object.geometry)[0],
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "line").length,
    24,
    "Axes must flatten to retained line/tick children",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "vector_path").length,
    2,
    "Axes.plot must lower to ordinary retained VectorPath curves",
  );
  assert.deepEqual(errors, [], `browser errors while testing retained Axes:\n${errors.join("\n")}`);
  console.log(
    "Retained Axes smoke passed: shared line/tick family, c2p/p2c, one host evaluation per Rust sample, VectorPath plots, and explicit transform boundary.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
