import initNoonWeb, {
  WasmAuthoringMobjectHandle,
  resolveAnimationOptions,
  resolveCompositionSchedule,
  resolveLifecyclePlan,
  resolveUniformCompositionSchedule,
  validatePresenceTransition,
} from "./pkg/noon_web.js";
import { loadPyodide } from "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.mjs";

const AUTHORING_CHANNEL = "noon.authoring";
const AUTHORING_PROTOCOL_VERSION = 5;
const HOST_CHANNEL = "noon.host-callback";
const HOST_PROTOCOL_VERSION = 1;
const PYTHON_MODULE_PATH = "/tmp/noon.py";
const PYTHON_IR_MODULE_PATH = "/tmp/_noon_ir.py";
const MANIM_COMPAT_MODULE_PATH = "/tmp/_manim_compat.py";
const MANIM_SEMANTIC_HANDLES_MODULE_PATH = "/tmp/_manim_semantic_handles.py";
const MANIM_RATE_FUNCTIONS_MODULE_PATH = "/tmp/_manim_rate_functions.py";
const MANIM_PHASE_B_MODULE_PATH = "/tmp/_manim_phase_b.py";
const MANIM_ANIMATION_OPTIONS_MODULE_PATH = "/tmp/_manim_animation_options.py";
const MANIM_ANIMATE_MODULE_PATH = "/tmp/_manim_animate.py";
const MANIM_ROTATE_MODULE_PATH = "/tmp/_manim_rotate.py";
const MANIM_COMPOSITION_MODULE_PATH = "/tmp/_manim_composition.py";
const MANIM_LIFECYCLE_MODULE_PATH = "/tmp/_manim_lifecycle.py";
const MANIM_GROWING_MODULE_PATH = "/tmp/_manim_growing.py";
const MANIM_DRAW_BORDER_THEN_FILL_MODULE_PATH = "/tmp/_manim_draw_border_then_fill.py";
const MANIM_REACTIVE_MODULE_PATH = "/tmp/_manim_reactive.py";
const MANIM_UPDATERS_MODULE_PATH = "/tmp/_manim_updaters.py";

const pyodidePromise = initializePyodide();
let requestQueue = Promise.resolve();
let engineHostPort = null;

pyodidePromise
  .then(() => post("ready"))
  .catch((error) => postError(null, error));

self.addEventListener("message", (event) => {
  requestQueue = requestQueue.then(() => handleRequest(event.data));
});

async function initializePyodide() {
  await initNoonWeb();
  self.noonCreateAuthoringMobjectHandle = (snapshotJson) =>
    new WasmAuthoringMobjectHandle(snapshotJson);
  self.noonResolveAnimationOptions = resolveAnimationOptionsPlain;
  self.noonResolveCompositionSchedule = resolveCompositionSchedulePlain;
  self.noonResolveUniformCompositionSchedule = resolveUniformCompositionSchedulePlain;
  self.noonResolveLifecyclePlan = resolveLifecyclePlanPlain;
  self.noonValidatePresenceTransition = validatePresenceTransitionPlain;

  const pyodide = await loadPyodide();
  const [
    apiResponse,
    irResponse,
    compatResponse,
    semanticHandlesResponse,
    rateFunctionsResponse,
    phaseBResponse,
    animationOptionsResponse,
    animateResponse,
    rotateResponse,
    compositionResponse,
    lifecycleResponse,
    growingResponse,
    drawBorderThenFillResponse,
    reactiveResponse,
    updatersResponse,
  ] = await Promise.all([
    fetch(new URL("./python/noon.py", import.meta.url)),
    fetch(new URL("./python/_noon_ir.py", import.meta.url)),
    fetch(new URL("./python/_manim_compat.py", import.meta.url)),
    fetch(new URL("./python/_manim_semantic_handles.py", import.meta.url)),
    fetch(new URL("./python/_manim_rate_functions.py", import.meta.url)),
    fetch(new URL("./python/_manim_phase_b.py", import.meta.url)),
    fetch(new URL("./python/_manim_animation_options.py", import.meta.url)),
    fetch(new URL("./python/_manim_animate.py", import.meta.url)),
    fetch(new URL("./python/_manim_rotate.py", import.meta.url)),
    fetch(new URL("./python/_manim_composition.py", import.meta.url)),
    fetch(new URL("./python/_manim_lifecycle.py", import.meta.url)),
    fetch(new URL("./python/_manim_growing.py", import.meta.url)),
    fetch(new URL("./python/_manim_draw_border_then_fill.py", import.meta.url)),
    fetch(new URL("./python/_manim_reactive.py", import.meta.url)),
    fetch(new URL("./python/_manim_updaters.py", import.meta.url)),
  ]);
  const responses = [
    [apiResponse, "Noon Python API"],
    [irResponse, "Noon Python IR emitter"],
    [compatResponse, "Noon Manim compatibility layer"],
    [semanticHandlesResponse, "Noon shared semantic handle layer"],
    [rateFunctionsResponse, "Noon Manim rate functions"],
    [phaseBResponse, "Noon Manim Phase B layer"],
    [animationOptionsResponse, "Noon Manim animation options"],
    [animateResponse, "Noon Manim animate layer"],
    [rotateResponse, "Noon Manim Rotate layer"],
    [compositionResponse, "Noon Manim composition layer"],
    [lifecycleResponse, "Noon Manim lifecycle layer"],
    [growingResponse, "Noon Manim growing layer"],
    [drawBorderThenFillResponse, "Noon Manim DrawBorderThenFill layer"],
    [reactiveResponse, "Noon reactive compatibility layer"],
    [updatersResponse, "Noon Manim updater layer"],
  ];
  for (const [response, label] of responses) {
    if (!response.ok) {
      throw new Error(`Unable to load ${label}: HTTP ${response.status}`);
    }
  }

  const modules = [
    [PYTHON_MODULE_PATH, apiResponse],
    [PYTHON_IR_MODULE_PATH, irResponse],
    [MANIM_COMPAT_MODULE_PATH, compatResponse],
    [MANIM_SEMANTIC_HANDLES_MODULE_PATH, semanticHandlesResponse],
    [MANIM_RATE_FUNCTIONS_MODULE_PATH, rateFunctionsResponse],
    [MANIM_PHASE_B_MODULE_PATH, phaseBResponse],
    [MANIM_ANIMATION_OPTIONS_MODULE_PATH, animationOptionsResponse],
    [MANIM_ANIMATE_MODULE_PATH, animateResponse],
    [MANIM_ROTATE_MODULE_PATH, rotateResponse],
    [MANIM_COMPOSITION_MODULE_PATH, compositionResponse],
    [MANIM_LIFECYCLE_MODULE_PATH, lifecycleResponse],
    [MANIM_GROWING_MODULE_PATH, growingResponse],
    [MANIM_DRAW_BORDER_THEN_FILL_MODULE_PATH, drawBorderThenFillResponse],
    [MANIM_REACTIVE_MODULE_PATH, reactiveResponse],
    [MANIM_UPDATERS_MODULE_PATH, updatersResponse],
  ];
  for (const [path, response] of modules) {
    pyodide.FS.writeFile(path, await response.text(), { encoding: "utf8" });
  }

  pyodide.runPython(`
import sys
sys.path.insert(0, "/tmp")
import _manim_compat
_manim_compat.install()
import _manim_rate_functions
_manim_rate_functions.install()
import _manim_phase_b
import _manim_semantic_handles
_manim_semantic_handles.install()
import _manim_animate
import _manim_rotate
_manim_rotate.install()
import _manim_composition
_manim_composition.install()
import _manim_lifecycle
import _manim_growing
_manim_growing.install()
import _manim_draw_border_then_fill
_manim_draw_border_then_fill.install()
import _manim_reactive
import _manim_updaters
_manim_updaters.install()
`);
  return pyodide;
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
      );
      post("result", { requestId, resultJson });
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
    postError(requestId, error);
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

async function runAuthoringSource(pyodide, source, context) {
  const dictConstructor = pyodide.globals.get("dict");
  const globals = dictConstructor();
  dictConstructor.destroy();
  globals.set("__noon_source", source);
  globals.set("__noon_context_json", JSON.stringify(context));

  try {
    return await pyodide.runPythonAsync(
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
    __noon_duration = float(__noon_result.time)
    __noon_identities = __noon_result.identity_document()
    __noon_callbacks = _manim_updaters.register_scene(__noon_result)
elif isinstance(__noon_result, PatchBatch):
    __noon_kind = "patch_batch"
    __noon_duration = None
    __noon_identities = None
    __noon_callbacks = None
else:
    raise TypeError("Python authoring result must be a noon.Scene or noon.PatchBatch")
json.dumps(
    {
        "kind": __noon_kind,
        "document": __noon_result.to_document(),
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
  });
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
