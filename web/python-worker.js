import { loadPyodide } from "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.mjs";

const AUTHORING_CHANNEL = "noon.authoring";
const AUTHORING_PROTOCOL_VERSION = 4;
const PYTHON_MODULE_PATH = "/tmp/noon.py";

const pyodidePromise = initializePyodide();
let requestQueue = Promise.resolve();

pyodidePromise
  .then(() => post("ready"))
  .catch((error) => postError(null, error));

self.addEventListener("message", (event) => {
  requestQueue = requestQueue.then(() => handleRequest(event.data));
});

async function initializePyodide() {
  const pyodide = await loadPyodide();
  const moduleUrl = new URL("./python/noon.py", import.meta.url);
  const response = await fetch(moduleUrl);
  if (!response.ok) {
    throw new Error(`Unable to load Noon Python API: HTTP ${response.status}`);
  }

  pyodide.FS.writeFile(PYTHON_MODULE_PATH, await response.text(), {
    encoding: "utf8",
  });
  pyodide.runPython("import sys; sys.path.insert(0, '/tmp')");
  return pyodide;
}

async function handleRequest(request) {
  let requestId = null;
  try {
    validateRequest(request);
    requestId = request.requestId;
    const pyodide = await pyodidePromise;
    const resultJson = await runAuthoringSource(
      pyodide,
      request.source,
      request.context,
    );
    post("result", {
      requestId,
      resultJson,
    });
  } catch (error) {
    postError(requestId, error);
  }
}

async function runAuthoringSource(pyodide, source, context) {
  const dictConstructor = pyodide.globals.get("dict");
  const globals = dictConstructor();
  dictConstructor.destroy();
  globals.set("__noon_source", source);
  globals.set("__noon_context_json", JSON.stringify(context));

  try {
    const resultJson = await pyodide.runPythonAsync(
      `
import json
from noon import PatchBatch, Scene

__noon_namespace = {"context": json.loads(__noon_context_json)}
exec(__noon_source, __noon_namespace)
if "result" not in __noon_namespace:
    raise RuntimeError("Python authoring source must assign a Scene or PatchBatch to result")
__noon_result = __noon_namespace["result"]
if isinstance(__noon_result, Scene):
    __noon_kind = "scene_document"
    __noon_identities = __noon_result.identity_document()
elif isinstance(__noon_result, PatchBatch):
    __noon_kind = "patch_batch"
    __noon_identities = None
else:
    raise TypeError("Python authoring result must be a noon.Scene or noon.PatchBatch")
json.dumps(
    {
        "kind": __noon_kind,
        "document": __noon_result.to_document(),
        "identities": __noon_identities,
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

function validateRequest(request) {
  if (!isRecord(request) || request.channel !== AUTHORING_CHANNEL) {
    throw new Error("Received a message from an unknown authoring channel");
  }
  if (request.protocolVersion !== AUTHORING_PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported authoring protocol version ${request.protocolVersion}`,
    );
  }
  if (request.type !== "run") {
    throw new Error(`Unsupported Python authoring request: ${request.type}`);
  }
  if (!Number.isSafeInteger(request.requestId) || request.requestId < 0) {
    throw new Error("Python authoring request has an invalid request ID");
  }
  if (typeof request.source !== "string" || request.source.trim() === "") {
    throw new Error("Python authoring source must be a non-empty string");
  }
  if (!isRecord(request.context)) {
    throw new Error("Python authoring context must be an object");
  }
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
