const state = { ready: false, error: null, cases: [], reentrantSamples: 0 };
globalThis.noonBlockingSceneProbe = state;
const params = new URLSearchParams(location.search);
const expectedBackend = params.get("backend") === "webgl" ? "WebGL2" : "WebGPU";
const packageDirectory = params.get("package") ?? "pkg";
const assert = (condition, message) => { if (!condition) throw new Error(message); };
const near = (actual, expected, message) => assert(Math.abs(actual - expected) < 1e-5, `${message}: ${actual} != ${expected}`);
const nextFrame = () => new Promise(resolve => requestAnimationFrame(resolve));
const lifecycle = runner => JSON.parse(runner.debugLifecycleJson());
const frame = runner => JSON.parse(runner.debugSelectionFrameJson());
async function rejected(promise, pattern) {
  let error;
  try { await promise; } catch (caught) { error = String(caught); }
  assert(error && pattern.test(error), `expected rejection ${pattern}, got ${error}`);
  return error;
}
async function until(predicate, label) {
  const deadline = performance.now() + 30_000;
  while (!predicate()) {
    assert(performance.now() < deadline, `timed out waiting for ${label}`);
    await nextFrame();
  }
}
async function pixels(canvas) {
  const bitmap = await createImageBitmap(await canvas.convertToBlob({ type: "image/png" }));
  const copy = new OffscreenCanvas(canvas.width, canvas.height);
  const context = copy.getContext("2d", { willReadFrequently: true });
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  return (worldX, worldY, radius = 0) => {
    const worldHeight = 8;
    const worldWidth = worldHeight * canvas.width / canvas.height;
    const centerX = Math.round((worldX + worldWidth / 2) / worldWidth * (canvas.width - 1));
    const centerY = Math.round((worldHeight / 2 - worldY) / worldHeight * (canvas.height - 1));
    const data = context.getImageData(centerX - radius, centerY - radius, radius * 2 + 1, radius * 2 + 1).data;
    const colors = [];
    for (let i = 0; i < data.length; i += 4) colors.push({ red: data[i], green: data[i + 1], blue: data[i + 2], alpha: data[i + 3] });
    return colors;
  };
}
const blue = color => color.blue > color.red + 30;
const orange = color => color.red > 180 && color.green > 60 && color.green < 190 && color.blue < 80;
const yellow = color => color.red > 180 && color.green > 150 && color.blue < color.green - 40;
function clean(canvas, runner) {
  runner.free();
  if (expectedBackend === "WebGL2") canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
}

try {
  assert(typeof WebAssembly.promising === "function" && typeof WebAssembly.Suspending === "function", "browser does not expose JSPI");
  const { default: initialize, BlockingSceneRunner } = await import(`./${packageDirectory}/noon_web.js`);
  await initialize();

  // Same ordinary Rust helper/loop/updater source as the native runner.
  {
    const canvas = new OffscreenCanvas(960, 540);
    const runner = new BlockingSceneRunner(canvas);
    const run = runner.runExample();
    const duplicatePending = await rejected(runner.runExample(), /only available/);
    let settled = false;
    const handled = run.then(() => { settled = true; }, error => { settled = true; throw error; });
    const samples = [];
    while (!settled) {
      const life = lifecycle(runner);
      if (life.hasRenderer) {
        const current = frame(runner);
        samples.push({ time: runner.time(), presentedFrames: runner.presentedFrames(), ids: current.objects.map(object => object.id), centers: current.objects.map(object => object.center), clockAnchor: life.clockAnchor });
        state.reentrantSamples += 1;
      }
      await nextFrame();
    }
    await handled;
    const final = frame(runner);
    const life = lifecycle(runner);
    assert(runner.rendererBackend() === expectedBackend, `wrong backend ${runner.rendererBackend()}`);
    near(runner.time(), 1, "authored endpoint");
    assert(final.objects.length === 2, "ordinary source changed object count");
    near(final.objects[0].center[0], 2, "loop/helper final square x");
    near(final.objects[0].center[1], 0, "loop/helper final square y");
    near(final.objects[1].center[0], -0.5 * Math.cos(0.75), "updater final center x");
    near(final.objects[1].center[1], -0.5 * Math.sin(0.75), "updater final center y");
    assert(life.phase === "finished" && !life.preparing, `source did not finish ${JSON.stringify(life)}`);
    assert(life.boundaryClockAnchors.length === 4, "missing admitted boundary anchors");
    const anchor = life.boundaryClockAnchors[0];
    assert(anchor && anchor[1] === 0, `initial clock is not rooted at authored zero ${anchor}`);
    assert(life.boundaryClockAnchors.every(value => JSON.stringify(value) === JSON.stringify(anchor)), `ordinary handoff reanchored clock: ${JSON.stringify(life.boundaryClockAnchors)}`);
    assert(samples.length >= 2, "browser did not reenter the same host during source suspension");
    const ids = final.objects.map(object => object.id);
    assert(samples.every(sample => JSON.stringify(sample.ids) === JSON.stringify(ids)), "object identities changed during suspension");
    const sample = await pixels(canvas);
    const finalSquare = sample(2, 0)[0];
    const frozenLine = sample(-0.65 * Math.cos(0.75), -0.65 * Math.sin(0.75), 3);
    const oldSquare = sample(0, 0.3)[0];
    assert(orange(finalSquare), `post-play orange edit missing: ${JSON.stringify(finalSquare)}`);
    assert(frozenLine.some(yellow), `frozen updater pixels missing: ${JSON.stringify(frozenLine)}`);
    assert(Math.max(oldSquare.red, oldSquare.green, oldSquare.blue) < 30, `old square position not cleared: ${JSON.stringify(oldSquare)}`);
    const duplicateFinished = await rejected(runner.runExample(), /only available/);
    const retained = { time: runner.time(), frames: runner.presentedFrames(), frame: runner.debugSelectionFrameJson() };
    await nextFrame(); await nextFrame();
    assert(runner.time() === retained.time && runner.presentedFrames() === retained.frames && runner.debugSelectionFrameJson() === retained.frame, "finished source continued changing");
    state.cases.push({ name: "ordinary-helper-loop-updater", time: runner.time(), frames: runner.presentedFrames(), backend: runner.rendererBackend(), samples, final, life, finalSquare, frozenLine: frozenLine.filter(yellow), oldSquare, duplicatePending, duplicateFinished });
    clean(canvas, runner);
  }

  // Rejecting an invalid first boundary keeps the original live program usable;
  // the final immediate edit must be published without an extra authored wait.
  for (const [name, method, time, x, color] of [
    ["invalid-first-wait-recovery", "runInvalidWaitExample", 0.25, 1, orange],
    ["static-source-final-frame", "runStaticExample", 0, 0, blue],
  ]) {
    const canvas = new OffscreenCanvas(960, 540);
    const runner = new BlockingSceneRunner(canvas);
    await runner[method]();
    const final = frame(runner);
    const life = lifecycle(runner);
    near(runner.time(), time, name + " time");
    assert(final.objects.length === 1, name + " object count");
    near(final.objects[0].center[0], x, name + " object center");
    assert(life.phase === "finished" && runner.presentedFrames() > 0, name + " publication");
    const actual = (await pixels(canvas))(x, 0)[0];
    assert(color(actual), `${name} final pixel ${JSON.stringify(actual)}`);
    state.cases.push({ name, time: runner.time(), frames: runner.presentedFrames(), final, life, pixel: actual });
    clean(canvas, runner);
  }

  // Cancellation can reenter this same WASM owner while an active wait is parked.
  {
    const canvas = new OffscreenCanvas(960, 540);
    const runner = new BlockingSceneRunner(canvas);
    const run = runner.runExample();
    const rejection = rejected(run, /cancelled by its host/);
    await until(() => runner.presentedFrames() > 0 && runner.time() > 0 && runner.time() < 1, "active authored boundary");
    const before = { time: runner.time(), frames: runner.presentedFrames(), frame: runner.debugSelectionFrameJson() };
    runner.cancel();
    const error = await rejection;
    await nextFrame(); await nextFrame();
    assert(runner.time() === before.time && runner.presentedFrames() === before.frames && runner.debugSelectionFrameJson() === before.frame, "cancelled source published more work");
    const life = lifecycle(runner);
    assert(life.phase === "cancelled" && !life.preparing, "active cancellation did not settle");
    const rerun = await rejected(runner.runExample(), /only available/);
    state.cases.push({ name: "cancel-active-boundary", before, life, error, rerun });
    clean(canvas, runner);
  }

  // Cancel before the queued GPU preparation task can publish a renderer. Its
  // eventual completion must retire itself, with no late frame or live owner.
  {
    const canvas = new OffscreenCanvas(960, 540);
    const runner = new BlockingSceneRunner(canvas);
    const run = runner.runExample();
    const before = lifecycle(runner);
    assert(before.preparing && !before.hasRenderer && before.presentedFrames === 0, `did not reach preparation suspension: ${JSON.stringify(before)}`);
    const rejection = rejected(run, /cancelled by its host/);
    runner.cancel();
    const error = await rejection;
    await until(() => !lifecycle(runner).preparing, "cancelled GPU preparation retirement");
    await nextFrame(); await nextFrame();
    const life = lifecycle(runner);
    assert(life.phase === "cancelled" && !life.hasRenderer && life.presentedFrames === 0 && runner.time() === 0, `late preparation was published: ${JSON.stringify(life)}`);
    state.cases.push({ name: "cancel-gpu-preparation", before, life, error });
    clean(canvas, runner);
  }
} catch (error) {
  state.error = String(error.stack ?? error);
} finally {
  state.ready = true;
}
