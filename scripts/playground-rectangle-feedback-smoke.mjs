import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { playgroundLaunchOptions } from "./playground-browser-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { inspectCapturedPng } from "./agent-preview-artifacts.mjs";
import { runFeedbackCli } from "../tools/noon-mcp/src/feedback-cli.mjs";
import { runPreviewCli } from "../tools/noon-mcp/src/preview-cli.mjs";
import { AgentPreviewService } from "../tools/noon-mcp/src/preview-service.mjs";

const root = path.resolve(fileURLToPath(new URL("../", import.meta.url)));
const output = path.resolve(root, process.env.NOON_FEEDBACK_ARTIFACTS ?? "browser-smoke-artifacts/rectangle-feedback");
const example = "showcase-cuda-transpose";
const target = 1.25;
const comment = "Brighten the CUDA subtitle so it is easier to read. Keep its position and the surrounding labels.";

function highlightBrightness(image, region) {
  const levels = [];
  for (let y = Math.ceil(region.y * image.height); y < (region.y + region.height) * image.height; y++) {
    for (let x = Math.ceil(region.x * image.width); x < (region.x + region.width) * image.width; x++) {
      const i = 4 * (y * image.width + x);
      levels.push((image.data[i] + image.data[i + 1] + image.data[i + 2]) / 3);
    }
  }
  levels.sort((a, b) => a - b);
  return levels[Math.floor(levels.length * 0.99)];
}

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
  // Drag backwards to prove the exported region stays aligned to the image.
  await page.mouse.move(bounds.x + bounds.width * 0.82, bounds.y + bounds.height * 0.135);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width * 0.18, bounds.y + bounds.height * 0.08, { steps: 5 });
  await page.mouse.up();
  const field = panel.getByRole("textbox", { name: "Comment" });
  assert.equal(await field.evaluate(text => text === document.activeElement), true);
  await field.fill(comment);
  await page.screenshot({ path: path.join(output, "selection.png") });
  const pending = page.waitForEvent("download");
  await panel.getByRole("button", { name: "Download evidence" }).click();
  const download = await pending;
  await download.saveAs(path.join(output, "viewer-feedback.json"));
  await panel.getByRole("button", { name: "Add to source" }).click();
  await panel.waitFor({ state: "hidden" });
  await stage.waitFor({ state: "hidden" });
  const annotated = await page.locator("#python-scene-source").inputValue();
  assert.ok(annotated.startsWith(draft), "feedback must preserve unapplied editor changes");
  assert.ok(annotated.includes(`# Noon feedback (pending) at ${target} s\n# Rectangle`));
  assert.ok(annotated.includes(`# ${comment}`));
  const note = annotated.slice(draft.length).trim();
  assert.ok(note.split("\n").every(line => line.startsWith("#")));
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
  assert.deepEqual(errors, []);

  const imported = await runFeedbackCli({ cwd: output, argv: ["import-live", "--input", "viewer-feedback.json", "--output", "review"] });
  const { feedback, frame } = imported.feedback;
  assert.equal(feedback.observation, comment);
  const expectedRegion = { x: 0.18, y: 0.08, width: 0.64, height: 0.055 };
  for (const [key, expected] of Object.entries(expectedRegion)) {
    assert.ok(Math.abs(feedback.region[key] - expected) < 0.01, `${key} misaligned: ${feedback.region[key]}`);
  }
  assert.ok(Math.abs(frame.provenance.publishedTime - target) < 0.02);
  assert.equal(await readFile(path.join(output, "review/source.txt"), "utf8"), original);
  const picture = PNG.sync.read(await readFile(path.join(output, "review/frame.png")));
  const pixels = new Set();
  for (let i = 0; i < picture.data.length; i += 16) pixels.add(picture.data.readUInt32BE(i));
  assert.ok(pixels.size > 100, "the captured lesson must contain actual drawn pixels");
  await writeFile(path.join(output, "original.py"), original);
  console.log(`Live rectangle saved in the scene source and independently exported at ${frame.provenance.publishedTime}s.`);

  const check = source => runFeedbackCli({ cwd: output, argv: ["check", "--feedback", "review/feedback.json", "--source", source] });
  assert.equal((await check("original.py")).sourceStatus, "current");
  // Apply the source comment's contrast correction to the current scene draft.
  const labelStyle = "y=3.14, size=12, color=MUTED)";
  const revised = annotated.replace(labelStyle, "y=3.14, size=12, color=INK)")
    .replace("# Unapplied editorial note", `# Unapplied editorial note\n\nfeedback_example = """\n\n${note}\n\n"""`);
  assert.notEqual(revised, annotated, "lesson subtitle changed before this revision fixture");
  await writeFile(path.join(output, "revised.py"), revised);
  assert.equal((await check("revised.py")).sourceStatus, "stale");

  // Use the same forward-sampling host and artifact pipeline as noon-preview.
  // This adapter renders only the checked-in example and the bounded edit above.
  const preview = await context.newPage();
  await preview.goto(`${server.baseUrl}/web/agent-preview-host.html`);
  await preview.waitForFunction(() => window.noonAgentPreviewHost !== undefined);
  let latestSnapshot;
  async function render(method, args) {
    const snapshot = await preview.evaluate(({ method, args }) => window.noonAgentPreviewHost[method](...args), { method, args });
    const imageData = await preview.locator("#scene").screenshot({ scale: "css" });
    const { width, height, byteLength, sha256 } = inspectCapturedPng(imageData);
    latestSnapshot = { ...snapshot, imageData, image: { mimeType: "image/png", width, height, encodedBytes: byteLength, sha256 } };
    return latestSnapshot;
  }
  const time = frame.provenance.publishedTime;
  const candidate = await runPreviewCli({ cwd: output,
    argv: ["--source", "revised.py", "--output", "candidate", "--time", String(time)],
    serviceFactory: () => new AgentPreviewService({ createSession: () => ({
      get snapshot() { return latestSnapshot; },
      open: (source, { loopDurationSeconds }) => render("open", [source, loopDurationSeconds]),
      sample: sampleTime => render("sample", [sampleTime]),
      close: reason => preview.evaluate(reason => window.noonAgentPreviewHost.close(reason), reason),
    }) }),
  });
  const { comparison, viewerPath } = await runFeedbackCli({ cwd: output,
    argv: ["compare", "--feedback", "review/feedback.json", "--candidate", candidate.manifestPath,
      "--sample", "1", "--source", "revised.py", "--output", "comparison", "--viewer"],
  });
  assert.equal(comparison.status, "pending_review");
  assert.equal(comparison.before.frame.provenance.publishedTime, time);
  assert.equal(comparison.after.provenance.publishedTime, time);
  assert.equal(comparison.after.provenance.backend, comparison.before.frame.provenance.backend);
  const revisedPicture = PNG.sync.read(await readFile(path.join(output, "comparison/after.png")));
  assert.ok(highlightBrightness(revisedPicture, feedback.region) > highlightBrightness(picture, feedback.region) + 35,
    "the marked subtitle must be visibly brighter after the revision");
  const review = page.getByRole("button", { name: "Review correction" });
  const accept = panel.getByRole("button", { name: "Accept correction" });
  const comparisonFile = panel.getByLabel("Comparison file");
  await review.click();
  await comparisonFile.setInputFiles(viewerPath);
  await panel.getByText("The correction was rendered from a different source.", { exact: false }).waitFor();
  assert.equal(await accept.isDisabled(), true);
  assert.equal(await page.locator("#python-scene-source").inputValue(), annotated);
  await panel.getByRole("button", { name: "Cancel" }).click();

  // Load the agent's exact revised draft, then review the comparison inline.
  await page.locator("#auto-run").uncheck();
  await editor.focus();
  await editor.press("ControlOrMeta+A");
  await page.keyboard.insertText(revised);
  assert.equal(await page.locator("#python-scene-source").inputValue(), revised);
  await page.locator("#auto-run").evaluate(input => { input.checked = true; });
  await review.click();
  await comparisonFile.setInputFiles(viewerPath);
  await stage.getByAltText("Original frame").waitFor();
  await stage.getByAltText("Revised frame").waitFor();
  assert.equal(await page.getByRole("dialog").count(), 0);
  assert.equal(await page.locator("#python-scene-source").inputValue(), revised);
  await page.screenshot({ path: path.join(output, "comparison", "playground-review.png") });
  await accept.click();
  await panel.waitFor({ state: "hidden" });
  const pendingHeader = "# Noon feedback (pending)";
  const commentStart = revised.lastIndexOf(pendingHeader);
  assert.ok(commentStart > revised.indexOf(pendingHeader), "fixture must include a quoted copy of the note");
  assert.equal(await page.locator("#python-scene-source").inputValue(),
    revised.slice(0, commentStart) + revised.slice(commentStart).replace(pendingHeader, "# Noon feedback (resolved)"));
  assert.equal(await page.evaluate(() => window.__noonExampleGallery.generationDiagnostics.runGeneration), runGeneration,
    "saving or resolving feedback must leave Auto-run idle");
  assert.deepEqual(errors, []);
  console.log("Reviewed the revised subtitle and resolved its comment in the Python draft.");
} finally {
  await browser?.close();
  await server.close();
}
