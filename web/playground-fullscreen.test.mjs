import assert from "node:assert/strict";
import test from "node:test";
import { installPreviewFullscreen } from "./playground-fullscreen.js";
import { Element, dom, flush } from "./playground-dom-fixture.mjs";
function fixture() {
  const { preview, document } = dom();
  const button = new Element("button");
  document.fullscreenEnabled = true;
  document.fullscreenElement = null;
  preview.requestFullscreen = async () => {
    document.fullscreenElement = preview;
    document.dispatchEvent(new Event("fullscreenchange"));
  };
  document.exitFullscreen = async () => {
    document.fullscreenElement = null;
    document.dispatchEvent(new Event("fullscreenchange"));
  };
  return { preview, document, button };
}

test("fullscreen state follows browser entry, Escape and button exit", async () => {
  const f = fixture();
  const dispose = installPreviewFullscreen(f.preview, f.button, f.document);
  assert.equal(f.button.disabled, false);
  f.button.dispatchEvent(new Event("click")); await flush();
  assert.equal(f.button.getAttribute("aria-pressed"), "true");
  await f.document.exitFullscreen();
  assert.equal(f.button.textContent, "Fullscreen");
  f.button.dispatchEvent(new Event("click")); await flush();
  f.button.dispatchEvent(new Event("click")); await flush();
  assert.equal(f.document.fullscreenElement, null);
  dispose();
  f.button.dispatchEvent(new Event("click")); await flush();
  assert.equal(f.document.fullscreenElement, null);
});

test("unsupported or denied fullscreen never takes down the preview", async () => {
  const unavailable = fixture(); unavailable.document.fullscreenEnabled = false;
  installPreviewFullscreen(unavailable.preview, unavailable.button, unavailable.document);
  assert.equal(unavailable.button.disabled, true);
  const f = fixture();
  f.preview.requestFullscreen = async () => { throw new Error("permission denied"); };
  installPreviewFullscreen(f.preview, f.button, f.document);
  f.button.dispatchEvent(new Event("click")); await flush();
  assert.equal(f.button.disabled, false);
  assert.equal(f.button.getAttribute("aria-pressed"), "false");
  assert.match(f.button.title, /permission denied/);
});
