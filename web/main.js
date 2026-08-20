import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { diffSceneDocuments, SceneIdentityMap } from "./scene-identity.js";

const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const statusText = document.querySelector("#status-text");
const sceneButton = document.querySelector("#replace-scene");
const patchButton = document.querySelector("#apply-patch");
const patchStatus = document.querySelector("#patch-status");
const sceneSourceEditor = document.querySelector("#python-scene-source");
const sourceEditor = document.querySelector("#python-source");
const sceneTab = document.querySelector("#scene-tab");
const patchTab = document.querySelector("#patch-tab");
const scenePanel = document.querySelector("#scene-editor-panel");
const patchPanel = document.querySelector("#patch-editor-panel");
const metricObjects = document.querySelector("#metric-objects");
const metricDraws = document.querySelector("#metric-draws");
const metricUpload = document.querySelector("#metric-upload");
const metricTime = document.querySelector("#metric-time");

const PALETTES = [
  {
    name: "electric",
    circle: [1.0, 0.78, 0.22],
    rectangle: [0.72, 0.38, 0.96],
    line: [0.22, 0.88, 0.96],
  },
  {
    name: "original",
    circle: [0.98, 0.38, 0.36],
    rectangle: [0.27, 0.65, 0.96],
    line: [0.3, 0.88, 0.57],
  },
];

let activeEditor = "scene";

function selectEditor(kind) {
  activeEditor = kind;
  const sceneActive = kind === "scene";
  sceneTab.setAttribute("aria-selected", String(sceneActive));
  patchTab.setAttribute("aria-selected", String(!sceneActive));
  scenePanel.dataset.active = String(sceneActive);
  patchPanel.dataset.active = String(!sceneActive);
}

sceneTab.addEventListener("click", () => selectEditor("scene"));
patchTab.addEventListener("click", () => selectEditor("patch"));

function setRuntimeStatus(message, state) {
  statusText.textContent = message;
  status.dataset.state = state;
}

function showError(error) {
  console.error(error);
  setRuntimeStatus(`Error: ${error}`, "error");
  patchStatus.value = "Runtime failed";
  patchStatus.dataset.state = "error";
}

function showPatchError(error) {
  console.error(error);
  patchStatus.value = `Python failed: ${error}`;
  patchStatus.dataset.state = "error";
}

function formatBytes(bytes) {
  if (bytes === 0) {
    return "0 B";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KiB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

async function loadDemoAuthoringSource(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Unable to load demo Python: HTTP ${response.status}`);
  }
  return response.text();
}

try {
  if (!navigator.gpu) {
    throw new Error("This browser does not expose WebGPU");
  }

  await init();
  const player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);

  function resize() {
    const scale = window.devicePixelRatio || 1;
    const width = Math.round(canvas.clientWidth * scale);
    const height = Math.round(canvas.clientHeight * scale);
    player.resize(width, height);
  }

  resize();
  new ResizeObserver(resize).observe(canvas);

  let paletteIndex = 0;
  let authoringClient = null;
  const sceneIdentities = new SceneIdentityMap();
  let authoredScene = null;

  Promise.all([
    loadDemoAuthoringSource("./python/demo_scene.py"),
    loadDemoAuthoringSource("./python/demo_patch.py"),
  ])
    .then(([sceneSource, patchSource]) => {
      sceneSourceEditor.value = sceneSource;
      sourceEditor.value = patchSource;
      sceneButton.disabled = false;
      patchButton.disabled = false;
      patchStatus.value = "Ready · Python worker starts on first run";
      patchStatus.dataset.state = "ready";
    })
    .catch(showPatchError);

  patchStatus.dataset.sequence = String(player.nextSequence());

  async function runScene() {
    sceneButton.disabled = true;
    patchButton.disabled = true;
    try {
      patchStatus.value = "Building Scene in the Python worker…";
      patchStatus.dataset.state = "running";
      authoringClient ??= new PythonAuthoringClient();
      const result = await authoringClient.run(sceneSourceEditor.value);
      if (result.kind !== "scene_document") {
        throw new Error("Python scene source returned a PatchBatch");
      }

      const playhead = player.time();
      const stableDocument = sceneIdentities.stabilize(
        result.document,
        result.identities,
      );
      const patches =
        authoredScene === null
          ? null
          : diffSceneDocuments(authoredScene, stableDocument);
      let operation;
      if (patches === null) {
        const incremental = player.reconcileScene(JSON.stringify(stableDocument));
        operation = incremental ? "Scene reconciled" : "Scene replaced safely";
      } else if (patches.length > 0) {
        const sequence = Number(player.nextSequence());
        if (!Number.isSafeInteger(sequence)) {
          throw new Error("Patch sequence exceeds JavaScript's safe integer range");
        }
        player.applyPatchBatch(
          JSON.stringify({ version: 1, sequence, patches }),
        );
        operation = `Scene reconciled with ${patches.length} patch${patches.length === 1 ? "" : "es"}`;
      } else {
        operation = "Scene already current";
      }
      authoredScene = stableDocument;

      const preservedPlayhead = player.time();
      if (preservedPlayhead !== playhead) {
        throw new Error("Scene replacement changed the current playhead");
      }
      const nextSequence = Number(player.nextSequence());
      patchStatus.value = `${operation} · ${player.objectCount()} objects · playhead ${preservedPlayhead.toFixed(2)} s preserved`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.sequence = String(nextSequence);
    } catch (error) {
      showPatchError(error);
    } finally {
      sceneButton.disabled = false;
      patchButton.disabled = false;
    }
  }

  async function runPatch() {
    sceneButton.disabled = true;
    patchButton.disabled = true;
    try {
      const sequence = Number(player.nextSequence());
      if (!Number.isSafeInteger(sequence)) {
        throw new Error("Patch sequence exceeds JavaScript's safe integer range");
      }
      const palette = PALETTES[paletteIndex];
      patchStatus.value = "Running patch in the Python worker…";
      patchStatus.dataset.state = "running";

      authoringClient ??= new PythonAuthoringClient();
      const result = await authoringClient.run(sourceEditor.value, {
        sequence,
        palette,
      });
      if (result.kind !== "patch_batch") {
        throw new Error("Python patch source returned a complete Scene");
      }
      const batch = result.document;
      if (batch.sequence !== sequence) {
        throw new Error(
          `Python returned patch sequence ${batch.sequence}; expected ${sequence}`,
        );
      }
      const playhead = player.time();
      player.applyPatchBatch(JSON.stringify(batch));

      const nextSequence = Number(player.nextSequence());
      if (nextSequence !== sequence + 1) {
        throw new Error("Runtime did not acknowledge the ordered patch batch");
      }
      const preservedPlayhead = player.time();
      if (preservedPlayhead !== playhead) {
        throw new Error("Patch batch changed the current playhead");
      }
      patchStatus.value = `Patch ${sequence} accepted · ${palette.name} palette · playhead preserved`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.sequence = String(nextSequence);
      patchStatus.dataset.theme = palette.name;
      paletteIndex = (paletteIndex + 1) % PALETTES.length;
      authoredScene = null;
    } catch (error) {
      showPatchError(error);
    } finally {
      sceneButton.disabled = false;
      patchButton.disabled = false;
    }
  }

  sceneButton.addEventListener("click", runScene);
  patchButton.addEventListener("click", runPatch);

  document.addEventListener("keydown", (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      if (activeEditor === "scene") {
        runScene();
      } else {
        runPatch();
      }
    }
  });

  window.addEventListener("pagehide", () => authoringClient?.terminate(), {
    once: true,
  });

  let lastStatusUpdate = -Infinity;
  function frame(timestamp) {
    try {
      const presented = player.renderFrame(timestamp);
      if (presented && timestamp - lastStatusUpdate > 200) {
        const objectCount = player.objectCount();
        const drawCalls = player.lastDrawCalls();
        const uploadBytes = player.lastBytesUploaded();
        const playhead = player.time();

        setRuntimeStatus(`${objectCount} objects · WebGPU live`, "running");
        metricObjects.value = String(objectCount);
        metricDraws.value = String(drawCalls);
        metricUpload.value = formatBytes(uploadBytes);
        metricTime.value = `${playhead.toFixed(2)} s`;

        status.dataset.instances = String(player.lastInstancesDrawn());
        status.dataset.uploadBytes = String(uploadBytes);
        status.dataset.geometryCacheMisses = String(
          player.lastGeometryCacheMisses(),
        );
        lastStatusUpdate = timestamp;
      }
      requestAnimationFrame(frame);
    } catch (error) {
      showError(error);
    }
  }

  requestAnimationFrame(frame);
} catch (error) {
  showError(error);
}
