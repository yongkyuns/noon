import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4188;
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
  throw new Error(`PolarPlane smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *
import json
import math
import _manim_semantic_handles as _handles


def assert_close(actual, expected, tolerance=1e-5):
    assert abs(float(actual) - float(expected)) <= tolerance, (actual, expected)


def assert_point_close(actual, expected, tolerance=1e-5):
    assert_close(actual.x, expected.x, tolerance)
    assert_close(actual.y, expected.y, tolerance)


def assert_polar_close(actual, expected, tolerance=1e-5):
    assert_close(actual[0], expected[0], tolerance)
    delta = math.atan2(
        math.sin(float(actual[1]) - float(expected[1])),
        math.cos(float(actual[1]) - float(expected[1])),
    )
    assert_close(delta, 0.0, tolerance)


def snapshot(mobject):
    handle = _handles._handle_for(mobject)
    assert handle is not None
    return json.loads(str(handle.snapshotJson()))


def corner_points(mobject):
    commands = snapshot(mobject)["geometry"]["vector_path"]["commands"]
    points = []
    for command in commands:
        payload = command.get("move_to") or command.get("line_to")
        assert payload is not None, command
        point = payload["to"]
        points.append(Vec2(float(point["x"]), float(point["y"])))
    return points


class RetainedPolarPlaneScene(Scene):
    def construct(self):
        plane = PolarPlane(
            radius_max=2,
            size=8,
            radius_step=1,
            azimuth_step=4,
            azimuth_offset=0.25,
            faded_line_ratio=2,
        )

        assert isinstance(plane, Axes)
        assert type(plane) is PolarPlane
        assert hasattr(Axes, "plot_polar_graph")
        assert len(plane.axes) == 2
        assert len(plane._radial_lines) == 4
        assert len(plane._circles) == 3
        assert len(plane._faded_radial_lines) == 4
        assert len(plane._faded_circles) == 2
        assert len(plane.background_lines) == 7
        assert len(plane.faded_lines) == 6
        assert len(plane.submobjects) == 4

        # size=8 over logical [-2, 2] means two scene units per radius unit.
        expected_background_radii = [0.0, 2.0, 4.0]
        expected_faded_radii = [1.0, 3.0]
        for circle, expected in zip(plane._circles, expected_background_radii):
            assert_close(circle.radius, expected)
        for circle, expected in zip(plane._faded_circles, expected_faded_radii):
            assert_close(circle.radius, expected)

        origin = plane.get_origin()
        first_radial = plane._radial_lines[0]
        assert_point_close(first_radial.get_start(), origin)
        radial_delta = first_radial.get_end() - origin
        assert_close(radial_delta.length(), 4.0)
        assert_close(math.atan2(radial_delta.y, radial_delta.x), 0.25)

        # plot_polar_graph is exactly the upstream composition over pr2pt + ParametricFunction.
        default_radius = lambda theta: 0.75 + 0.25 * math.cos(theta)
        default_graph = plane.plot_polar_graph(
            default_radius,
            use_smoothing=False,
        )
        assert isinstance(default_graph, ParametricFunction)
        assert default_graph.underlying_function is default_radius
        assert_close(default_graph.t_range[0], 0.0)
        assert_close(default_graph.t_range[1], 2.0 * math.pi)
        assert_close(default_graph.t_range[2], 0.01)
        default_points = corner_points(default_graph)
        assert len(default_points) > 600
        assert_point_close(default_points[0], plane.pr2pt(default_radius(0.0), 0.0))
        assert_point_close(
            default_points[-1],
            plane.pr2pt(default_radius(2.0 * math.pi), 2.0 * math.pi),
        )

        # The shared polar helpers are CoordinateSystem behavior, not PolarPlane-only math.
        axes = Axes(
            x_range=[-3, 3, 1],
            y_range=[-3, 3, 1],
            x_length=6,
            y_length=6,
            tips=False,
        )
        assert_point_close(axes.pr2pt(2.0, math.pi / 3.0), axes.c2p(1.0, math.sqrt(3.0)))
        assert_polar_close(axes.pt2pr(axes.pr2pt(1.5, -0.4)), (1.5, -0.4))

        plane.shift(LEFT * 0.6 + UP * 0.4)
        plane.scale(0.8)
        plane.rotate(0.3)
        target = (1.25, -0.7)
        transformed = plane.pr2pt(*target)
        assert_polar_close(plane.pt2pr(transformed), target)

        rose = lambda theta: 1.0 + 0.5 * math.cos(2.0 * theta)
        explicit_graph = plane.plot_polar_graph(
            rose,
            theta_range=[0.0, math.pi, math.pi / 2.0],
            use_smoothing=False,
            color=ORANGE,
            stroke_width=4,
        )
        assert explicit_graph.underlying_function is rose
        assert explicit_graph.t_range == [0.0, math.pi, math.pi / 2.0]
        explicit_points = corner_points(explicit_graph)
        assert len(explicit_points) == 3
        for point, theta in zip(explicit_points, [0.0, math.pi / 2.0, math.pi]):
            assert_point_close(point, plane.pr2pt(rose(theta), theta))
        assert_close(snapshot(explicit_graph)["style"]["stroke_width"], 0.04)

        copied = plane.copy()
        assert type(copied) is PolarPlane
        assert copied is not plane
        assert len(copied._radial_lines) == len(plane._radial_lines)
        assert len(copied._circles) == len(plane._circles)
        assert_point_close(copied.pr2pt(*target), transformed)
        copied.shift(RIGHT)
        assert (copied.pr2pt(*target) - plane.pr2pt(*target)).length() > 0.5
        assert_polar_close(copied.pt2pr(copied.pr2pt(*target)), target)

        no_fade = PolarPlane(radius_max=1, size=2, azimuth_step=4, faded_line_ratio=0)
        ratio_one = PolarPlane(radius_max=1, size=2, azimuth_step=4, faded_line_ratio=1)
        assert len(no_fade._faded_radial_lines) == 0
        assert len(no_fade._faded_circles) == 0
        assert len(no_fade._radial_lines) == len(ratio_one._radial_lines) == 4
        assert len(no_fade._circles) == len(ratio_one._circles) == 2

        partial = PolarPlane(radius_max=1, size=2, azimuth_step=2.5)
        assert len(partial._radial_lines) == 3

        try:
            PolarPlane(azimuth_units="turns")
            raise AssertionError("invalid azimuth units unexpectedly succeeded")
        except ValueError as error:
            assert "Invalid azimuth units" in str(error)

        try:
            PolarPlane(azimuth_direction="SIDEWAYS")
            raise AssertionError("invalid azimuth direction unexpectedly succeeded")
        except ValueError as error:
            assert "CW, CCW" in str(error)

        for operation in (plane.get_coordinate_labels, plane.add_coordinates):
            try:
                operation()
                raise AssertionError("PolarPlane label API unexpectedly succeeded")
            except NotImplementedError as error:
                assert "number/MathTex labels" in str(error)

        self.add(plane, explicit_graph)
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
  assert.equal(result.document.objects.length, 16);
  const kinds = result.document.objects.map((object) => Object.keys(object.geometry)[0]);
  assert.deepEqual(kinds, [
    "line", "line", "line", "line",
    "circle", "circle",
    "line", "line", "line", "line",
    "circle", "circle", "circle",
    "line", "line", "vector_path",
  ]);
  assert.ok(
    kinds.every((kind) => kind === "line" || kind === "circle" || kind === "vector_path"),
    "PolarPlane/plot_polar_graph must lower only to existing retained geometry",
  );
  assert.deepEqual(errors, [], `browser errors while testing retained PolarPlane:\n${errors.join("\n")}`);
  console.log(
    "Retained PolarPlane smoke passed: exact radial/circle subdivision and order, unit-size scaling, retained polar graphs with default/explicit theta ranges, transform-safe polar mapping, copy independence, explicit label deferral, and ordinary retained geometry.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
