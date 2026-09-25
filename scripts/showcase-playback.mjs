// Browser UI automation only. The production player still owns playback and seek.
import assert from "node:assert/strict";

export async function waitForPublishedGalleryFrame(page, targetTime, storyboardDuration) {
  assert.ok(Number.isFinite(targetTime) && targetTime >= 0);
  assert.ok(Number.isFinite(storyboardDuration) && storyboardDuration > 0);
  const tolerance = 8 * Number.EPSILON * Math.max(1, storyboardDuration);
  await page.waitForFunction(async ({ targetTime, tolerance }) => {
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
  const waitReady = async (expectedTime = null, paused = false) => {
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
        (!paused || toggle?.getAttribute("aria-label") === "Play animation") &&
        (expectedTime === null || Math.abs(Number(controls.dataset.elapsedSeconds) - expectedTime) < 1e-7);
    }, { expectedTime, paused });
  };

  await waitReady();
  const pause = page.getByRole("button", { name: "Pause animation", exact: true });
  if (await pause.count()) await pause.click();
  // Do not rely on seek to stop playback. Pause first so an advancing clock
  // cannot race the command-completion check or the screenshot.
  await waitReady(null, true);
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
  await waitReady(targetTime, true);
  return targetTime;
}

// Run against real browser range inputs and the actual production controls,
// with a recording player. This qualifies DOM/seek forwarding, not the runtime.
export async function qualifyPlayheadEndpoints(page, controlsModule = "./playground-playback-controls.js") {
  return page.evaluate(async (moduleUrl) => {
    const { PlaygroundPlaybackControls } = await import(moduleUrl);
    const cases = [];
    for (const duration of [2.5999999999999996, 2.6, 2.6000000000000005, 7.999999999999999, 8, 0.0004]) {
      const pane = document.createElement("section");
      document.body.append(pane);
      const calls = [], errors = [];
      const player = {
        pause: async () => ({ time: 0, playing: false }),
        resume: async () => ({ time: 0, playing: true }),
        restartPlayback: async () => ({ time: 0, playing: true }),
        seek: async (time) => { calls.push(time); return { time, playing: false }; },
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
          await Promise.resolve();
          const expected = requested === duration ? duration : actualInput;
          if (Math.abs(actualInput - requested) > 8 * Number.EPSILON * duration ||
              calls.at(-1) !== expected || errors.length ||
              Number(controls.element.dataset.elapsedSeconds) !== expected ||
              controls.element.dataset.busy !== "false" || controls.durationSeconds !== duration ||
              Number(slider.max) !== duration) {
            throw new Error(`Playhead changed an authored time: ${JSON.stringify({ duration, requested, actualInput, calls, errors })}`);
          }
          cases.push({ duration, requested, actualInput, forwarded: calls.at(-1) });
        }
      } finally {
        controls.destroy();
        pane.remove();
      }
    }
    return { scope: "production DOM controls with a recording player", cases };
  }, controlsModule);
}
