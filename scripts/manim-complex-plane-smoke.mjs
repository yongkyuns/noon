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
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`ComplexPlane smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *


def assert_close(actual, expected, tolerance=1e-5):
    assert abs(float(actual) - float(expected)) <= tolerance, (actual, expected)


def assert_point_close(actual, expected, tolerance=1e-5):
    assert_close(actual.x, expected.x, tolerance)
    assert_close(actual.y, expected.y, tolerance)


def assert_complex_close(actual, expected, tolerance=1e-5):
    assert_close(actual.real, expected.real, tolerance)
    assert_close(actual.imag, expected.imag, tolerance)


class RetainedComplexPlaneScene(Scene):
    def construct(self):
        plane = ComplexPlane(
            x_range=[-3, 3, 1],
            y_range=[-2, 2, 1],
            x_length=6,
            y_length=4,
        )
        reference = NumberPlane(
            x_range=[-3, 3, 1],
            y_range=[-2, 2, 1],
            x_length=6,
            y_length=4,
        )

        assert isinstance(plane, NumberPlane)
        assert type(plane) is ComplexPlane
        assert len(plane.submobjects) == len(reference.submobjects)
        assert len(plane.background_lines) == len(reference.background_lines)
        assert len(plane.faded_lines) == len(reference.faded_lines)

        for value in (2, -1.5, 2 + 1j, -2.25 - 1.5j):
            z = complex(value)
            assert_point_close(plane.number_to_point(value), plane.coords_to_point(z.real, z.imag))
            assert_point_close(plane.n2p(value), plane.c2p(z.real, z.imag))
            assert_complex_close(plane.point_to_number(plane.n2p(value)), z)
            assert_complex_close(plane.p2n(plane.n2p(value)), z)

        plane.shift(LEFT * 0.6 + UP * 0.4)
        plane.scale(0.8)
        plane.rotate(0.3)
        target = -1.25 + 0.75j
        transformed = plane.n2p(target)
        assert_complex_close(plane.p2n(transformed), target)
        assert_point_close(transformed, plane.c2p(target.real, target.imag))

        copied = plane.copy()
        assert type(copied) is ComplexPlane
        assert copied is not plane
        assert_point_close(copied.n2p(target), transformed)
        copied.shift(RIGHT)
        assert (copied.n2p(target) - plane.n2p(target)).length() > 0.5
        assert_complex_close(copied.p2n(copied.n2p(target)), target)

        for operation in (plane.get_coordinate_labels, plane.add_coordinates):
            try:
                operation()
                raise AssertionError("ComplexPlane label API unexpectedly succeeded")
            except NotImplementedError as error:
                assert "number/MathTex labels" in str(error)

        self.add(plane, copied)
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
  assert.ok(result.document.objects.length > 0);
  assert.ok(
    result.document.objects.every((object) => Object.keys(object.geometry)[0] === "line"),
    "ComplexPlane must reuse ordinary retained NumberPlane Line geometry only",
  );
  assert.deepEqual(errors, [], `browser errors while testing retained ComplexPlane:\n${errors.join("\n")}`);
  console.log(
    "Retained ComplexPlane smoke passed: NumberPlane identity, scalar complex mapping, transform-safe round trips, copy independence, explicit label deferral, and ordinary retained Line geometry.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
