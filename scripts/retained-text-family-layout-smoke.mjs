import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4194;
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
  throw new Error(`retained family layout smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *


def close(actual, expected, label, tolerance=1e-5):
    assert abs(float(actual) - float(expected)) <= tolerance, f"{label}: {actual} != {expected}"


def union_bounds(*members):
    return (
        min(member.get_critical_point(LEFT).x for member in members),
        min(member.get_critical_point(DOWN).y for member in members),
        max(member.get_critical_point(RIGHT).x for member in members),
        max(member.get_critical_point(UP).y for member in members),
    )


class RetainedFamilyLayout(Scene):
    def construct(self):
        first = Text("Layout A", font_size=40).shift(LEFT * 1.75 + UP * 0.4)
        second = Text("Layout BBB", font_size=32).shift(RIGHT * 1.25 + DOWN * 0.3)
        family = VGroup(first, VGroup(second))

        min_x, min_y, max_x, max_y = union_bounds(first, second)
        close(family.width, max_x - min_x, "nested retained family width")
        close(family.height, max_y - min_y, "nested retained family height")
        center = family.get_center()
        close(center.x, (min_x + max_x) * 0.5, "nested retained family center x")
        close(center.y, (min_y + max_y) * 0.5, "nested retained family center y")

        first_before = first.get_center()
        second_before = second.get_center()
        delta = RIGHT * 0.75 + UP * 0.55
        family.shift(delta)
        close(first.get_center().x, first_before.x + delta.x, "family shift first x")
        close(first.get_center().y, first_before.y + delta.y, "family shift first y")
        close(second.get_center().x, second_before.x + delta.x, "family shift second x")
        close(second.get_center().y, second_before.y + delta.y, "family shift second y")

        family.center()
        centered = family.get_center()
        close(centered.x, 0.0, "family center x")
        close(centered.y, 0.0, "family center y")

        arranged_a = Text("A", font_size=36)
        arranged_b = Text("BBBB", font_size=36)
        arranged = VGroup(arranged_a, VGroup(arranged_b))
        arranged.arrange(RIGHT, buff=0.375, center=True)
        gap = arranged_b.get_critical_point(LEFT).x - arranged_a.get_critical_point(RIGHT).x
        close(gap, 0.375, "nested retained arrange gap")
        arranged_center = arranged.get_center()
        close(arranged_center.x, 0.0, "arranged family center x")
        close(arranged_center.y, 0.0, "arranged family center y")

        square = Square(side_length=1.25).shift(LEFT * 0.9)
        mixed_text = Text("Mixed", font_size=30).shift(RIGHT * 0.8)
        mixed = VGroup(square, mixed_text)
        min_x, min_y, max_x, max_y = union_bounds(square, mixed_text)
        close(mixed.width, max_x - min_x, "mixed family width")
        close(mixed.height, max_y - min_y, "mixed family height")
        mixed_center = mixed.get_center()
        close(mixed_center.x, (min_x + max_x) * 0.5, "mixed family center x")
        close(mixed_center.y, (min_y + max_y) * 0.5, "mixed family center y")

        square_before = square.get_center()
        mixed_text_before = mixed_text.get_center()
        mixed.shift(DOWN * 0.6)
        close(square.get_center().y, square_before.y - 0.6, "mixed family square shift")
        close(mixed_text.get_center().y, mixed_text_before.y - 0.6, "mixed family text shift")

        typst_family = VGroup(Typst("*Typst*", font_size=36))
        try:
            _ = typst_family.width
            raise AssertionError("Typst family layout must remain explicit until Rust-owned Typst bounds exist")
        except NotImplementedError as error:
            assert "Typst/MathTypst family layout" in str(error)

        self.add(first, second, arranged_a, arranged_b, mixed_text)
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

  const result = await page.evaluate((python) => window.noonManimCompat.run(python), source);
  assert.equal(result.kind, "scene_document");
  assert.equal(
    result.document.objects.length,
    0,
    "retained family layout must not synthesize legacy placeholder geometry",
  );
  assert.ok(result.retainedDocument, "retained family layout scene must emit retained state");
  assert.deepEqual(
    result.retainedDocument.objects.map((object) => object.text.source),
    ["Layout A", "Layout BBB", "A", "BBBB", "Mixed"],
  );
  const wire = JSON.stringify(result.retainedDocument);
  for (const forbidden of ["glyph", "font_bytes", "svg", "geometry", "atlas"]) {
    assert.ok(!wire.includes(forbidden), `retained family layout wire must not contain ${forbidden}`);
  }
  assert.deepEqual(
    errors,
    [],
    `browser errors while testing retained Text family layout:\n${errors.join("\n")}`,
  );
  console.log(
    "Retained Text family layout smoke passed: shared Rust family bounds, nested arrange, mixed geometry/Text translation, and explicit Typst layout debt all hold without legacy Text geometry.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
