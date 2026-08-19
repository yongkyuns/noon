import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { diffSceneDocuments, SceneIdentityMap } from "./scene-identity.js";

const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const sceneButton = document.querySelector("#replace-scene");
const patchButton = document.querySelector("#apply-patch");
const patchStatus = document.querySelector("#patch-status");
const sceneSourceEditor = document.querySelector("#python-scene-source");
const sourceEditor = document.querySelector("#python-source");

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

function showError(error) {
  console.error(error);
  status.value = `Error: ${error}`;
  status.dataset.state = "error";
  patchStatus.value = "Patch failed";
  patchStatus.dataset.state = "error";
}

function showPatchError(error) {
  console.error(error);
  patchStatus.value = `Python failed: ${error}`;
  patchStatus.dataset.state = "error";
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
      patchStatus.value = "Python worker starts on first run";
      patchStatus.dataset.state = "ready";
    })
    .catch(showPatchError);
  patchStatus.dataset.sequence = String(player.nextSequence());
  sceneButton.addEventListener("click", async () => {
    sceneButton.disabled = true;
    patchButton.disabled = true;
    try {
      patchStatus.value = "Building a complete scene in Python…";
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
      patchStatus.value = `${operation} · ${player.objectCount()} objects · next patch ${nextSequence} · playhead ${preservedPlayhead.toFixed(2)} s preserved`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.sequence = String(nextSequence);
    } catch (error) {
      showPatchError(error);
    } finally {
      sceneButton.disabled = false;
      patchButton.disabled = false;
    }
  });
  patchButton.addEventListener("click", async () => {
    sceneButton.disabled = true;
    patchButton.disabled = true;
    try {
      const sequence = Number(player.nextSequence());
      if (!Number.isSafeInteger(sequence)) {
        throw new Error("Patch sequence exceeds JavaScript's safe integer range");
      }
      const palette = PALETTES[paletteIndex];
      patchStatus.value = "Running Python in the worker…";
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
      patchStatus.value = `Patch ${sequence} accepted · ${palette.name} · playhead ${preservedPlayhead.toFixed(2)} s preserved`;
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
  });
  window.addEventListener("pagehide", () => authoringClient?.terminate(), {
    once: true,
  });

  let lastStatusUpdate = -Infinity;
  function frame(timestamp) {
    try {
      const presented = player.renderFrame(timestamp);
      if (presented && timestamp - lastStatusUpdate > 200) {
        status.value = `${player.objectCount()} objects · ${player.lastDrawCalls()} draws · ${player.time().toFixed(2)} s`;
        status.dataset.state = "running";
        status.dataset.instances = String(player.lastInstancesDrawn());
        status.dataset.uploadBytes = String(player.lastBytesUploaded());
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
