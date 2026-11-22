import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { playgroundLaunchOptions } from "./playground-browser-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(fileURLToPath(new URL("../", import.meta.url)));
const output = path.resolve(root, process.env.NOON_FEEDBACK_ARTIFACTS ?? "browser-smoke-artifacts/rectangle-feedback");
const example = "showcase-cuda-transpose";
const target = 1.25;
const comment = "Brighten the CUDA subtitle so it is easier to read. Keep its position and the surrounding labels.";

await mkdir(output, { recursive: true });
const server = await serveRepository(root, 0);
let browser;
try {
  browser = await chromium.launch(playgroundLaunchOptions("chromium"));
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
  await cache.install(context);
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  const response = await page.goto(`${server.baseUrl}/web/index.html?catalog=showcase&example=${example}`);
  assert.ok(response?.ok(), "playground did not load");
  await page.waitForFunction(id => {
    const patch = document.querySelector("#patch-status");
    if (patch?.dataset.state === "error") throw new Error(patch.value);
    return patch?.dataset.state === "applied" && patch.dataset.exampleId === id &&
      window.__noonExampleGallery?.runInFlight === false;
  }, example, { timeout: 120_000 });
  console.log("CUDA lesson loaded in the real playground.");

  const controls = page.locator(".playback-controls");
  if (await controls.getAttribute("data-playing") === "true") await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls");
    return controls?.dataset.playing === "false" && controls.dataset.busy === "false";
  });
  await page.locator(".playback-scrubber").evaluate((input, time) => {
    input.value = String(time);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, target);
  await page.waitForFunction(time => {
    const controls = document.querySelector(".playback-controls");
    return controls?.dataset.busy === "false" && Math.abs(Number(controls.dataset.elapsedSeconds) - time) < 0.02;
  }, target);

  const original = await page.locator("#python-scene-source").inputValue();
  await page.locator("#auto-run").uncheck();
  const editor = page.locator("#scene-editor-panel .cm-content");
  await editor.click();
  await editor.press("ControlOrMeta+End");
  await page.keyboard.insertText("\n# Unapplied editorial note\n");
  const draft = await page.locator("#python-scene-source").inputValue();
  assert.notEqual(draft, original);
  const runGeneration = await page.evaluate(() => window.__noonExampleGallery.generationDiagnostics.runGeneration);
  await page.locator("#auto-run").evaluate(input => { input.checked = true; });

  await page.locator("#preview-comment").click();
  const panel = page.getByRole("region", { name: "Frame feedback" });
  const stage = page.locator(".canvas-frame .noon-feedback-image");
  const capturedImage = stage.getByAltText("Frozen frame from the displayed Noon Run");
  await panel.waitFor();
  await page.waitForFunction(() => {
    const status = document.querySelector(".noon-feedback-status");
    if (status?.dataset.error === "true") throw new Error(status.textContent);
    return status?.textContent.startsWith("Paused at ");
  });
  assert.equal(await page.getByRole("dialog").count(), 0);
  const bounds = await capturedImage.evaluate(image => {
    const box = image.getBoundingClientRect();
    const scale = Math.min(box.width / image.naturalWidth, box.height / image.naturalHeight);
    const width = scale * image.naturalWidth, height = scale * image.naturalHeight;
    return { x: box.left + (box.width - width) / 2, y: box.top + (box.height - height) / 2, width, height };
  });
  await page.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  assert.equal(await stage.locator(".noon-feedback-mark").getAttribute("data-shape"), "point");
  // Drag backwards to check that the source coordinates stay aligned to the image.
  await page.mouse.move(bounds.x + bounds.width * 0.82, bounds.y + bounds.height * 0.135);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width * 0.18, bounds.y + bounds.height * 0.08, { steps: 5 });
  await page.mouse.up();
  const field = panel.getByRole("textbox", { name: "Comment" });
  assert.equal(await field.evaluate(text => text === document.activeElement), true);
  await field.fill(comment);
  await page.screenshot({ path: path.join(output, "selection.png") });
  assert.deepEqual(await panel.getByRole("button").allTextContents(), ["Cancel", "Add to source"]);
  await panel.getByRole("button", { name: "Add to source" }).click();
  await panel.waitFor({ state: "hidden" });
  await stage.waitFor({ state: "hidden" });
  const annotated = await page.locator("#python-scene-source").inputValue();
  assert.ok(annotated.startsWith(draft), "feedback must preserve unapplied editor changes");
  assert.ok(annotated.includes(`# Noon feedback at ${target} s\n# Rectangle`));
  assert.ok(annotated.includes(`# ${comment}`));
  const note = annotated.slice(draft.length).trim();
  assert.ok(note.split("\n").every(line => line.startsWith("#")));
  const sourceHash = createHash("sha256").update(original).digest("hex");
  assert.ok(note.includes(`# Displayed source SHA-256 ${sourceHash}`));
  const coordinates = /# Rectangle on the frame \(normalized\) x=([\d.]+) y=([\d.]+) width=([\d.]+) height=([\d.]+)/.exec(note);
  assert.ok(coordinates, "missing rectangle coordinates");
  const expected = [0.18, 0.08, 0.64, 0.055];
  assert.ok(expected.every((value, i) => Math.abs(Number(coordinates[i + 1]) - value) < 0.01),
    `rectangle misaligned in ${coordinates[0]}`);
  const subtitleLine = original.split("\n").findIndex(line => line.includes('meta = label("CUDA shared-memory transpose')) + 1;
  const hints = note.split("\n").filter(line => line.startsWith("# Object hint "));
  assert.ok(hints.length <= 4);
  assert.ok(hints.some(line => line.includes('Text("CUDA shared-memory transpose') &&
    line.includes(`Python line ${subtitleLine}`)), `missing subtitle source hint in ${note}`);
  const pendingSource = page.waitForEvent("download");
  await page.getByRole("button", { name: "Save .py" }).click();
  await (await pendingSource).saveAs(path.join(output, "annotated.py"));
  assert.equal(await readFile(path.join(output, "annotated.py"), "utf8"), annotated);
  assert.equal(await capturedImage.getAttribute("src"), null);
  assert.equal(await controls.getAttribute("data-playing"), "false");
  assert.equal(await page.getByRole("button", { name: "Review correction" }).count(), 0);
  assert.equal(await page.evaluate(() => window.__noonExampleGallery.generationDiagnostics.runGeneration), runGeneration,
    "adding and saving feedback must leave Auto-run idle");
  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "true");
  assert.deepEqual(errors, []);
  console.log(`Saved the rectangle comment in the Python source at ${target} s and resumed playback.`);
} finally {
  await browser?.close();
  await server.close();
}
