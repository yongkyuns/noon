from pathlib import Path
import shutil

root = Path('.')
carrier = Path(__file__).parent
shutil.copyfile(carrier / 'playground-gallery-runtime-smoke.mjs', root / 'scripts/playground-gallery-runtime-smoke.mjs')
p = root / 'web/direct-execution-smoke-probe.js'
s = p.read_text()
s = s.replace('  createDirectAffineCallbackSmokeRenderer,', '  createDirectAffineCallbackSmokeRenderer,\n  createDirectLiveUpdaterLifecycleSmokeRenderer,', 1)
needle = 'async function directScaleInPlaceProof(expectedBackend) {'
assert needle in s
s = s.replace(needle, '''async function directLiveUpdaterLifecycleProof(expectedBackend) {
  const canvas = new OffscreenCanvas(960, 540);
  const renderer = await createDirectLiveUpdaterLifecycleSmokeRenderer(canvas);
  const samples = [];
  try {
    renderer.resize(canvas.width, canvas.height);
    await settleDirectPublication(renderer, 0);
    let final;
    for (const milliseconds of [1000, 2000, 3000, 4000, 4500]) {
      renderer.advanceDirectRealtime(milliseconds);
      final = await settleDirectPublication(renderer, milliseconds);
      const expectedAngle = milliseconds <= 2000 ? milliseconds / 1000
        : milliseconds <= 4000 ? (4000 - milliseconds) / 1000 : 0;
      const color = await sampleRenderedColor(canvas,
        -0.6 * Math.cos(expectedAngle), -0.6 * Math.sin(expectedAngle));
      const time = renderer.time();
      samples.push({ time, expectedAngle, color });
      if (Math.abs(time - milliseconds / 1000) > 1e-6 ||
          Math.min(color.red, color.green) <= color.blue + 30) {
        throw new Error(`live updater reversal/freeze pixel mismatch: ${JSON.stringify(samples)}`);
      }
    }
    const metrics = { backend: renderer.rendererBackend(), time: renderer.time(),
      objects: renderer.objectCount(), cadence: final.cadence, samples };
    if (metrics.backend !== expectedBackend || metrics.objects !== 2 ||
        metrics.time !== 4.5 || metrics.cadence !== "idle") {
      throw new Error(`live updater lifecycle did not settle: ${JSON.stringify(metrics)}`);
    }
    return metrics;
  } finally {
    renderer.free();
    if (expectedBackend === "WebGL2") {
      canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
    }
  }
}

''' + needle, 1)
needle = '  metrics.scaleInPlace = await directScaleInPlaceProof(expectedBackend);'
assert needle in s
s = s.replace(needle, needle + '\n  metrics.liveUpdaterLifecycle = await directLiveUpdaterLifecycleProof(expectedBackend);', 1)
p.write_text(s)
p = root / 'docs/mobile-web-rendering.md'
p.write_text(p.read_text() + '''

### Full-gallery continuation follow-up (#1207)

The portable host compiler also admits synchronous lambda and nested callback
bodies which do not reference the outer scene or hide a play/wait barrier. It
leaves those bodies unchanged and preserves their ordinary Python closure and
callable identity. Direct module-level statement-position play/wait calls use
Python top-level await in the original module namespace. Definitions and module
effects execute once; arbitrary non-Scene methods and returned awaitables are
not treated as canonical barriers. Export execution keeps ordinary compilation.
This remains bounded host-language portability, not a general synchronous
Python compiler or a second animation scheduler.

Updater removal/replacement after a completed segment is a shared Rust semantic
transaction. The compiler prepares a revised callback index from semantic
preflight before the existing session atomically publishes it. Runtime identity,
current time, and the last effective frame survive the change; removal freezes
that effective value instead of restoring the authored value. Pending callback
phases, retroactive edits, mixed structural/registration transactions, and first
registration on a target absent from the initial callback index are rejected.
The latter two remain explicitly unsupported; they are not silently replayed.

This first bounded registration publication rebuilds the callback-only index in
O(R log R) time and O(R) temporary storage, where R is retained callback occurrence
history. It does not traverse or relower unrelated scene geometry, reset the
runtime, or add new per-frame work. It is not an O(1) registration edit. More
incremental callback-index editing remains under the shared live-session work
owned by #969; no temporary frontend schedule or compatibility authority is added.

The native `ordinary_live_updater_lifecycle` example and the direct Rust/WASM
pixel probe execute the same sequential remove/reverse/remove program as the
unchanged Python RotationUpdater gallery example. The full-gallery browser gate
executes every ready source and explicitly disables JSPI for the four cases
identified by the public audit. This complements, rather than replaces, the
canonical raster/timeline qualification and performance gates.
''')
p = root / 'scripts/build-web-demo.sh'
s = p.read_text(); needle = '  node --check scripts/shared-authoring-smoke.mjs'
assert needle in s
p.write_text(s.replace(needle, needle + '\n  node --check scripts/playground-gallery-runtime-smoke.mjs', 1))
