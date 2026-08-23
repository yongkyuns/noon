import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { SceneIdentityMap } from "./scene-identity.js";
import { SampleWindow } from "./frame-metrics.js";

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
const metricCpuFrame = document.querySelector("#metric-cpu-frame");
const metricRuntime = document.querySelector("#metric-runtime");
const metricPrepare = document.querySelector("#metric-prepare");
const metricUploadMs = document.querySelector("#metric-upload-ms");
const metricEncode = document.querySelector("#metric-encode");
const metricGpu = document.querySelector("#metric-gpu");
const paneToolbar = document.querySelector(".pane-toolbar");
const toolbarActions = document.querySelector(".actions");
const editorStack = document.querySelector(".editor-stack");

const SCENE_EXAMPLES = [
  {
    name: "Getting started",
    path: "./python/demo_scene.py",
    summary:
      "Three primitives introduce position, rotation, and opacity tracks with semantic layout helpers.",
    features: "primitives · position · rotation · opacity",
  },
  {
    name: "Analytic Transform",
    path: "./python/examples/analytic_transform.py",
    summary:
      "Circle radius, rectangle size, and line endpoints interpolate directly without path conversion.",
    features: "Transform · analytic geometry · zero tessellation",
  },
  {
    name: "Lifecycle handoffs",
    path: "./python/examples/lifecycle_handoffs.py",
    summary:
      "ReplacementTransform swaps stable scene identity while TransformFromCopy keeps its source present.",
    features: "ReplacementTransform · TransformFromCopy · Presence",
  },
  {
    name: "Fade & appearance",
    path: "./python/examples/fade_appearance.py",
    summary:
      "A semitransparent object fades out and back in while authored semantic opacity stays unchanged.",
    features: "FadeOut · FadeIn · Appearance · semantic opacity",
  },
  {
    name: "Matching shapes",
    path: "./python/examples/matching_shapes.py",
    summary:
      "Two reordered rows pair circles and a rectangle by semantic shape signature rather than list position.",
    features: "TransformMatchingShapes · signatures · stable pairing",
  },
  {
    name: "Create shapes",
    path: "./python/examples/create_shapes.py",
    summary:
      "Circle, square, line, and arbitrary vector path draw progressively while analytic shapes return to their fast paths at completion.",
    features: "Create · analytic outlines · cached reveal",
  },
  {
    name: "Path reveal",
    path: "./python/examples/path_reveal.py",
    summary:
      "One multi-contour vector path draws progressively over its deterministic ordered arc-length domain.",
    features: "VectorPath · Reveal · multi-contour ordering",
  },
  {
    name: "Filled path Transform",
    path: "./python/examples/filled_path_transform.py",
    summary:
      "A parameterized rounded loop morphs into a five-point star using validated fixed fill topology.",
    features: "Transform · filled path · fixed topology",
  },
  {
    name: "Staggered timing",
    path: "./python/examples/staggered_choreography.py",
    summary:
      "Seven identical circles share one motion while start_time alone creates a readable stagger.",
    features: "start_time · easing · timeline composition",
  },
  {
    name: "Instanced field · 180",
    path: "./python/examples/instanced_field.py",
    summary:
      "A semantic 18×10 grid exercises analytic instancing, batching, and dirty-range uploads.",
    features: "180 circles · instancing · GPU batching",
  },
  {
    name: "Morph stress · 1,000",
    path: "./python/examples/morph_stress_test.py",
    context: { object_count: 1000 },
    summary:
      "One thousand simultaneous path morphs reuse twelve target geometries for focused stress profiling.",
    features: "1,000 morphs · 12 meshes · CPU/GPU profile",
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
    color: #8793aa;
    font-size: 0.69rem;
    line-height: 1.35;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .example-features {
    flex: 0 0 auto;
    max-width: 42%;
    padding: 0.28rem 0.48rem;
    border: 1px solid #2b3851;
    border-radius: 999px;
    color: #8190a9;
    font: 0.64rem ui-monospace, SFMono-Regular, Menlo, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 920px) {
    .example-features {
      display: none;
    }
    .example-picker label {
      display: none;
    }
  }
`;
document.head.appendChild(galleryStyle);

const exampleStrip = document.createElement("div");
exampleStrip.className = "example-strip";
const exampleCopy = document.createElement("div");
exampleCopy.className = "example-copy";
const exampleName = document.createElement("span");
exampleName.className = "example-name";
const exampleSummary = document.createElement("span");
exampleSummary.className = "example-summary";
exampleCopy.append(exampleName, exampleSummary);
const exampleFeatures = document.createElement("span");
exampleFeatures.className = "example-features";
exampleStrip.append(exampleCopy, exampleFeatures);
editorStack.parentNode.insertBefore(exampleStrip, editorStack);

const examplePicker = document.createElement("div");
examplePicker.className = "example-picker";
const examplePickerLabel = document.createElement("label");
examplePickerLabel.textContent = "Example";
const exampleSelect = document.createElement("select");
examplePicker.append(examplePickerLabel, exampleSelect);
paneToolbar.insertBefore(examplePicker, toolbarActions);

let activeExampleIndex = 0;
let activePaletteIndex = 0;

function activeExample() {
  return SCENE_EXAMPLES[activeExampleIndex];
}

function activePatchExample() {
  return PATCH_EXAMPLES[activePaletteIndex % PATCH_EXAMPLES.length];
}

function updateExampleMetadata(example) {
  exampleName.textContent = example.name;
  exampleSummary.textContent = example.summary;
  exampleFeatures.textContent = example.features;
}

function updateExampleSelect() {
  exampleSelect.replaceChildren();
  SCENE_EXAMPLES.forEach((example, index) => {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = example.name;
    exampleSelect.appendChild(option);
  });
  exampleSelect.value = String(activeExampleIndex);
}

async function fetchText(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`failed to fetch ${path}: ${response.status}`);
  }
  return response.text();
}

async function loadExample(index, { execute = true } = {}) {
  activeExampleIndex = index;
  const example = activeExample();
  exampleSelect.value = String(index);
  updateExampleMetadata(example);
  sceneSourceEditor.value = await fetchText(example.path);
  if (!execute) {
    return;
  }
  await executeSceneSource();
}

exampleSelect.addEventListener("change", async () => {
  const index = Number.parseInt(exampleSelect.value, 10);
  if (!Number.isInteger(index) || index < 0 || index >= SCENE_EXAMPLES.length) {
    return;
  }
  try {
    await loadExample(index);
  } catch (error) {
    status.textContent = "error";
    statusText.textContent = error instanceof Error ? error.message : String(error);
  }
});

function setStatus(label, text = "") {
  status.textContent = label;
  statusText.textContent = text;
}

function formatMetric(value, digits = 2) {
  return Number.isFinite(value) ? value.toFixed(digits) : "—";
}

function updateMetrics(stats, frameMetrics) {
  metricObjects.textContent = String(stats.instance_count ?? 0);
  metricDraws.textContent = String(stats.batch_count ?? 0);
  metricUpload.textContent = String(stats.dirty_instance_count ?? 0);
  metricTime.textContent = `${formatMetric(frameMetrics.time, 2)} s`;
  metricCpuFrame.textContent = `${formatMetric(frameMetrics.cpuFrameMs)} ms`;
  metricRuntime.textContent = `${formatMetric(frameMetrics.runtimeMs)} ms`;
  metricPrepare.textContent = `${formatMetric(frameMetrics.prepareMs)} ms`;
  metricUploadMs.textContent = `${formatMetric(frameMetrics.uploadMs)} ms`;
  metricEncode.textContent = `${formatMetric(frameMetrics.encodeMs)} ms`;
  metricGpu.textContent = frameMetrics.gpuMs == null ? "—" : `${formatMetric(frameMetrics.gpuMs)} ms`;
}

let pythonClient = null;
let sceneIdentity = new SceneIdentityMap();
let player = null;
let frameMetrics = new SampleWindow(180);
let running = true;
let lastFrameTime = null;

async function ensurePythonClient() {
  if (pythonClient === null) {
    pythonClient = new PythonAuthoringClient();
  }
  await pythonClient.ready();
  return pythonClient;
}

async function executeSceneSource() {
  setStatus("running", "executing Python scene");
  const client = await ensurePythonClient();
  const result = await client.execute(sceneSourceEditor.value, activeExample().context ?? {});
  if (result.kind !== "scene") {
    throw new Error(`scene source returned ${result.kind}, expected scene`);
  }
  const stableScene = sceneIdentity.rewriteScene(result.document);
  const reconcile = player.reconcileSceneJson(JSON.stringify(stableScene));
  setStatus(reconcile, `t=${formatMetric(player.time(), 2)} s`);
}

async function executePatchSource() {
  patchStatus.textContent = "running";
  const client = await ensurePythonClient();
  const example = activePatchExample();
  sourceEditor.value = await fetchText(example.path);
  const result = await client.execute(sourceEditor.value, example.context ?? {});
  if (result.kind !== "patch") {
    throw new Error(`patch source returned ${result.kind}, expected patch`);
  }
  const stablePatch = sceneIdentity.rewritePatch(result.document);
  player.applyPatchBatchJson(JSON.stringify(stablePatch));
  patchStatus.textContent = `applied · seq ${stablePatch.sequence}`;
}

sceneButton.addEventListener("click", async () => {
  try {
    await executeSceneSource();
  } catch (error) {
    setStatus("error", error instanceof Error ? error.message : String(error));
  }
});

patchButton.addEventListener("click", async () => {
  try {
    activePaletteIndex = (activePaletteIndex + 1) % PALETTES.length;
    await executePatchSource();
  } catch (error) {
    patchStatus.textContent = error instanceof Error ? error.message : String(error);
  }
});

sceneTab.addEventListener("click", () => {
  sceneTab.classList.add("active");
  patchTab.classList.remove("active");
  scenePanel.hidden = false;
  patchPanel.hidden = true;
});

patchTab.addEventListener("click", async () => {
  patchTab.classList.add("active");
  sceneTab.classList.remove("active");
  patchPanel.hidden = false;
  scenePanel.hidden = true;
  try {
    const example = activePatchExample();
    sourceEditor.value = await fetchText(example.path);
  } catch (error) {
    patchStatus.textContent = error instanceof Error ? error.message : String(error);
  }
});

function animationFrame(timestamp) {
  if (!running || player === null) {
    return;
  }
  const frameStart = performance.now();
  if (lastFrameTime === null) {
    lastFrameTime = timestamp;
  }
  const deltaSeconds = Math.max(0, timestamp - lastFrameTime) / 1000;
  lastFrameTime = timestamp;
  player.advance(deltaSeconds);
  const runtimeDone = performance.now();
  const stats = player.render();
  const renderDone = performance.now();
  const profile = player.renderProfile();
  const frameEnd = performance.now();

  frameMetrics.push({
    time: player.time(),
    cpuFrameMs: frameEnd - frameStart,
    runtimeMs: runtimeDone - frameStart,
    prepareMs: profile.prepare_ms,
    uploadMs: profile.upload_ms,
    encodeMs: profile.encode_ms,
    gpuMs: profile.gpu_ms,
  });
  updateMetrics(stats, frameMetrics.latest());
  requestAnimationFrame(animationFrame);
}

try {
  await init();
  player = await NoonCanvasPlayer.attach(canvas);
  const initialDocument = JSON.parse(demoSceneJson());
  sceneIdentity.initialize(initialDocument);
  player.loadSceneJson(JSON.stringify(initialDocument));
  updateExampleSelect();
  await loadExample(0, { execute: false });
  patchStatus.textContent = "ready";
  setStatus("ready", "Python editor + Rust/WASM runtime");
  requestAnimationFrame(animationFrame);
} catch (error) {
  running = false;
  setStatus("error", error instanceof Error ? error.message : String(error));
}
