import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4193;
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
  throw new Error(`retained family fade smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *

class RetainedFamilyFade(Scene):
    def construct(self):
        first = Text("Family A", font_size=40).shift(LEFT)
        second = Text("Family B", font_size=40).shift(RIGHT)
        family = VGroup(first, VGroup(second))

        mirror = family.copy()
        assert len(mirror) == 2

        detached = Text("Detached", font_size=32)
        holder = VGroup(detached)
        holder.remove(detached)
        holder.add(detached)

        self.wait(0.25)
        assert family not in self.mobjects

        self.play(FadeIn(family), run_time=0.75, rate_func=linear)
        assert family in self.mobjects
        assert first not in self.mobjects
        assert second not in self.mobjects

        self.play(FadeOut(family), run_time=0.5)
        assert family not in self.mobjects
        assert first not in self.mobjects
        assert second not in self.mobjects

        unsupported = VGroup(
            Text("Unsupported A", font_size=32),
            Text("Unsupported B", font_size=32),
        )
        try:
            self.play(FadeIn(unsupported, shift=UP), run_time=0.25)
            raise AssertionError("shifted retained family FadeIn must fail")
        except NotImplementedError as error:
            assert "shared retained family layout semantics" in str(error)
        assert unsupported not in self.mobjects
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
    "retained Text family fades must not create legacy placeholder geometry",
  );
  assert.ok(result.retainedDocument, "family fade must emit retained authoring state");
  assert.deepEqual(
    result.retainedDocument.objects.map((object) => object.text.source),
    ["Family A", "Family B"],
  );

  const tracks = result.retainedDocument.tracks ?? [];
  assert.equal(
    tracks.filter((track) => track.property === "position").length,
    0,
    "default family fades must not synthesize position tracks",
  );
  assert.equal(
    tracks.filter((track) => track.property === "scale").length,
    0,
    "default family fades must not synthesize scale tracks",
  );

  for (const object of result.retainedDocument.objects) {
    const objectTracks = tracks.filter((track) => track.object === object.object);
    const presence = objectTracks.filter((track) => track.property === "presence");
    const appearance = objectTracks.filter((track) => track.property === "appearance");
    assert.deepEqual(
      presence.map((track) => ({
        values: track.values.bool,
        start: track.timing.start_time,
        duration: track.timing.duration,
        easing: track.timing.easing,
      })),
      [
        { values: { from: false, to: true }, start: 0.25, duration: 0, easing: "linear" },
        { values: { from: true, to: false }, start: 1.5, duration: 0, easing: "linear" },
      ],
      `${object.text.source} must use leaf Presence lifecycle`,
    );
    assert.deepEqual(
      appearance.map((track) => ({
        values: track.values.scalar,
        start: track.timing.start_time,
        duration: track.timing.duration,
        easing: track.timing.easing,
      })),
      [
        { values: { from: 0, to: 1 }, start: 0.25, duration: 0.75, easing: "linear" },
        { values: { from: 1, to: 0 }, start: 1, duration: 0.5, easing: "smooth" },
        { values: { from: 0, to: 1 }, start: 1.5, duration: 0, easing: "linear" },
      ],
      `${object.text.source} must use leaf Appearance fade plus cleanup`,
    );
  }

  assert.equal(result.duration, 1.5);
  const wire = JSON.stringify(result.retainedDocument);
  for (const forbidden of ["glyph", "font_bytes", "svg", "geometry", "atlas"]) {
    assert.ok(!wire.includes(forbidden), `retained family fade wire must not contain ${forbidden}`);
  }
  assert.deepEqual(errors, [], `browser errors while testing retained family fades:\n${errors.join("\n")}`);
  console.log(
    "Retained Text family fade smoke passed: nested VGroup identity is shared, default FadeIn/FadeOut lower to leaf Presence/Appearance tracks, wrapper identity stays top-level, and unsupported family layout endpoints fail before mutation.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
