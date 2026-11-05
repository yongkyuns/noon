// Browser UI automation only. The production player still owns playback and seek.
import assert from "node:assert/strict";
import { waitForBrowserObservation } from "./playground-browser-support.mjs";

export async function waitForPublishedGalleryFrame(page, targetTime, storyboardDuration) {
  assert.ok(Number.isFinite(targetTime) && targetTime >= 0);
  assert.ok(Number.isFinite(storyboardDuration) && storyboardDuration > 0);
  const tolerance = 8 * Number.EPSILON * Math.max(1, storyboardDuration);
  await waitForBrowserObservation(page, async ({ targetTime, tolerance }) => {
    const gallery = window.__noonExampleGallery;
    if (!gallery) return false;
    const report = await gallery.executionMetrics();
    const time = Number(report?.metrics?.time);
    return Number.isFinite(time) && Math.abs(time - targetTime) <= tolerance;
  }, { targetTime, tolerance });
  return page.evaluate(() => window.__noonExampleGallery.executionMetrics());
}

export async function seekPausedGallery(page, storyboardDuration, sampleTime = null, trace = null) {
  assert.ok(Number.isFinite(storyboardDuration) && storyboardDuration > 0);
  assert.ok(sampleTime === null || (Number.isFinite(sampleTime) && sampleTime >= 0 && sampleTime <= storyboardDuration),
    "requested checkpoint is outside the storyboard");
  const record = event => { try { trace?.(event); } catch { /* diagnostics cannot alter the controls probe */ } };
  const waitReady = async (expectedTime = null, paused = false, stage = "controls-ready") => {
    record({ kind: "wait-entered", stage, expectedTime, paused });
    try {
      await page.waitForFunction(({ expectedTime, paused }) => {
        const patch = document.querySelector("#patch-status");
        const controls = document.querySelector(".playback-controls");
        if (patch?.dataset.state === "error") throw new Error(patch.value || "Gallery execution failed");
        if (!controls) return false;
        if (controls.dataset.controllable === "false") {
          throw new Error(controls.title || "Gallery replay is unavailable");
        }
        const toggle = controls.querySelector(".playback-toggle");
        return controls.dataset.busy === "false" &&
          (!paused || (controls.dataset.playing === "false" &&
            toggle?.getAttribute("aria-label") === "Play animation")) &&
          (expectedTime === null || Math.abs(Number(controls.dataset.elapsedSeconds) - expectedTime) < 1e-7);
      }, { expectedTime, paused }, { polling: 50 });
      record({ kind: "wait-resolved", stage });
    } catch (error) {
      record({ kind: "wait-rejected", stage,
        error: { name: error?.name ?? "Error", message: String(error).slice(0, 500) } });
      throw error;
    }
  };

  await waitReady(null, false, "initial-controls-ready");
  const pause = page.getByRole("button", { name: "Pause animation", exact: true });
  record({ kind: "pause-count-started" });
  const pauseCount = await pause.count();
  record({ kind: "pause-count-resolved", count: pauseCount });
  if (pauseCount) {
    record({ kind: "pause-click-started" });
    try {
      await pause.click();
      record({ kind: "pause-click-resolved" });
    } catch (error) {
      record({ kind: "pause-click-rejected",
        error: { name: error?.name ?? "Error", message: String(error).slice(0, 500) } });
      throw error;
    }
  } else if (trace) {
    // Capture one bounded DOM/accessibility-adjacent snapshot only on the
    // anomalous zero-match path; leave the original paused wait/assertions intact.
    record({ kind: "pause-dom-snapshot-started" });
    void Promise.race([
      page.evaluate(() => {
        const style = element => {
          if (!element) return null;
          const css = getComputedStyle(element), rect = element.getBoundingClientRect();
          return { display: css.display, visibility: css.visibility, opacity: css.opacity,
            pointerEvents: css.pointerEvents, rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } };
        };
        const ancestry = element => {
          const rows = [];
          for (let node = element, depth = 0; node && depth < 8; node = node.parentElement, depth++) {
            rows.push({ tag: node.tagName, id: node.id || null, className: typeof node.className === "string" ? node.className : null,
              hidden: Boolean(node.hidden), ariaHidden: node.getAttribute("aria-hidden"), inert: Boolean(node.inert), style: style(node) });
          }
          return rows;
        };
        const controls = document.querySelector(".playback-controls"), toggle = controls?.querySelector(".playback-toggle");
        const active = document.activeElement;
        return { capturedAtMs: performance.now(), visibilityState: document.visibilityState,
          hasFocus: document.hasFocus(), activeElement: active ? { tag: active.tagName, id: active.id || null,
            className: typeof active.className === "string" ? active.className : null,
            ariaLabel: active.getAttribute("aria-label") } : null,
          controls: controls ? { data: { ...controls.dataset }, hidden: Boolean(controls.hidden),
            ariaHidden: controls.getAttribute("aria-hidden"), ariaBusy: controls.getAttribute("aria-busy"), style: style(controls) } : null,
          toggle: toggle ? { tag: toggle.tagName, type: toggle.type, hidden: Boolean(toggle.hidden),
            disabled: Boolean(toggle.disabled), ariaLabel: toggle.getAttribute("aria-label"),
            text: toggle.textContent, style: style(toggle), ancestry: ancestry(toggle) } : null };
      }),
      new Promise(resolve => setTimeout(() => resolve({ state: "page-snapshot-timeout" }), 250)),
    ]).catch(error => ({ state: "page-snapshot-error", error: String(error).slice(0, 500) }))
      .then(snapshot => record({ kind: "pause-dom-snapshot-resolved", snapshot }));
  }
  // Do not rely on seek to stop playback. Pause first so an advancing clock
  // cannot race the command-completion check or the screenshot.
  await waitReady(null, true, "paused-controls-ready");
  const scrubber = page.locator(".playback-scrubber");
  const authoredDuration = Number(await scrubber.getAttribute("max"));
  assert.ok(Number.isFinite(authoredDuration) && Math.abs(authoredDuration - storyboardDuration) < 1e-7,
    `Gallery duration ${authoredDuration} differs from storyboard ${storyboardDuration}`);
  // Endpoint requests preserve the engine's actual floating-point duration.
  const targetTime = sampleTime === null ? authoredDuration : sampleTime;
  assert.ok(targetTime <= authoredDuration, "requested checkpoint exceeds the actual duration");
  record({ kind: "seek-input-started", targetTime });
  const requestedTime = await scrubber.evaluate((input, target) => {
    input.value = String(target);
    const time = Number(input.value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return time;
  }, targetTime);
  record({ kind: "seek-input-dispatched", targetTime, requestedTime });
  assert.ok(Math.abs(requestedTime - targetTime) <= 8 * Number.EPSILON * Math.max(1, authoredDuration),
    "Browser slider quantized the requested checkpoint");
  // The slider remains enabled during a seek; data-busy is the command barrier.
  await waitReady(targetTime, true, "seek-target-ready");
  return targetTime;
}

// Run against real browser range inputs and the actual production controls,
// with a recording player. This qualifies DOM/seek forwarding, not the runtime.
export async function qualifyPlayheadEndpoints(page, controlsModule = "./playground-playback-controls.js") {
  return page.evaluate(async (moduleUrl) => {
    const { PlaygroundPlaybackControls } = await import(moduleUrl);
    const cases = [];
    for (const duration of [2.5999999999999996, 2.6, 2.6000000000000005,
      7.999999999999997, 7.999999999999999, 8, 0.0004]) {
      const pane = document.createElement("section");
      document.body.append(pane);
      const calls = [], presentations = [], errors = [];
      const player = {
        pause: async () => ({ time: 0, playing: false }),
        resume: async () => ({ time: 0, playing: true }),
        restartPlayback: async () => ({ time: 0, playing: true }),
        seek: (time) => {
          calls.push(time);
          const sequence = calls.length;
          return new Promise(resolve => requestAnimationFrame(() => {
            const receipt = { session: 1, sequence, time, unchanged: false };
            presentations.push(receipt);
            resolve({ time, playing: false, presentationReceipt: receipt });
          }));
        },
      };
      const controls = new PlaygroundPlaybackControls(player, pane, {
        durationSeconds: duration, onError: (error) => errors.push(String(error)),
      });
      try {
        controls.sync({ time: 0, playing: false });
        const slider = pane.querySelector(".playback-scrubber");
        for (const requested of [0, duration, duration / 3]) {
          slider.value = String(requested);
          const actualInput = Number(slider.value);
          slider.dispatchEvent(new Event("input", { bubbles: true }));
          await new Promise(resolve => requestAnimationFrame(resolve));
          await new Promise(resolve => requestAnimationFrame(resolve));
          const expected = requested === duration ? duration : actualInput;
          if (Math.abs(actualInput - requested) > 8 * Number.EPSILON * duration ||
              calls.at(-1) !== expected || errors.length ||
              presentations.at(-1)?.time !== expected ||
              Number(controls.element.dataset.elapsedSeconds) !== expected ||
              controls.element.dataset.busy !== "false" || controls.durationSeconds !== duration ||
              Number(slider.max) !== duration) {
            throw new Error(`Playhead changed an authored time: ${JSON.stringify({ duration, requested, actualInput, calls, errors })}`);
          }
          cases.push({ duration, requested, actualInput, forwarded: calls.at(-1), receipt: presentations.at(-1) });
        }
      } finally {
        controls.destroy();
        pane.remove();
      }
    }
    return { scope: "production DOM controls with a recording player", cases };
  }, controlsModule);
}
