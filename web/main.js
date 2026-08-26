import { ExecutionWorkerClient } from "./execution-worker-client.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { SceneIdentityMap } from "./scene-identity.js";

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
    name: "Manim CE · Create a circle",
    path: "./python/examples/tutorial_quickstart.py",
    summary:
      "A Noon/browser adaptation of the Manim CE quickstart: create and reveal a styled circle.",
    features: "Manim CE · Circle · Create · fill/stroke",
  },
  {
    name: "Manim CE · Square → circle",
    path: "./python/examples/tutorial_square_to_circle.py",
    summary:
      "The canonical Manim square-to-circle lesson, adapted to Noon's browser runtime.",
    features: "Manim CE · Square · Circle · Transform",
  },
  {
    name: "Manim CE · Positioning",
    path: "./python/examples/tutorial_positioning.py",
    summary:
      "Arrange a circle, square, and line as a group, place them at an edge, then animate the group.",
    features: "Manim CE · VGroup · arrange · to_edge",
  },
  {
    name: "Manim CE · .animate",
    path: "./python/examples/tutorial_animate.py",
    summary:
      "Chain shift, rotate, scale, and color changes through Manim-style .animate syntax.",
    features: "Manim CE · .animate · shift · rotate · scale",
  },
  {
    name: "Manim CE · Transform lifecycle",
    path: "./python/examples/tutorial_transform_lifecycle.py",
    summary:
      "Compare Transform with ReplacementTransform and their different object-lifecycle semantics.",
    features: "Manim CE · Transform · ReplacementTransform",
  },
  {
    name: "Manim CE · Animation groups",
    path: "./python/examples/tutorial_composition.py",
    summary:
      "Compose simultaneous and staggered motion with AnimationGroup and LaggedStart.",
    features: "Manim CE · AnimationGroup · LaggedStart",
  },
  {
    name: "Manim CE · ValueTracker",
    path: "./python/examples/tutorial_value_tracker.py",
    summary:
      "Drive a native reactive binding from a Manim-style ValueTracker animation.",
    features: "Manim CE · ValueTracker · native reactivity",
  },
  {
    name: "Manim parity · CreateCircle",
    path: "./python/examples/manim_parity_create_circle.py",
    summary:
      "Source-equivalent ManimCE v0.21 CreateCircle with no Noon visual or timing tuning.",
    features: "exact parity candidate · create-circle · pixels + time",
    parityFixture: "create-circle",
  },
  {
    name: "Manim parity · SquareToCircle",
    path: "./python/examples/manim_parity_square_to_circle.py",
    summary:
      "Source-equivalent ManimCE v0.21 Create → Transform → FadeOut sequence used by the parity oracle.",
    features: "exact parity candidate · square-to-circle · pixels + time",
    parityFixture: "square-to-circle",
  },
  {
    name: "Manim parity · SquareAndCircle",
    path: "./python/examples/manim_parity_square_and_circle.py",
    summary:
      "Source-equivalent ManimCE v0.21 positioning scene with the canonical next_to(..., buff=0.5) contract.",
    features: "exact parity candidate · layout · pixels + time",
    parityFixture: "square-and-circle",
  },
  {
    name: "Manim parity · AnimatedSquareToCircle",
    path: "./python/examples/manim_parity_animated_square_to_circle.py",
    summary:
      "Source-equivalent ManimCE v0.21 .animate and Transform sequence sampled by the raster/timeline oracle.",
    features: "exact parity candidate · animate · pixels + time",
    parityFixture: "animated-square-to-circle",
  },
  {
    name: "Manim parity · DifferentRotations",
    path: "./python/examples/manim_parity_different_rotations.py",
    summary:
      "Source-equivalent ManimCE v0.21 comparison of target-state .animate.rotate and Rotate semantics.",
    features: "exact parity candidate · Rotate · pixels + time",
    parityFixture: "different-rotations",
  },
  {
    name: "Manim parity · Add / Wait / LaggedStartMap",
    path: "./python/examples/manim_parity_add_wait_lagged_start_map.py",
    summary:
      "Source-equivalent ManimCE v0.21 composition scene for zero-duration Add, duration Wait, and mapped stagger timing.",
    features: "exact parity candidate · composition · pixels + time",
    parityFixture: "add-wait-lagged-start-map",
  },
  {
    name: "Manim parity · GrowFromPoint / Center / Edge",
    path: "./python/examples/manim_parity_grow_point_center_edge.py",
    summary:
      "Source-equivalent ManimCE v0.21 growing scene covering point, center, edge, point color, and staggered composition.",
    features: "exact parity candidate · growing · pixels + time",
    parityFixture: "grow-point-center-edge",
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

const EMPTY_SCENE_JSON = '{"version":1,"objects":[],"tracks":[]}';

try {
  const player = new ExecutionWorkerClient(canvas, {
    onError(error) {
      showError(error);
    },
  });
  const ready = await player.start(EMPTY_SCENE_JSON, { loopDurationSeconds: 4.0 });
  const rendererBackend = ready.render.backend;
  status.dataset.rendererBackend = rendererBackend;
  status.dataset.executionTopology = "engine-render-workers";

  function resize() {
    const scale = window.devicePixelRatio || 1;
    player.resize(canvas.clientWidth, canvas.clientHeight, scale);
  }

  resize();
  new ResizeObserver(resize).observe(canvas);

  let paletteIndex = 0;
  let authoringClient = null;
  const sceneIdentities = new SceneIdentityMap();

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

  const initialState = await player.state();
  patchStatus.dataset.sequence = String(initialState.nextPatchSequence);

  async function runScene() {
    setBusy(true);
    try {
      patchStatus.value = "Building Scene in the Python worker…";
      patchStatus.dataset.state = "running";
      authoringClient ??= new PythonAuthoringClient();
      const authored = await authoringClient.run(
        sceneSourceEditor.value,
        currentExample("scene").context ?? {},
      );
      if (authored.kind !== "scene_document") {
        throw new Error("Python scene source returned a PatchBatch");
      }

      const before = await player.state();
      // Python updater closures retain authoring-local object IDs. Callback-aware
      // stable identity reconciliation belongs to #64; callback-free scenes keep
      // using the stable identity adapter today.
      const runtimeDocument =
        authored.callbacks === null
          ? sceneIdentities.stabilize(authored.document, authored.identities)
          : authored.document;
      const result = await player.reconcileScene(JSON.stringify(runtimeDocument), {
        callbacks: authored.callbacks,
        authoringClient,
        loopDurationSeconds: authored.duration > 0 ? authored.duration : null,
      });
      if (result.time !== before.time) {
        throw new Error("Scene replacement changed the current playhead");
      }
      const report = await player.metrics();
      const operation = result.incremental
        ? "Scene updated incrementally"
        : "Scene rebuilt atomically";
      patchStatus.value = `${operation} · ${report.metrics.objectCount} objects · playhead ${result.time.toFixed(2)} s preserved`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.sequence = String(result.nextPatchSequence);
    } catch (error) {
      showPatchError(error);
    } finally {
      setBusy(false);
    }
  }

  async function runPatch() {
    setBusy(true);
    try {
      const before = await player.state();
      const sequence = Number(before.nextPatchSequence);
      if (!Number.isSafeInteger(sequence)) {
        throw new Error("Patch sequence exceeds JavaScript's safe integer range");
      }
      const palette = PALETTES[paletteIndex];
      patchStatus.value = "Running patch in the Python worker…";
      patchStatus.dataset.state = "running";

      authoringClient ??= new PythonAuthoringClient();
      const authored = await authoringClient.run(sourceEditor.value, { sequence, palette });
      if (authored.kind !== "patch_batch") {
        throw new Error("Python patch source returned a complete Scene");
      }
      const batch = authored.document;
      if (batch.sequence !== sequence) {
        throw new Error(`Python returned patch sequence ${batch.sequence}; expected ${sequence}`);
      }
      const result = await player.applyPatchBatch(JSON.stringify(batch));
      if (Number(result.nextPatchSequence) !== sequence + 1) {
        throw new Error("Runtime did not acknowledge the ordered patch batch");
      }
      if (result.time !== before.time) {
        throw new Error("Patch batch changed the current playhead");
      }
      patchStatus.value = `Patch ${sequence} accepted · ${currentExample("patch").name} · playhead preserved`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.sequence = String(result.nextPatchSequence);
      patchStatus.dataset.theme = palette.name;
      paletteIndex = (paletteIndex + 1) % PALETTES.length;
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
        void runScene();
      } else {
        void runPatch();
      }
    }
  });

  window.addEventListener(
    "pagehide",
    () => {
      authoringClient?.terminate();
      player.terminate();
    },
    { once: true },
  );

  let lastStatusUpdate = -Infinity;
  let metricsPending = false;
  async function updateWorkerMetrics(timestamp) {
    if (metricsPending || timestamp - lastStatusUpdate <= 200) {
      return;
    }
    metricsPending = true;
    try {
      const report = await player.metrics();
      const metrics = report.metrics;
      const host = report.engineMetrics.host;
      setRuntimeStatus(`${metrics.objectCount} objects · ${rendererBackend} worker`, "running");
      metricObjects.value = String(metrics.objectCount);
      metricDraws.value = String(metrics.drawCalls);
      metricUpload.value = formatBytes(metrics.bytesUploaded);
      metricTime.value = `${metrics.time.toFixed(2)} s`;
      metricCpuFrame.value = "engine worker";
      metricRuntime.value = host.enabled ? `${host.missedDeadlines} host misses` : "engine worker";
      metricPrepare.value = "render worker";
      metricUploadMs.value = "render worker";
      metricEncode.value = "render worker";
      metricGpu.value = rendererBackend;
      status.dataset.instances = String(metrics.instancesDrawn);
      status.dataset.uploadBytes = String(metrics.bytesUploaded);
      status.dataset.geometryCacheMisses = String(metrics.geometryCacheMisses);
      status.dataset.hostMissedDeadlines = String(host.missedDeadlines);
      status.dataset.hostDroppedLateResults = String(host.droppedLateResults);
      status.dataset.presentedFrames = String(metrics.presentedFrames);
      lastStatusUpdate = timestamp;
    } catch (error) {
      showError(error);
    } finally {
      metricsPending = false;
    }
  }

  function frame(timestamp) {
    void updateWorkerMetrics(timestamp);
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
} catch (error) {
  showError(error);
}
