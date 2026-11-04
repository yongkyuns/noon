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

export async function seekPausedGallery(page, storyboardDuration, sampleTime = null) {
  assert.ok(Number.isFinite(storyboardDuration) && storyboardDuration > 0);
  assert.ok(sampleTime === null || (Number.isFinite(sampleTime) && sampleTime >= 0 && sampleTime <= storyboardDuration),
    "requested checkpoint is outside the storyboard");
  const isReady = ({ expectedTime, paused }) => {
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
  };
  const diagnoseAnimationFramePolling = async () => {
    if (process.env.NOON_PLAYGROUND_RAF_DIAGNOSTIC !== "1") return;
    const setup = await page.evaluate(() => {
      const gallery = window.__noonExampleGallery;
      const controls = document.querySelector(".playback-controls");
      if (gallery?.selectedExampleId !== "manim-shrink-to-center-text" ||
          controls?.dataset.busy !== "false") return null;
      window.__noonRafDiagnosticOriginalRequestAnimationFrame = window.requestAnimationFrame;
      window.requestAnimationFrame = () => 0;
      return { selectedExampleId: gallery.selectedExampleId, initialBusy: controls.dataset.busy,
        visibilityState: document.visibilityState };
    });
    if (setup === null) return;
    const probeReady = () => document.documentElement.dataset.noonRafProbeReady === "true";
    const makePendingThenReady = async () => page.evaluate(() => {
      document.documentElement.dataset.noonRafProbeReady = "false";
      setTimeout(() => { document.documentElement.dataset.noonRafProbeReady = "true"; }, 120);
    });
    let defaultPolling;
    let intervalPolling;
    try {
      try {
        await makePendingThenReady();
        await page.waitForFunction(probeReady, null, { timeout: 500 });
        defaultPolling = "resolved";
      } catch (error) {
        if (error.name !== "TimeoutError") throw error;
        defaultPolling = "timed-out";
      }
      await makePendingThenReady();
      await page.waitForFunction(probeReady, null, { polling: 50, timeout: 500 });
      intervalPolling = "resolved";
    } catch (error) {
      intervalPolling = `failed: ${String(error)}`;
    } finally {
      await page.evaluate(() => {
        if (window.__noonRafDiagnosticOriginalRequestAnimationFrame) {
          window.requestAnimationFrame = window.__noonRafDiagnosticOriginalRequestAnimationFrame;
          delete window.__noonRafDiagnosticOriginalRequestAnimationFrame;
        }
        delete document.documentElement.dataset.noonRafProbeReady;
      });
    }
    const result = { ...setup, probe: "isolated DOM readiness; controls unchanged", defaultPolling, intervalPolling,
      overrideAffectedWaitRealm: defaultPolling === "timed-out" };
    console.log("Playwright RAF polling negative control:", JSON.stringify(result));
    assert.equal(defaultPolling, "timed-out", `RAF suppression did not exercise the default wait: ${JSON.stringify(result)}`);
    assert.equal(intervalPolling, "resolved", `RAF negative control interval polling failed: ${JSON.stringify(result)}`);
  };
  const waitReady = async (phase, expectedTime = null, paused = false) => {
    try {
      await page.waitForFunction(isReady, { expectedTime, paused }, { polling: 50 });
    } catch (error) {
      let observed = null;
      try {
        observed = await page.evaluate(async () => {
          const controls = document.querySelector(".playback-controls");
          const patch = document.querySelector("#patch-status");
          const toggle = controls?.querySelector(".playback-toggle");
          const scrubber = controls?.querySelector(".playback-scrubber");
          const gallery = window.__noonExampleGallery;
          let metrics = null, metricsError = null;
          try { metrics = await gallery?.executionMetrics(); } catch (cause) { metricsError = String(cause); }
          return {
            dom: {
              controls: controls ? { ...controls.dataset, toggle: toggle?.getAttribute("aria-label") ?? null } : null,
              scrubber: scrubber ? { value: scrubber.value, min: scrubber.min, max: scrubber.max } : null,
              patch: patch ? { state: patch.dataset.state, value: patch.value } : null,
              status: document.querySelector("#status") ? { ...document.querySelector("#status").dataset } : null,
            },
            playback: gallery ? {
              executionMode: gallery.executionMode,
              transportMode: gallery.transportMode,
              runInFlight: gallery.runInFlight,
              metrics,
              metricsError,
              presentationReceipt: metrics?.presentationReceipt ?? metrics?.metrics?.presentationReceipt ?? null,
              executionDiagnostics: gallery.executionDiagnostics,
              runPhaseMetrics: gallery.runPhaseMetrics,
            } : null,
            availableReceipts: window.__galleryControlReceipts ?? window.__triangleControlReceipts ?? null,
          };
        });
      } catch (captureError) {
        observed = { captureError: String(captureError) };
      }
      throw new Error(`Gallery seek phase "${phase}" did not settle; expected=${JSON.stringify({ expectedTime, paused })}; observed=${JSON.stringify(observed)}; cause=${String(error)}`, { cause: error });
    }
  };

  await diagnoseAnimationFramePolling();
  await waitReady("controls-ready-before-pause");
  const pause = page.getByRole("button", { name: "Pause animation", exact: true });
  const pauseRequested = await pause.count() > 0;
  if (pauseRequested) await pause.click();
  // Do not rely on seek to stop playback. Pause first so an advancing clock
  // cannot race the command-completion check or the screenshot.
  await waitReady(`pause-settled (requested=${pauseRequested})`, null, true);
  const scrubber = page.locator(".playback-scrubber");
  const authoredDuration = Number(await scrubber.getAttribute("max"));
  assert.ok(Number.isFinite(authoredDuration) && Math.abs(authoredDuration - storyboardDuration) < 1e-7,
    `Gallery duration ${authoredDuration} differs from storyboard ${storyboardDuration}`);
  // Endpoint requests preserve the engine's actual floating-point duration.
  const targetTime = sampleTime === null ? authoredDuration : sampleTime;
  assert.ok(targetTime <= authoredDuration, "requested checkpoint exceeds the actual duration");
  const requestedTime = await scrubber.evaluate((input, target) => {
    input.value = String(target);
    const time = Number(input.value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return time;
  }, targetTime);
  assert.ok(Math.abs(requestedTime - targetTime) <= 8 * Number.EPSILON * Math.max(1, authoredDuration),
    "Browser slider quantized the requested checkpoint");
  // The slider remains enabled during a seek; data-busy is the command barrier.
  await waitReady("seek-settled", targetTime, true);
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
