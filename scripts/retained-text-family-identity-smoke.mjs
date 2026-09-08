import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const source = `
from noon import *

class RetainedFamilyIdentity(Scene):
    def construct(self):
        first = Text("Family A", font_size=40)
        second = Text("Family B", font_size=40)
        nested = VGroup(first, VGroup(second))

        assert len(nested) == 2
        assert nested[0] is first
        assert len(nested[1]) == 1
        assert nested[1][0] is second

        clone = nested.copy()
        assert len(clone) == 2
        assert clone is not nested
        assert clone[0] is not first
        assert clone[1] is not nested[1]
        assert clone[1][0] is not second

        holder = VGroup(first)
        holder.remove(first)
        assert len(holder) == 0
        holder.add(first)
        assert len(holder) == 1
        assert holder[0] is first
`;

const server = await serveRepository(root, 4193);
let browser;
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.goto(`${server.baseUrl}/web/manim-compat-smoke.html`);
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30000 });
  // runLive owns source attachment; the unrelated animated ready probes need
  // not run again for this static family qualification.
  const result = await page.evaluate(source => window.noonManimCompat.runLive(source), source);
  assert.equal(result.mode, "semantic");
  assert.equal(result.metrics.objectCount, 0);
  assert.equal(result.frame.objects.length, 0);
  assert.equal(result.frame.present_object_count, 0);
  assert.equal(result.metrics.instancesDrawn, 0, "detached family must not synthesize renderer objects");
  assert.deepEqual(errors, []);
  console.log("PASS shared Text family identity: preserved Python assertions and normal retained rendering");
} finally { await browser?.close(); await server.close(); }
