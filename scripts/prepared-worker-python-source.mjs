// Browser-qualification source only. This is not a worker implementation or a
// product source adapter. All completion/publication uses public Rust bindings;
// the enclosing test retains its existing actual render-worker transport.
export async function createPythonSource(wasm, modules, pyodideUrl) {
  const { loadPyodide } = await import(pyodideUrl);
  const pyodide = await loadPyodide();
  let store = new wasm.WasmAuthoringStore(), context = null;
  globalThis.noonCreateCanonicalAuthoringSceneContext = () => store.createSceneContext();
  globalThis.noonCreateAuthoringValueTrackerHandle = value => store.createValueTracker(value);
  globalThis.noonAuthoringGeometryOptions = wasm.WasmManimGeometryOptions;
  globalThis.noonAuthoringVectorPath = () => new wasm.WasmAuthoringVectorPath();
  globalThis.noonCreateAuthoringGeometryHandle = options => store.createManimGeometry(options);
  globalThis.noonAuthoringMembershipBatch = kind => new wasm.WasmSceneMembershipBatch(kind);
  globalThis.noonCreateAuthoringFamilyHandle = (batch, zIndex, drawEnabled, ...color) => color.length
    ? store.createFamilyWithColor(batch, zIndex, ...color, drawEnabled) : store.createFamily(batch, zIndex, drawEnabled);
  globalThis.noonResolveAnimationOptions = (...args) => {
    const value = wasm.resolveAnimationOptions(...args);
    try { return { runTime:value.runTime, rateFunc:value.rateFunc, lagRatio:value.lagRatio,
      pathArc:value.pathArc, reverseRateFunction:value.reverseRateFunction }; }
    finally { value.free(); }
  };
  for (const {runtimePath, source} of modules) pyodide.FS.writeFile(runtimePath, source);
  await pyodide.runPythonAsync(`
import sys
sys.path.insert(0, "/tmp")
from noon import Scene, Circle, Indicate, BLUE, RIGHT
scene = Scene()
target = Circle(radius=0.8, color=BLUE, fill_opacity=1.0, stroke_width=0.0)
scene.add(target)
scene.on_click(target, Indicate(target, run_time=0.5))
endpoint = target.copy().shift(RIGHT * 2)
animation = scene.declare_live_transform_to(target, endpoint, run_time=1.0, rate_func="linear")
live = scene.live_execution()
assert live.wait(1.0) == 1.0
assert live.prepare() == 1.0
source_context = scene._canonical_authoring_context
assert source_context.liveExecutionOwnership() == "active"
`);
  context = pyodide.globals.get("source_context");
  let player = null, delivered = 0;
  function adopt() {
    const capacity = player.pendingSourceObjectCapacity();
    if (capacity === undefined) throw new Error("source segment failed to retain its suffix");
    player.reservePrefixObjectCapacity(capacity);
    if (!player.deliverSourcePrefix(performance.now())) throw new Error("retained suffix was not adopted");
  }
  return {
    createWorkerPlayer(session) {
      if (player) throw new Error("qualification attempted duplicate player bootstrap");
      player = context.createPreparedExecutionPlayer(session);
      return player;
    },
    async publishNextToWorker(current, terminal) {
      if (current !== player || !context) throw new Error("invalid qualification source owner");
      if (!terminal) {
        if (delivered !== 0) throw new Error("source statement executed twice");
        await pyodide.runPythonAsync("assert live.play(animation) == 2.0");
        player.prepareSourceSegment(context);
        await pyodide.runPythonAsync(`
assert abs(live.effective_center(target).x - 2.0) < 1e-6
assert abs(target.get_center().x - 2.0) < 1e-6
assert source_context.liveExecutionOwnership() == "active"
`);
        adopt(); delivered++;
      } else {
        if (delivered !== 1) throw new Error("terminal completion out of order");
        await pyodide.runPythonAsync("assert live.wait(1.0) == 3.0");
        player.prepareSourceSegment(context); adopt();
        // A final same-time edit is authored before sealing, not inferred from
        // the wait's segment completion or a transport acknowledgment.
        await pyodide.runPythonAsync("live.set_translation(target, 3.0, 0.0)");
        player.finishSourcePreparation(context); adopt(); delivered++;
        await pyodide.runPythonAsync(`
del live, animation, endpoint, target, scene, source_context
import gc
gc.collect()
`);
        context.free(); context = null; store.free(); store = null;
      }
    },
    producerAlive() { return context !== null; },
    free() {
      if (context || store) throw new Error("qualification failed to retire its source");
      player = null;
    },
  };
}
