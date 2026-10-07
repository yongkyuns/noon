// Qualify effective bounds with the actual compiled Rust owner and Python worker.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import * as wasm from "../web/pkg/noon_web.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
wasm.initSync({ module: await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")) });

function near(actual, expected, label) {
  assert.ok(Number.isFinite(actual) && Math.abs(actual - expected) < 1e-6,
    `${label}: expected ${expected}, got ${actual}`);
}

const store = new wasm.WasmAuthoringStore();
const target = store.createManimRectangle(2, 1);
const tracker = store.createValueTracker(3);
const context = store.createSceneContext();
context.bindMobject("1", target);
context.associateValueTracker(tracker);
context.bindTrackerPosition(target, context.trackerPosition(tracker, 1, 0, 0, 2));
context.beginLiveExecution(1);
const observation = context.queryMobjectLayout(target);
near(target.centerX, 0, "authored center remains independent");
near(observation.centerX, 3, "current center x");
near(observation.centerY, 2, "current center y");

function crossLine(index, expectedStart, expectedEnd) {
  const candidate = observation.beginCrossLine(index, 1.5);
  const line = context.liveCreateManimGeometry(candidate);
  // A debug codec is used only to inspect the retained geometry in this test.
  const geometry = JSON.parse(line.snapshotJson()).geometry.line;
  near(geometry.start.x, expectedStart[0], `line ${index} start x`);
  near(geometry.start.y, expectedStart[1], `line ${index} start y`);
  near(geometry.end.x, expectedEnd[0], `line ${index} end x`);
  near(geometry.end.y, expectedEnd[1], `line ${index} end y`);
  line.free();
}
crossLine(0, [1.5, 2.75], [4.5, 1.25]);
// The first publication must not make the second child use a different view.
crossLine(1, [4.5, 2.75], [1.5, 1.25]);
for (const candidate of [
  observation.beginSurroundingRectangle(0, 0, 0),
  observation.beginBackgroundRectangle(0, 0, 0, 0.75),
]) {
  const matcher = context.liveCreateManimGeometry(candidate);
  near(matcher.centerX, 3, "matcher current center x");
  near(matcher.centerY, 2, "matcher current center y");
  near(matcher.width, 2, "matcher width");
  near(matcher.height, 1, "matcher height");
  matcher.free();
}
observation.free();
context.free();
tracker.free();
target.free();
store.free();
console.log("Rust/WASM effective shape-matcher bounds passed");

// Native signal bindings deliberately distinguish authored from current state;
// this is an ownership regression, not a new Manim parity claim.
const source = `
from noon import *

class CurrentShapeMatcherSmoke(Scene):
    def construct(self):
        tracker = ValueTracker(3)
        target = Rectangle(width=2, height=1)
        self.add(target)
        self.bind_position(target, tracker, direction=RIGHT, offset=(0, 2))
        self.wait(0.1)
        assert abs(target.get_center().x - 3) < 1e-6
        assert abs(target.get_center().y - 2) < 1e-6
        cross = Cross(target, scale_factor=1.5)
        assert isinstance(cross, VGroup) and len(cross) == 2
        expected = [((1.5, 2.75), (4.5, 1.25)), ((4.5, 2.75), (1.5, 1.25))]
        for member, (start, end) in zip(cross, expected):
            assert isinstance(member, Line)
            assert abs(member.get_start().x - start[0]) < 1e-6
            assert abs(member.get_start().y - start[1]) < 1e-6
            assert abs(member.get_end().x - end[0]) < 1e-6
            assert abs(member.get_end().y - end[1]) < 1e-6
        surround = SurroundingRectangle(target, buff=0)
        background = BackgroundRectangle(target, buff=0)
        for matcher in (surround, background):
            assert abs(matcher.get_center().x - 3) < 1e-6
            assert abs(matcher.get_center().y - 2) < 1e-6
            assert abs(matcher.width - 2) < 1e-6
            assert abs(matcher.height - 1) < 1e-6
        family = VGroup(target)
        family_cross = Cross(family)
        assert abs(family_cross[0].get_start().x - 2) < 1e-6
        assert abs(family_cross[0].get_start().y - 2.5) < 1e-6
        assert abs(family_cross[0].get_end().x - 4) < 1e-6
        assert abs(family_cross[0].get_end().y - 1.5) < 1e-6
        self.add(cross, surround, background, family_cross)

        # After a completed authored animation, construction must observe the
        # published effective transform rather than the original declaration.
        animated = Rectangle(width=2, height=1)
        self.add(animated)
        self.play(animated.animate.shift((3, 2, 0)), run_time=0.2, rate_func=linear)
        animated_cross = Cross(animated, scale_factor=1.5)
        for member, (start, end) in zip(animated_cross, expected):
            assert isinstance(member, Line)
            assert abs(member.get_start().x - start[0]) < 1e-6
            assert abs(member.get_start().y - start[1]) < 1e-6
            assert abs(member.get_end().x - end[0]) < 1e-6
            assert abs(member.get_end().y - end[1]) < 1e-6
        self.add(animated_cross)
        self.wait(0.1)
`;

const { default: playwright } = await import("playwright");
const port = 4177;
const url = `http://127.0.0.1:${port}/web/manim-compat-smoke.html`;
const server = spawn("python3", ["-m", "http.server", String(port),
  "--bind", "127.0.0.1", "--directory", root], { stdio: ["ignore", "pipe", "pipe"] });
let serverOutput = "";
let serverError = null;
server.on("error", (error) => { serverError = error; });
server.stdout.on("data", (data) => { serverOutput += data; });
server.stderr.on("data", (data) => { serverOutput += data; });
let browser;
try {
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (serverError) throw serverError;
    if (server.exitCode !== null) throw new Error(`Server exited: ${serverOutput}`);
    try {
      ready = (await fetch(url, { signal: AbortSignal.timeout(1000) })).ok;
    } catch { /* The local HTTP process may still be starting. */ }
    if (ready) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.ok(ready, `Shape matcher test server did not start: ${serverOutput}`);
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true,
    args: ["--disable-dev-shm-usage"] });
  const page = await browser.newPage();
  page.setDefaultTimeout(180_000);
  await page.goto(url);
  await page.waitForFunction(() => typeof window.noonManimCompat?.runLive === "function");
  const result = await page.evaluate((pythonSource) => Promise.race([
    window.noonManimCompat.runLive(pythonSource),
    new Promise((_, reject) => setTimeout(() => reject(new Error("Shape matcher worker timed out")), 180_000)),
  ]), source);
  near(result.duration, 0.4, "normal authored timing including completed animation");
  assert.ok(result.metrics.presentedFrames > 0, "shared renderer must present a frame");
  console.log("Python worker effective leaf/family shape-matcher bounds passed");
} finally {
  if (browser) await browser.close();
  server.kill();
}
