import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = await allocateLoopbackPort();
const baseUrl = `http://127.0.0.1:${port}`;

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function allocateLoopbackPort() {
  return new Promise((resolve, reject) => {
    const probe = net.createServer();
    probe.unref();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      if (address === null || typeof address === "string") {
        probe.close();
        reject(new Error("Unable to allocate a loopback smoke-test port"));
        return;
      }
      probe.close((error) => {
        if (error) reject(error);
        else resolve(address.port);
      });
    });
  });
}

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
  throw new Error(`ImplicitFunction smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *
import json
import math
import _manim_semantic_handles as _handles


def snapshot(mobject):
    handle = _handles._handle_for(mobject)
    assert handle is not None
    return json.loads(str(handle.snapshotJson()))


def path_commands(mobject):
    return snapshot(mobject)["geometry"]["vector_path"]["commands"]


def point_from_command(command):
    payload = command.get("move_to") or command.get("line_to") or command.get("cubic_to")
    assert payload is not None, command
    return payload["to"]


def corner_subpaths(mobject):
    subpaths = []
    active = None
    for command in path_commands(mobject):
        if "move_to" in command:
            active = []
            subpaths.append(active)
        else:
            assert "line_to" in command, command
            assert active is not None
        active.append(point_from_command(command))
    return subpaths


def assert_close(actual, expected, tolerance=1e-5):
    assert abs(float(actual) - float(expected)) <= tolerance, (actual, expected)


class RetainedImplicitFunctionScene(Scene):
    def construct(self):
        assert issubclass(ImplicitFunction, VMobject)
        assert hasattr(Axes, "plot_implicit_curve")

        direct_calls = []
        direct = ImplicitFunction(
            lambda x, y: (direct_calls.append((x, y)), x * x + y * y - 1.0)[1],
            x_range=[-2, 2, 0.25],
            y_range=[-2, 2],
            min_depth=4,
            max_quads=900,
            use_smoothing=False,
            color=YELLOW,
        )
        assert direct.function is not None
        assert direct.x_range == [-2.0, 2.0, 0.25]
        assert direct.y_range == [-2.0, 2.0]
        assert direct.min_depth == 4
        assert direct.max_quads == 900
        assert direct.use_smoothing is False
        assert len(direct_calls) > 100
        direct_subpaths = corner_subpaths(direct)
        assert len(direct_subpaths) == 1
        circle = direct_subpaths[0]
        assert len(circle) > 8
        assert circle[0] == circle[-1]
        for point in circle:
            residual = point["x"] * point["x"] + point["y"] * point["y"] - 1.0
            assert abs(residual) < 0.04, residual

        multi = ImplicitFunction(
            lambda x, y: (x * x + y * y - 0.64) * (x * x + y * y - 2.25),
            x_range=[-2, 2],
            y_range=[-2, 2],
            min_depth=4,
            max_quads=1200,
            use_smoothing=False,
        )
        multi_subpaths = corner_subpaths(multi)
        assert len(multi_subpaths) == 2, len(multi_subpaths)
        assert all(curve[0] == curve[-1] for curve in multi_subpaths)

        smoothed = ImplicitFunction(
            lambda x, y: x * x + y * y - 0.25,
            x_range=[-1, 1],
            y_range=[-1, 1],
            min_depth=3,
            max_quads=300,
            use_smoothing=True,
        )
        smooth_commands = path_commands(smoothed)
        assert any("cubic_to" in command for command in smooth_commands[1:])

        undefined = ImplicitFunction(
            lambda x, y: float("nan"),
            x_range=[-1, 1],
            y_range=[-1, 1],
            min_depth=2,
            max_quads=64,
            use_smoothing=False,
        )
        assert path_commands(undefined) == []

        axes = Axes(
            x_range=[-2, 2, 1],
            y_range=[-2, 2, 1],
            x_length=8,
            y_length=4,
            tips=False,
        )
        axes.shift(RIGHT * 1.1 + UP * 0.4)
        axes.scale(0.8)
        axes.rotate(0.25)
        mapped = axes.plot_implicit_curve(
            lambda x, y: x * x + y * y - 1.0,
            min_depth=3,
            max_quads=500,
            use_smoothing=False,
            color=PURE_GREEN,
        )
        assert isinstance(mapped, ImplicitFunction)
        assert mapped.axes is axes
        mapped_subpaths = corner_subpaths(mapped)
        assert len(mapped_subpaths) == 1
        for point in mapped_subpaths[0]:
            x, y = axes.p2c(Vec2(point["x"], point["y"]))
            assert abs(x * x + y * y - 1.0) < 0.05

        try:
            ImplicitFunction(lambda x, y: x + y, x_range=[1, -1])
            raise AssertionError("descending implicit range unexpectedly succeeded")
        except ValueError:
            pass
        try:
            axes.plot_implicit_curve(lambda x, y: x + y, min_depth=-1)
            raise AssertionError("negative min_depth unexpectedly succeeded")
        except ValueError:
            pass

        self.add(direct, multi, smoothed, undefined, axes, mapped)
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
    source,
  );

  assert.equal(result.kind, "scene_document");
  assert.equal(result.document.tracks.length, 0);
  const geometryKinds = result.document.objects.map((object) => Object.keys(object.geometry)[0]);
  assert.ok(geometryKinds.filter((kind) => kind === "vector_path").length >= 5);
  assert.ok(
    geometryKinds.every((kind) => kind === "line" || kind === "vector_path"),
    `ImplicitFunction scene introduced unexpected geometry: ${geometryKinds.join(", ")}`,
  );
  assert.deepEqual(errors, [], `browser errors while testing retained ImplicitFunction:\n${errors.join("\n")}`);
  console.log(
    "Retained ImplicitFunction smoke passed: adaptive closed/multi contours, shared smoothing, NaN regions, transform-safe Axes mapping, validation, VectorPath-only plots, and zero tracks.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
