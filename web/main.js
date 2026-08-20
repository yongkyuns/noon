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
const paneToolbar = document.querySelector(".pane-toolbar");
const toolbarActions = document.querySelector(".actions");
const editorStack = document.querySelector(".editor-stack");

const SCENE_EXAMPLES = [
  {
    name: "Getting started",
    path: "./python/demo_scene.py",
    summary: "Circles, rectangles, lines and a cubic path morph on one 4 s timeline.",
    features: "primitives · path morph · timeline",
  },
  {
    name: "Path morph / Transform",
    path: "./python/examples/path_morph_transform.py",
    summary:
      "A rounded closed loop transforms into a sharp star using Scene.play(Transform(...)).",
    features: "Transform · path morph · GPU interpolation",
  },
  {
    name: "Staggered choreography",
    path: "./python/examples/staggered_choreography.py",
    summary:
      "Seventeen independent objects with staggered position, rotation and opacity tracks.",
    features: "staggering · easing · track composition",
  },
  {
    name: "Vector path garden",
    path: "./python/examples/vector_path_garden.py",
    summary:
      "Repeated cubic and quadratic paths demonstrate semantic path authoring and cached mesh reuse.",
    features: "cubic paths · quadratic paths · geometry cache",
  },
  {
    name: "180-dot instanced field",
    path: "./python/examples/instanced_field.py",
    summary:
      "A dense animated circle field designed to show batching, instancing and dirty-range uploads.",
    features: "180 objects · instancing · GPU batching",
  },
  {
    name: "Kinetic lines",
    path: "./python/examples/kinetic_lines.py",
    summary:
      "Thirty-two independently animated analytic lines plus outlined circles in a rotating fan.",
    features: "analytic lines · batching · opacity",
  },
  {
    name: "Mixed geometry",
    path: "./python/examples/mixed_geometry.py",
    summary:
      "Analytic circles and rectangles animate around a cached cubic vector mesh without retessellation.",
    features: "mixed geometry · cache reuse · live tracks",
  },
];

const PATCH_EXAMPLES = [
  {
    name: "Palette swap",
    path: "./python/demo_patch.py",
    summary:
      "Changes styles on existing runtime objects using one ordered semantic PatchBatch.",
    features: "style patch · dirty ranges · playhead preserved",
  },
  {
    name: "Transform remix",
    path: "./python/examples/transform_patch.py",
    summary:
      "Directly moves, rotates and scales the first three objects without rebuilding the scene.",
    features: "transform patch · live mutation · no scene rebuild",
  },
];

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

const galleryStyle = document.createElement("style");
galleryStyle.textContent = `
  .example-picker {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.42rem;
    margin-left: auto;
    margin-right: 0.45rem;
    color: #7f8ca5;
    font-size: 0.7rem;
  }
  .example-picker select {
    max-width: 13.5rem;
    min-width: 0;
    border: 1px solid #303d58;
    border-radius: 0.55rem;
    padding: 0.43rem 1.8rem 0.43rem 0.58rem;
    background: #111722;
    color: #dbe2ef;
    font: 0.72rem ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .example-strip {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    min-height: 2.9rem;
    padding: 0.52rem 0.82rem;
    border-bottom: 1px solid #263149;
    background: rgba(11, 15, 24, 0.92);
  }
  .example-copy {
    min-width: 0;
    flex: 1;
  }
  .example-name {
    display: block;
    color: #dce3f1;
    font-size: 0.74rem;
    font-weight: 700;
  }
  .example-summary {
    display: block;
    margin-top: 0.12rem;
    overflow: hidden;
    color: #7f8ca5;
    font-size: 0.68rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .example-features {
    flex: none;
    max-width: 42%;
    overflow: hidden;
    color: #a79aff;
    font: 0.66rem ui-monospace, SFMono-Regular, Menlo, monospace;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 74rem) {
    .example-picker span { display: none; }
    .example-picker select { max-width: 10.5rem; }
    .example-features { display: none; }
  }
  @media (max-width: 56rem) {
    .example-picker { order: 3; width: 100%; margin: 0 0 0.55rem; }
    .example-picker select { width: 100%; max-width: none; }
  }
`;
document.head.append(galleryStyle);

const examplePicker = document.createElement("label");
examplePicker.className = "example-picker";
const examplePickerLabel = document.createElement("span");
examplePickerLabel.textContent = "Example";
const exampleSelect = document.createElement("select");
exampleSelect.setAttribute("aria-label", "Playground example");
examplePicker.append(examplePickerLabel, exampleSelect);
paneToolbar.insertBefore(examplePicker, toolbarActions);

const exampleStrip = document.createElement("div");
exampleStrip.className = "example-strip";
const exampleCopy = document.createElement("div");
exampleCopy.className = "example-copy";
const exampleName = document.createElement("span");
exampleName.className = "example-name";
const exampleSummary = document.createElement("span");
exampleSummary.className = "example-summary";
const exampleFeatures = document.createElement("span");
exampleFeatures.className = "example-features";
exampleCopy.append(exampleName, exampleSummary);
exampleStrip.append(exampleCopy, exampleFeatures);
editorStack.parentElement.insertBefore(exampleStrip, editorStack);

let activeEditor = "scene";
const selectedExample = { scene: 0, patch: 0 };

function examplesFor(kind) {
  return kind === "scene" ? SCENE_EXAMPLES : PATCH_EXAMPLES;
}

function currentExample(kind = activeEditor) {
  return examplesFor(kind)[selectedExample[kind]];
}

function refreshExampleChrome() {
  const examples = examplesFor(activeEditor);
  exampleSelect.replaceChildren();
  examples.forEach((example, index) => {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = example.name;
    option.selected = index === selectedExample[activeEditor];
    exampleSelect.append(option);
  });
  const example = currentExample();
  exampleName.textContent = example.name;
  exampleSummary.textContent = example.summary;
  exampleFeatures.textContent = example.features;
}

function selectEditor(kind) {
  activeEditor = kind;
  const sceneActive = kind === "scene";
  sceneTab.setAttribute("aria-selected", String(sceneActive));
  patchTab.setAttribute("aria-selected", String(!sceneActive));
  scenePanel.dataset.active = String(sceneActive);
  patchPanel.dataset.active = String(!sceneActive);
  refreshExampleChrome();
}

sceneTab.addEventListener("click", () => selectEditor("scene"));
patchTab.addEventListener("click", () => selectEditor("patch"));
refreshExampleChrome();

function setRuntimeStatus(message, state) {
  statusText.textContent = message;
  status.dataset.state = state;
}

function setBusy(busy) {
  sceneButton.disabled = busy;
  patchButton.disabled = busy;
  exampleSelect.disabled = busy;
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

  async function loadExample(kind, index, { run = false } = {}) {
    const examples = examplesFor(kind);
    if (!Number.isInteger(index) || index < 0 || index >= examples.length) {
      throw new Error(`Unknown ${kind} example index ${index}`);
    }
    const example = examples[index];
    setBusy(true);
    try {
      const source = await loadDemoAuthoringSource(example.path);
      selectedExample[kind] = index;
      if (kind === "scene") {
        sceneSourceEditor.value = source;
        authoredScene = null;
      } else {
        sourceEditor.value = source;
      }
      if (activeEditor === kind) {
        refreshExampleChrome();
      }
      patchStatus.value = `${example.name} loaded · ${example.features}`;
      patchStatus.dataset.state = "ready";
    } finally {
      setBusy(false);
    }

    if (run) {
      if (kind === "scene") {
        await runScene();
      } else {
        await runPatch();
      }
    }
  }

  Promise.all([
    loadDemoAuthoringSource(SCENE_EXAMPLES[0].path),
    loadDemoAuthoringSource(PATCH_EXAMPLES[0].path),
  ])
    .then(([sceneSource, patchSource]) => {
      sceneSourceEditor.value = sceneSource;
      sourceEditor.value = patchSource;
      setBusy(false);
      patchStatus.value = "Ready · choose a feature example or edit Python";
      patchStatus.dataset.state = "ready";
    })
    .catch(showPatchError);

  patchStatus.dataset.sequence = String(player.nextSequence());

  async function runScene() {
    setBusy(true);
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
      setBusy(false);
    }
  }

  async function runPatch() {
    setBusy(true);
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
      patchStatus.value = `Patch ${sequence} accepted · ${currentExample("patch").name} · playhead preserved`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.sequence = String(nextSequence);
      patchStatus.dataset.theme = palette.name;
      paletteIndex = (paletteIndex + 1) % PALETTES.length;
      authoredScene = null;
    } catch (error) {
      showPatchError(error);
    } finally {
      setBusy(false);
    }
  }

  sceneButton.addEventListener("click", runScene);
  patchButton.addEventListener("click", runPatch);

  exampleSelect.addEventListener("change", async () => {
    const index = Number(exampleSelect.value);
    try {
      await loadExample(activeEditor, index, { run: activeEditor === "scene" });
    } catch (error) {
      showPatchError(error);
      setBusy(false);
    }
  });

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
