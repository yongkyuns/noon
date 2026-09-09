import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

// Exercise the real enhanced editor without loading Python, WASM or a renderer.
const editorSource = await readFile(new URL("../web/python-editor.js", import.meta.url), "utf8");
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  const errors = [];
  const ruffRequests = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("request", (request) => {
    if (request.url().includes("ruff-wasm")) ruffRequests.push(request.url());
  });
  await page.route("http://editor.test/**", (route) => {
    const isModule = route.request().url().endsWith("/editor.js");
    return route.fulfill({
      contentType: isModule ? "text/javascript" : "text/html",
      body: isModule ? editorSource : `<!doctype html>
        <textarea id="python-scene-source">original</textarea>
        <button id="reset" disabled>Reset</button>
        <script>
          const textarea = document.querySelector("textarea");
          const reset = document.querySelector("button");
          window.observedSources = [];
          textarea.addEventListener("input", () => {
            observedSources.push(textarea.value);
            reset.disabled = textarea.value === "original";
          });
          reset.onclick = () => {
            textarea.value = "original";
            reset.disabled = true;
          };
        </script>
        <script type="module" src="/editor.js"></script>`,
    });
  });
  await page.goto("http://editor.test/");
  await page.waitForSelector(".python-code-editor[data-editor-ready='true']");
  const snapshot = () => page.evaluate(() => ({
    source: document.querySelector("textarea").value,
    observed: window.observedSources.slice(),
    resetDisabled: document.querySelector("button").disabled,
  }));

  // Gallery loads and Reset project source without scheduling a user edit.
  await page.evaluate(() => { document.querySelector("textarea").value = "loaded"; });
  assert.deepEqual(await snapshot(), { source: "loaded", observed: [], resetDisabled: true });
  assert.equal(ruffRequests.length, 0, "programmatic source loads must not activate Ruff");

  const content = page.locator(".cm-content");
  await content.fill("draft");
  assert.deepEqual(await snapshot(), {
    source: "draft", observed: ["draft"], resetDisabled: false,
  }, "the first input must publish the committed document and enable Reset");

  await content.press("End");
  await content.press("x");
  let state = await snapshot();
  assert.equal(state.source, "draftx");
  assert.equal(state.observed.at(-1), "draftx", "keyboard edits must not publish the previous value");

  for (const key of ["ControlOrMeta+z", "ControlOrMeta+Shift+z"]) {
    const previous = state;
    await content.press(key);
    state = await snapshot();
    assert.notEqual(state.source, previous.source, `${key} must change the document`);
    assert.equal(state.observed.length, previous.observed.length + 1);
    assert.equal(state.observed.at(-1), state.source, `${key} must publish its committed source`);
  }
  assert.equal(state.source, "draftx", "redo must restore the edited source");

  await page.locator("#reset").click();
  assert.deepEqual(await snapshot(), {
    source: "original", observed: state.observed, resetDisabled: true,
  }, "Reset must restore source without generating another user edit");
  assert.deepEqual(errors, []);
  console.log("Python editor input smoke passed: committed edits, undo/redo, Reset and source projection.");
} finally {
  await browser.close();
}
