import initNoonWeb, {
  RetainedNativeTextAuthoringHandle,
  RetainedTypstAuthoringHandle,
  WasmAuthoringStore,
  canonicalRetainedSceneSpecJson,
  manimAnnularSectorSnapshotJson,
  manimAnnulusSnapshotJson,
  manimDashedLineSnapshotJson,
  manimDotSnapshotJson,
  manimElbowSnapshotJson,
  manimRoundedRectangleSnapshotJson,
  manimSectorSnapshotJson,
  manimTriangleSnapshotJson,
  manimUnderlineSnapshotJson,
  resolveAnimationOptions,
  resolveCompositionSchedule,
  resolveLifecyclePlan,
  resolveUniformCompositionSchedule,
  validatePresenceTransition,
} from "./pkg/noon_web.js";
import { attachSemanticEngine } from "./semantic-engine-endpoint.js";
import { PYTHON_COMPAT_MODULES } from "./python-compat-modules.js";
import { loadPyodide } from "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.mjs";

const AUTHORING_CHANNEL = "noon.authoring";
const AUTHORING_PROTOCOL_VERSION = 6;
const HOST_CHANNEL = "noon.host-callback";
const HOST_PROTOCOL_VERSION = 1;
const AUTHORING_STARTUP_METRICS_VERSION = 1;
const moduleGraphReadyAt = performance.now();

const pyodidePromise = initializePyodide();
let requestQueue = Promise.resolve();
let engineHostPort = null;
const semanticContexts = new Map();
let nextSemanticContext = 0;

pyodidePromise
  .then(() => post("ready"))
  .catch((error) => postError(null, error));

self.addEventListener("message", (event) => {
  requestQueue = requestQueue.then(() => handleRequest(event.data));
});

async function initializePyodide() {
  const initializeStartedAt = performance.now();
  const resourceDurations = {};
  const noonWebReady = measureStartupTask(resourceDurations, "noonWebInitMs", () => initNoonWeb());
  const pyodideReady = measureStartupTask(resourceDurations, "pyodideInitMs", () => loadPyodide());
  const compatibilityBundleReady = measureStartupTask(
    resourceDurations,
    "compatibilityBundleMs",
    () => loadCompatibilityBundle(),
  );
  const startupResourcesReady = Promise.all([
    noonWebReady,
    pyodideReady,
    compatibilityBundleReady,
  ]);
  const [, pyodide, compatibilityModules] = await startupResourcesReady;
  const resourcesReadyAt = performance.now();
  const authoringStore = new WasmAuthoringStore();
  self.noonCreateCanonicalAuthoringSceneContext = () =>
    authoringStore.createSceneContext();
  self.noonRegisterSemanticExecution = (context) => {
    // Registration is the authoring-run publication boundary. It retains a
    // returned runtime for renderer recovery, but invalidates one only when
    // direct authored work changed since that runtime was published.
    context.prepareExecutionRun();
    const token = `semantic-${nextSemanticContext++}`;
    semanticContexts.set(token, { context, endpoints: new Set(), released: false });
    return token;
  };
  self.noonCreateAuthoringMobjectHandle = (snapshotJson) =>
    authoringStore.createMobject(snapshotJson);
  self.noonCreateAuthoringDotHandle = (pointX, pointY, radius) =>
    authoringStore.createMobject(manimDotSnapshotJson(pointX, pointY, radius));
  self.noonCreateAuthoringTriangleHandle = () =>
    authoringStore.createMobject(manimTriangleSnapshotJson());
  self.noonCreateAuthoringElbowHandle = (...args) =>
    authoringStore.createMobject(manimElbowSnapshotJson(...args));
  self.noonCreateAuthoringRoundedRectangleHandle = (...args) =>
    authoringStore.createMobject(manimRoundedRectangleSnapshotJson(...args));
  self.noonCreateAuthoringAnnularSectorHandle = (...args) =>
    authoringStore.createMobject(manimAnnularSectorSnapshotJson(...args));
  self.noonCreateAuthoringSectorHandle = (...args) =>
    authoringStore.createMobject(manimSectorSnapshotJson(...args));
  self.noonCreateAuthoringAnnulusHandle = (...args) =>
    authoringStore.createMobject(manimAnnulusSnapshotJson(...args));
  self.noonCreateAuthoringDashedLineHandle = (...args) =>
    authoringStore.createMobject(manimDashedLineSnapshotJson(...args));
  self.noonCreateAuthoringUnderlineHandle = (targetHandle, buff) =>
    authoringStore.createMobject(manimUnderlineSnapshotJson(targetHandle.snapshotJson(), buff));
  self.noonCreateAuthoringCircleHandle = (radius) => authoringStore.createManimCircle(radius);
  self.noonCreateAuthoringSquareHandle = (sideLength) => authoringStore.createManimSquare(sideLength);
  self.noonCreateAuthoringRectangleHandle = (width, height) =>
    authoringStore.createManimRectangle(width, height);
  self.noonCreateAuthoringLineHandle = (startX, startY, endX, endY) =>
    authoringStore.createManimLine(startX, startY, endX, endY);
  self.noonCreateAuthoringTextHandle = (source, fontFamily, fontSize, lineSpacing) =>
    authoringStore.createManimText(source, fontFamily, fontSize, lineSpacing);
  self.noonCreateAuthoringFamilyHandle = () => authoringStore.createFamily();
  self.noonCreateAuthoringFamilyMemberHandle = () =>
    authoringStore.createFamilyMember();
  self.noonCreateRetainedNativeTextHandle = (source, fontFamily, fontSize, lineSpacing) =>
    new RetainedNativeTextAuthoringHandle(source, fontFamily, fontSize, lineSpacing);
  self.noonCreateRetainedTypstHandle = (source, math, fontSize) =>
    new RetainedTypstAuthoringHandle(source, math, fontSize);
  self.noonResolveAnimationOptions = resolveAnimationOptionsPlain;
  self.noonResolveCompositionSchedule = resolveCompositionSchedulePlain;
  self.noonResolveUniformCompositionSchedule = resolveUniformCompositionSchedulePlain;
  self.noonResolveLifecyclePlan = resolveLifecyclePlanPlain;
  self.noonValidatePresenceTransition = validatePresenceTransitionPlain;
  self.noonCanonicalSceneSpecJson = canonicalRetainedSceneSpecJson;
  const bindingsReadyAt = performance.now();

  for (const [index, descriptor] of PYTHON_COMPAT_MODULES.entries()) {
    pyodide.FS.writeFile(descriptor.runtimePath, compatibilityModules[index].source, {
      encoding: "utf8",
    });
  }
  const compatibilityFilesReadyAt = performance.now();

  pyodide.runPython(`
import sys
sys.path.insert(0, "/tmp")
import _manim_compat
_manim_compat.install()
import _manim_rate_functions
_manim_rate_functions.install()
import _manim_phase_b
import _manim_geometry
import _manim_semantic_handles
_manim_semantic_handles.install()
import _manim_shared_geometry
_manim_shared_geometry.install()
import _manim_dashed_line
_manim_dashed_line.install()
import _manim_animate
import _manim_rotate
_manim_rotate.install()
import _manim_composition
_manim_composition.install()
import _manim_lifecycle
# Retained Text specializes content binding below the lifecycle-owned Scene.add path;
# it must not replace or intercept scene membership semantics.
import _manim_typst
_manim_typst.install()
# Install retained animation before later Scene.play adapters capture their
# predecessor. This keeps retained dispatch inside the normal wrapper chain and
# avoids Python call-expression binding races for Scene.play(ShrinkToCenter(Text(...))).
import _manim_retained_animate
_manim_retained_animate.install()
# Reconcile direct retained mutations with the same canonical Rust authoring
# state before later Scene.play adapters capture the retained scheduler.
import _manim_retained_state
_manim_retained_state.install()
import _manim_growing
_manim_growing.install()
import _manim_draw_border_then_fill
_manim_draw_border_then_fill.install()
import _manim_indication
_manim_indication.install()
import _manim_reactive
import _manim_updaters
_manim_updaters.install()
import _manim_camera
_manim_camera.install()
# Final production SceneSpec ownership: after all content/lifecycle adapters have
# installed, bind their events into one per-scene Rust canonical authoring context.
import _manim_canonical_scene
_manim_canonical_scene.install()
`);
  const importsReadyAt = performance.now();
  self.__noonAuthoringStartupMetrics = Object.freeze({
    version: AUTHORING_STARTUP_METRICS_VERSION,
    totalMs: importsReadyAt,
    moduleGraphLoadMs: moduleGraphReadyAt,
    initializeMs: importsReadyAt - initializeStartedAt,
    startupResourcesMs: resourcesReadyAt - initializeStartedAt,
    noonWebInitMs: resourceDurations.noonWebInitMs,
    pyodideInitMs: resourceDurations.pyodideInitMs,
    compatibilityBundleMs: resourceDurations.compatibilityBundleMs,
    authoringBindingsMs: bindingsReadyAt - resourcesReadyAt,
    compatibilityFsInstallMs: compatibilityFilesReadyAt - bindingsReadyAt,
    compatibilityImportInstallMs: importsReadyAt - compatibilityFilesReadyAt,
    compatibilityModuleCount: compatibilityModules.length,
    compatibilitySourceChars: compatibilityModules.reduce(
      (total, module) => total + module.source.length,
      0,
    ),
  });
  return pyodide;
}

function measureStartupTask(metrics, key, task) {
  const startedAt = performance.now();
  let result;
  try {
    result = task();
  } catch (error) {
    throw error;
  }
  return Promise.resolve(result).then((value) => {
    metrics[key] = performance.now() - startedAt;
    return value;
  });
}

async function loadCompatibilityBundle() {
  const response = await fetch(new URL("./python/compat-bundle.json", import.meta.url));
  if (!response.ok) {
    throw new Error(`Unable to load Noon Python compatibility bundle: HTTP ${response.status}`);
  }
  const bundle = await response.json();
  if (!isRecord(bundle) || bundle.version !== 1 || !Array.isArray(bundle.modules)) {
    throw new Error("Noon Python compatibility bundle has an invalid envelope");
  }
  if (bundle.modules.length !== PYTHON_COMPAT_MODULES.length) {
    throw new Error(
      `Noon Python compatibility bundle module count ${bundle.modules.length} does not match manifest ${PYTHON_COMPAT_MODULES.length}`,
    );
  }
  for (const [index, descriptor] of PYTHON_COMPAT_MODULES.entries()) {
    const module = bundle.modules[index];
    if (
      !isRecord(module) ||
      module.runtimePath !== descriptor.runtimePath ||
      module.label !== descriptor.label ||
      typeof module.source !== "string"
    ) {
      throw new Error(`Noon Python compatibility bundle is stale at ${descriptor.sourcePath}`);
    }
  }
  return bundle.modules;
}

function resolveAnimationOptionsPlain(...args) {
  const result = resolveAnimationOptions(...args);
  try {
    return {
      ok: result.ok,
      runTime: result.runTime,
      rateFunc: result.rateFunc,
      lagRatio: result.lagRatio,
      pathArc: result.pathArc,
      reverseRateFunction: result.reverseRateFunction,
      errorKind: result.errorKind ?? "",
      message: result.message ?? "",
    };
  } finally {
    result.free();
  }
}

function compositionResultPlain(result) {
  try {
    const intervals = [];
    for (let index = 0; index < result.length; index += 1) {
      intervals.push({
        startTime: result.startTime(index),
        duration: result.duration(index),
        endTime: result.endTime(index),
      });
    }
    return {
      ok: result.ok,
      runTime: result.runTime,
      intrinsicRunTime: result.intrinsicRunTime,
      intervals,
      errorKind: result.errorKind ?? "",
      message: result.message ?? "",
    };
  } finally {
    result.free();
  }
}

function resolveCompositionSchedulePlain(childRunTimesJson, lagRatio, runTime) {
  const childRunTimes = JSON.parse(childRunTimesJson);
  if (!Array.isArray(childRunTimes)) {
    throw new TypeError("child runtimes must decode to an array");
  }
  return compositionResultPlain(
    resolveCompositionSchedule(new Float64Array(childRunTimes), lagRatio, runTime),
  );
}

function resolveUniformCompositionSchedulePlain(...args) {
  return compositionResultPlain(resolveUniformCompositionSchedule(...args));
}

function lifecycleResultPlain(result) {
  try {
    return {
      ok: result.ok,
      bind: result.bind,
      showNow: result.showNow,
      hideNow: result.hideNow,
      showAtStart: result.showAtStart,
      hideAtEnd: result.hideAtEnd,
      errorKind: result.errorKind ?? "",
      message: result.message ?? "",
    };
  } finally {
    result.free();
  }
}

function resolveLifecyclePlanPlain(...args) {
  return lifecycleResultPlain(resolveLifecyclePlan(...args));
}

function validatePresenceTransitionPlain(...args) {
  return lifecycleResultPlain(validatePresenceTransition(...args));
}

async function handleRequest(request) {
  let requestId = null;
  try {
    validateRequest(request);
    requestId = request.requestId;
    const pyodide = await pyodidePromise;
    if (request.type === "run") {
      const resultJson = await runAuthoringSource(
        pyodide,
        request.source,
        request.context,
        request.exportDocument ?? false,
      );
      post("result", { requestId, resultJson });
      return;
    }
    if (request.type === "attach_semantic_execution") {
      const entry = semanticContexts.get(request.contextId);
      if (!entry || entry.released) throw new Error("unknown or retired semantic execution context");
      let endpoint;
      endpoint = attachSemanticEngine(entry.context, request, () => {
        entry.endpoints.delete(endpoint);
        retireSemanticContext(request.contextId, entry);
      });
      entry.endpoints.add(endpoint);
      post("semantic_execution_attached", { requestId });
      return;
    }
    if (request.type === "release_semantic_execution") {
      const entry = semanticContexts.get(request.contextId);
      if (entry) {
        entry.released = true;
        retireSemanticContext(request.contextId, entry);
      }
      post("semantic_execution_released", { requestId });
      return;
    }
    if (request.type === "callback_phase") {
      const patchBatchJson = await runCallbackPhase(
        pyodide,
        request.sessionId,
        request.frame,
        request.sequence,
      );
      post("callback_result", { requestId, patchBatchJson });
      return;
    }
    if (request.type === "attach_engine_port") {
      engineHostPort?.close?.();
      engineHostPort = request.port;
      engineHostPort.addEventListener("message", (event) => {
        requestQueue = requestQueue.then(() => handleHostRequest(event.data));
      });
      engineHostPort.start();
      post("host_port_attached", { requestId });
      return;
    }
    throw new Error(`Unsupported Python authoring request: ${request.type}`);
  } catch (error) {
    if (request?.type === "attach_semantic_execution") {
      request.controlPort?.close?.();
      request.renderPort?.close?.();
    }
    postError(requestId, error);
  }
}

function retireSemanticContext(token, entry) {
  if (entry.released && entry.endpoints.size === 0) {
    semanticContexts.delete(token);
    // Python may still retain this same wrapper on a reusable Scene. Dropping
    // our registry reference lets wasm-bindgen finalize it after all owners leave.
  }
}

async function handleHostRequest(request) {
  let requestId = null;
  let generation = null;
  try {
    validateHostRequest(request);
    requestId = request.requestId;
    generation = request.generation;
    const pyodide = await pyodidePromise;
    const patchBatchJson = await runCallbackPhase(pyodide, request.sessionId, request.frame, request.sequence);
    postHost("callback_result", { requestId, generation, patchBatchJson });
  } catch (error) {
    postHost("error", { requestId, generation, message: error instanceof Error ? error.message : String(error) });
  }
}

async function runAuthoringSource(pyodide, source, context, exportDocument = false) {
  const dictConstructor = pyodide.globals.get("dict");
  const globals = dictConstructor();
  dictConstructor.destroy();
  globals.set("__noon_source", source);
  globals.set("__noon_context_json", JSON.stringify(context));
  globals.set("__noon_export_document", exportDocument);

  try {
    const resultJson = await pyodide.runPythonAsync(
      `
import json
import _manim_updaters
from noon import PatchBatch, Scene

__noon_namespace = {
    "context": json.loads(__noon_context_json),
    "__name__": "__main__",
}
exec(__noon_source, __noon_namespace)

if "result" in __noon_namespace:
    __noon_result = __noon_namespace["result"]
else:
    __noon_scene_classes = [
        value
        for value in __noon_namespace.values()
        if isinstance(value, type)
        and issubclass(value, Scene)
        and value is not Scene
        and getattr(value, "__module__", None) == "__main__"
    ]
    if not __noon_scene_classes:
        raise RuntimeError(
            "Python authoring source must either assign result or define one Scene subclass"
        )
    if len(__noon_scene_classes) != 1:
        __noon_names = ", ".join(cls.__name__ for cls in __noon_scene_classes)
        raise RuntimeError(
            "Python authoring source defines multiple Scene subclasses; "
            f"select one explicitly via result = SceneClass(): {__noon_names}"
        )
    __noon_result = __noon_scene_classes[0]()
    __noon_result.setup()
    try:
        __noon_result.construct()
    finally:
        __noon_result.tear_down()

if isinstance(__noon_result, Scene):
    __noon_kind = "scene_document"
    import _manim_canonical_scene
    from js import noonRegisterSemanticExecution
    __noon_callbacks = _manim_updaters.register_scene(__noon_result)
    __noon_context = (None if __noon_export_document else
        _manim_canonical_scene.execution_context(__noon_result, __noon_callbacks))
    __noon_semantic = None
    __noon_live_duration = None
    if __noon_context is not None:
        __noon_live_duration = __noon_context.liveHandoffDuration()
        __noon_semantic = {"context_id": str(noonRegisterSemanticExecution(__noon_context))}
        __noon_scene_spec = None
        __noon_document = None
        __noon_retained = None
        __noon_identities = None
    else:
        if __noon_callbacks and getattr(__noon_result, "_semantic_text_handles", {}):
            raise RuntimeError(
                "native Text with Python callbacks is not supported by the retained "
                "renderer path yet; callback lowering must migrate to the shared session"
            )
        # A native Text timeline/export remains in the canonical context so its
        # temporary #959 codec is derived from the Rust store at finalization.
        # Geometry-only fallback retains the existing legacy materialization.
        if not getattr(__noon_result, "_semantic_text_handles", {}):
            _manim_canonical_scene.materialize_legacy_geometry(__noon_result)
        __noon_scene_spec = __noon_result.to_scene_spec()
        __noon_document = __noon_result.to_document()
        # The canonical document already includes every text object. The old
        # retained sidecar is not an execution input and must not ask a native
        # semantic Text handle for a source mirror.
        __noon_retained = None
        __noon_identities = __noon_result.identity_document()
    __noon_duration = (
        float(__noon_live_duration)
        if __noon_live_duration is not None
        else float(__noon_result.time)
    )
elif isinstance(__noon_result, PatchBatch):
    __noon_semantic = None
    __noon_kind = "patch_batch"
    __noon_scene_spec = None
    __noon_document = __noon_result.to_document()
    __noon_retained = None
    __noon_duration = None
    __noon_identities = None
    __noon_callbacks = None
else:
    raise TypeError("Python authoring result must be a noon.Scene or noon.PatchBatch")
json.dumps(
    {
        "kind": __noon_kind,
        "semantic_execution": __noon_semantic,
        "document": __noon_document,
        "retained_document": __noon_retained,
        "scene_spec": __noon_scene_spec,
        "duration": __noon_duration,
        "identities": __noon_identities,
        "callbacks": __noon_callbacks,
    },
    separators=(",", ":"),
    allow_nan=False,
)
`,
      { globals },
    );
    return resultJson;
  } finally {
    globals.destroy();
  }
}

async function runCallbackPhase(pyodide, sessionId, frame, sequence) {
  const dictConstructor = pyodide.globals.get("dict");
  const globals = dictConstructor();
  dictConstructor.destroy();
  globals.set("__noon_callback_session", sessionId);
  globals.set("__noon_callback_frame_json", JSON.stringify(frame));
  globals.set("__noon_callback_sequence", sequence);
  try {
    return await pyodide.runPythonAsync(
      `
import json
import _manim_updaters
_manim_updaters.run_callback_phase(
    int(__noon_callback_session),
    json.loads(__noon_callback_frame_json),
    int(__noon_callback_sequence),
)
`,
      { globals },
    );
  } finally {
    globals.destroy();
  }
}

function validateRequest(request) {
  if (!isRecord(request) || request.channel !== AUTHORING_CHANNEL) {
    throw new Error("Received a message from an unknown authoring channel");
  }
  if (request.protocolVersion !== AUTHORING_PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported authoring protocol version ${request.protocolVersion}`,
    );
  }
  if (!Number.isSafeInteger(request.requestId) || request.requestId < 0) {
    throw new Error("Python authoring request has an invalid request ID");
  }
  if (request.type === "run") {
    if (request.exportDocument !== undefined && typeof request.exportDocument !== "boolean") {
      throw new Error("exportDocument must be boolean");
    }

    if (typeof request.source !== "string" || request.source.trim() === "") {
      throw new Error("Python authoring source must be a non-empty string");
    }
    if (!isRecord(request.context)) {
      throw new Error("Python authoring context must be an object");
    }
    return;
  }
  if (request.type === "callback_phase") {
    if (!Number.isSafeInteger(request.sessionId) || request.sessionId < 0) {
      throw new Error("Python callback request has an invalid session ID");
    }
    if (!Number.isSafeInteger(request.sequence) || request.sequence < 0) {
      throw new Error("Python callback request has an invalid patch sequence");
    }
    if (!isRecord(request.frame)) {
      throw new Error("Python callback request must contain a frame object");
    }
    return;
  }
  if (request.type === "release_semantic_execution") {
    if (typeof request.contextId !== "string" || !request.contextId) throw new Error("invalid semantic context token");
    return;
  }
  if (request.type === "attach_semantic_execution") {
    if (typeof request.contextId !== "string" || !request.contextId ||
        !(request.controlPort instanceof MessagePort) || !(request.renderPort instanceof MessagePort)) {
      throw new Error("semantic attachment requires a context and two ports");
    }
    return;
  }
  if (request.type === "attach_engine_port") {
    if (!(request.port instanceof MessagePort)) {
      throw new Error("Python host attachment requires a MessagePort");
    }
    return;
  }
  throw new Error(`Unsupported Python authoring request: ${request.type}`);
}

function validateHostRequest(request) {
  if (!isRecord(request) || request.channel !== HOST_CHANNEL ||
      request.protocolVersion !== HOST_PROTOCOL_VERSION || request.type !== "callback_phase" ||
      !Number.isSafeInteger(request.requestId) || request.requestId < 0 ||
      !Number.isSafeInteger(request.generation) || request.generation < 0 ||
      !Number.isSafeInteger(request.sessionId) || request.sessionId < 0 ||
      !Number.isSafeInteger(request.sequence) || request.sequence < 0 || !isRecord(request.frame)) {
    throw new Error("invalid engine host callback request");
  }
}

function postHost(type, payload = {}) {
  if (engineHostPort === null) return;
  engineHostPort.postMessage({ channel: HOST_CHANNEL, protocolVersion: HOST_PROTOCOL_VERSION, type, ...payload });
}

function post(type, payload = {}) {
  self.postMessage({
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type,
    ...payload,
  });
}

function postError(requestId, error) {
  post("error", {
    requestId,
    message: error instanceof Error ? error.message : String(error),
    diagnostic: error instanceof Error && error.stack ? { stack: error.stack } : null,
  });
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
