import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { SceneIdentityMap } from "./scene-identity.js";
import {
  exampleUrl,
  filterGalleryExamples,
  galleryCategories,
  loadGalleryManifest,
  parityLabel,
  requestedExampleId,
} from "./example-gallery.js";

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
const workspace = document.querySelector(".workspace");
const toolbarActions = document.querySelector(".actions");

// The public demo is a Manim compatibility surface. Noon-native patch templates and
// implementation/stress examples remain repository fixtures, but are not user-facing examples.
patchButton.hidden = true;
patchTab.hidden = true;
patchPanel.hidden = true;
sourceEditor.hidden = true;
sceneTab.hidden = true;
sceneButton.textContent = "Run";

const galleryStyle = document.createElement("style");
galleryStyle.textContent = `
  .example-gallery {
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: rgb(7 10 16 / 78%);
    overflow: hidden;
  }
  .gallery-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.9rem 1rem;
    border-bottom: 1px solid var(--border);
  }
  .gallery-title { min-width: 0; }
  .gallery-title strong {
    display: block;
    color: #e4e8f2;
    font-size: 0.86rem;
  }
  .gallery-title span {
    display: block;
    margin-top: 0.18rem;
    color: var(--muted-2);
    font-size: 0.68rem;
  }
  .gallery-controls {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }
  .gallery-controls input,
  .gallery-controls select {
    min-width: 0;
    border: 1px solid #303d58;
    border-radius: 0.58rem;
    background: #111722;
    color: #dbe2ef;
    padding: 0.48rem 0.62rem;
    font: 0.72rem ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .gallery-controls input { width: min(15rem, 30vw); }
  .gallery-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12.5rem, 1fr));
    gap: 0.72rem;
    padding: 0.85rem;
  }
  .gallery-empty {
    padding: 1.4rem;
    color: var(--muted);
    font-size: 0.76rem;
  }
  .example-card {
    min-width: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid #27334b;
    border-radius: 0.78rem;
    background: #0c111b;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }
  .example-card:hover { border-color: #4a5e87; }
  .example-card[aria-selected="true"] {
    border-color: #8e7cff;
    box-shadow: 0 0 0 1px rgb(142 124 255 / 45%);
  }
  .example-card:focus-visible {
    outline: 2px solid var(--accent-strong);
    outline-offset: 2px;
  }
  .example-thumb {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    background: #1c1c1c;
    border-bottom: 1px solid #222d41;
  }
  .example-card-copy { padding: 0.66rem 0.7rem 0.72rem; }
  .example-card-title {
    display: block;
    overflow: hidden;
    color: #e0e5f0;
    font-size: 0.74rem;
    font-weight: 750;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .example-card-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-top: 0.42rem;
  }
  .example-category,
  .example-parity {
    overflow: hidden;
    font-size: 0.61rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .example-category { color: #8390a8; }
  .example-parity { color: #b4a8ff; }
  .selected-example {
    display: flex;
    align-items: flex-start;
    gap: 0.8rem;
    min-height: 3.8rem;
    margin-bottom: 1rem;
    padding: 0.75rem 0.85rem;
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    background: rgba(11, 15, 24, 0.92);
  }
  .selected-copy { min-width: 0; flex: 1; }
  .selected-title {
    display: block;
    color: #e3e7f1;
    font-size: 0.78rem;
    font-weight: 750;
  }
  .selected-summary {
    display: block;
    margin-top: 0.18rem;
    color: #8592aa;
    font-size: 0.68rem;
  }
  .selected-tags {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.35rem;
    max-width: 48%;
  }
  .selected-tag {
    padding: 0.22rem 0.4rem;
    border: 1px solid #313d56;
    border-radius: 999px;
    color: #aab5ca;
    font: 0.61rem ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .selected-tag.parity { color: #c2b8ff; border-color: #514691; }
  .reset-example { white-space: nowrap; }
  @media (max-width: 68rem) {
    .gallery-head { align-items: stretch; flex-direction: column; }
    .gallery-controls { display: grid; grid-template-columns: 1fr 1fr 1fr; }
    .gallery-controls input { width: 100%; }
  }
  @media (max-width: 44rem) {
    .gallery-controls { grid-template-columns: 1fr; }
    .gallery-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.55rem; padding: 0.6rem; }
    .selected-example { flex-direction: column; }
    .selected-tags { justify-content: flex-start; max-width: none; }
  }
  @media (max-width: 28rem) {
    .gallery-grid { grid-template-columns: 1fr; }
  }
`;
document.head.append(galleryStyle);

const galleryManifest = await loadGalleryManifest();
const SCENE_EXAMPLES = galleryManifest.examples;
if (SCENE_EXAMPLES.length === 0) {
  throw new Error("No source-equivalent ManimCE examples are ready for the gallery");
}

const gallerySection = document.createElement("section");
gallerySection.className = "example-gallery";
gallerySection.setAttribute("aria-label", "Manim compatible scene examples");
const galleryHead = document.createElement("div");
galleryHead.className = "gallery-head";
const galleryTitle = document.createElement("div");
galleryTitle.className = "gallery-title";
galleryTitle.innerHTML = `<strong>ManimCE examples</strong><span>Source-equivalent v${galleryManifest.reference?.version ?? "0.21.0"} scenes</span>`;
const galleryControls = document.createElement("div");
galleryControls.className = "gallery-controls";
const gallerySearch = document.createElement("input");
gallerySearch.type = "search";
gallerySearch.placeholder = "Search examples";
gallerySearch.setAttribute("aria-label", "Search Manim examples");
const categorySelect = document.createElement("select");
categorySelect.setAttribute("aria-label", "Filter examples by category");
categorySelect.append(new Option("All categories", "all"));
for (const category of galleryCategories(SCENE_EXAMPLES)) {
  categorySelect.append(new Option(category.replace(/^parity\//, ""), category));
}
const paritySelect = document.createElement("select");
paritySelect.setAttribute("aria-label", "Filter examples by parity status");
paritySelect.append(
  new Option("All parity states", "all"),
  new Option("Parity candidate", "candidate"),
  new Option("Parity qualified", "parity-qualified"),
);
galleryControls.append(gallerySearch, categorySelect, paritySelect);
galleryHead.append(galleryTitle, galleryControls);
const galleryGrid = document.createElement("div");
galleryGrid.className = "gallery-grid";
gallerySection.append(galleryHead, galleryGrid);
workspace.before(gallerySection);

const selectedExampleStrip = document.createElement("div");
selectedExampleStrip.className = "selected-example";
selectedExampleStrip.setAttribute("aria-label", "Selected example");
const selectedCopy = document.createElement("div");
selectedCopy.className = "selected-copy";
const selectedTitle = document.createElement("span");
selectedTitle.className = "selected-title";
const selectedSummary = document.createElement("span");
selectedSummary.className = "selected-summary";
selectedCopy.append(selectedTitle, selectedSummary);
const selectedTags = document.createElement("div");
selectedTags.className = "selected-tags";
selectedExampleStrip.append(selectedCopy, selectedTags);
workspace.before(selectedExampleStrip);

const resetButton = document.createElement("button");
resetButton.type = "button";
resetButton.className = "secondary-button reset-example";
resetButton.textContent = "Reset example";
resetButton.disabled = true;
toolbarActions.prepend(resetButton);

let selectedExampleId = null;
let canonicalSource = "";
let authoringClient = null;
const sceneIdentities = new SceneIdentityMap();
const drafts = new Map();
const sourceCache = new Map();
let player = null;
let rendererBackend = "";

function currentExample() {
  return SCENE_EXAMPLES.find((example) => example.id === selectedExampleId) ?? null;
}

function setRuntimeStatus(message, state) {
  statusText.textContent = message;
  status.dataset.state = state;
}

function setBusy(busy) {
  sceneButton.disabled = busy;
  resetButton.disabled = busy || sceneSourceEditor.value === canonicalSource;
  gallerySearch.disabled = busy;
  categorySelect.disabled = busy;
  paritySelect.disabled = busy;
  for (const card of galleryGrid.querySelectorAll(".example-card")) {
    card.disabled = busy;
  }
}

function showError(error) {
  console.error(error);
  setRuntimeStatus(`Error: ${error}`, "error");
  patchStatus.value = "Runtime failed";
  patchStatus.dataset.state = "error";
}

function showSceneError(error) {
  console.error(error);
  patchStatus.value = `Python failed: ${error}`;
  patchStatus.dataset.state = "error";
}

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

async function loadDemoAuthoringSource(path) {
  if (sourceCache.has(path)) return sourceCache.get(path);
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Unable to load demo Python: HTTP ${response.status}`);
  }
  const source = await response.text();
  sourceCache.set(path, source);
  return source;
}

function renderSelectedMetadata() {
  const example = currentExample();
  if (!example) return;
  selectedTitle.textContent = example.title;
  selectedSummary.textContent = example.summary;
  selectedTags.replaceChildren();
  const parity = document.createElement("span");
  parity.className = "selected-tag parity";
  parity.textContent = parityLabel(example.parityStatus);
  selectedTags.append(parity);
  for (const feature of example.features.slice(0, 6)) {
    const tag = document.createElement("span");
    tag.className = "selected-tag";
    tag.textContent = feature;
    selectedTags.append(tag);
  }
}

function renderGallery() {
  const visible = filterGalleryExamples(SCENE_EXAMPLES, {
    query: gallerySearch.value,
    category: categorySelect.value,
    parityStatus: paritySelect.value,
  });
  galleryGrid.replaceChildren();
  if (visible.length === 0) {
    const empty = document.createElement("div");
    empty.className = "gallery-empty";
    empty.textContent = "No Manim examples match these filters.";
    galleryGrid.append(empty);
    return;
  }
  for (const example of visible) {
    const card = document.createElement("button");
    card.type = "button";
    card.className = "example-card";
    card.dataset.exampleId = example.id;
    card.setAttribute("aria-selected", String(example.id === selectedExampleId));
    const image = document.createElement("img");
    image.className = "example-thumb";
    image.src = example.thumbnail;
    image.alt = example.thumbnailAlt;
    image.loading = "lazy";
    image.decoding = "async";
    const copy = document.createElement("span");
    copy.className = "example-card-copy";
    const title = document.createElement("span");
    title.className = "example-card-title";
    title.textContent = example.title;
    const meta = document.createElement("span");
    meta.className = "example-card-meta";
    const category = document.createElement("span");
    category.className = "example-category";
    category.textContent = example.category.replace(/^parity\//, "");
    const parity = document.createElement("span");
    parity.className = "example-parity";
    parity.textContent = parityLabel(example.parityStatus);
    meta.append(category, parity);
    copy.append(title, meta);
    card.append(image, copy);
    card.addEventListener("click", () => {
      void selectExample(example.id, { run: true, updateUrl: true, scroll: true });
    });
    galleryGrid.append(card);
  }
}

async function runScene() {
  const example = currentExample();
  if (!example || !player) return;
  setBusy(true);
  try {
    patchStatus.value = `Building ${example.title} in the Python worker…`;
    patchStatus.dataset.state = "running";
    authoringClient ??= new PythonAuthoringClient();
    const authored = await authoringClient.run(sceneSourceEditor.value, {});
    if (authored.kind !== "scene_document") {
      throw new Error("Python scene source returned a PatchBatch");
    }

    const runtimeDocument =
      authored.callbacks === null
        ? sceneIdentities.stabilize(authored.document, authored.identities)
        : authored.document;
    const result = await player.reconcileScene(JSON.stringify(runtimeDocument), {
      retainedDocumentJson:
        authored.retainedDocument === null ? null : JSON.stringify(authored.retainedDocument),
      callbacks: authored.callbacks,
      authoringClient,
      loopDurationSeconds: authored.duration > 0 ? authored.duration : null,
    });
    rendererBackend = player.rendererBackend;
    status.dataset.rendererBackend = rendererBackend;
    status.dataset.executionMode = player.mode;
    const report = await player.metrics();
    const operation = result.incremental ? "Scene updated incrementally" : "Scene rebuilt atomically";
    patchStatus.value = `${operation} · ${example.title} · ${report.metrics.objectCount} objects`;
    patchStatus.dataset.state = "applied";
    patchStatus.dataset.exampleId = example.id;
    patchStatus.dataset.parityStatus = example.parityStatus;
    patchStatus.dataset.sequence = String(result.nextPatchSequence);
  } catch (error) {
    showSceneError(error);
  } finally {
    setBusy(false);
  }
}

async function selectExample(
  id,
  { run = false, updateUrl = false, scroll = false } = {},
) {
  const example = SCENE_EXAMPLES.find((candidate) => candidate.id === id);
  if (!example) {
    throw new Error(`Unknown Manim example ${id}`);
  }
  if (selectedExampleId && selectedExampleId !== id && sceneSourceEditor.value !== canonicalSource) {
    drafts.set(selectedExampleId, sceneSourceEditor.value);
  }

  setBusy(true);
  try {
    const source = await loadDemoAuthoringSource(example.path);
    selectedExampleId = id;
    canonicalSource = source;
    sceneSourceEditor.value = drafts.get(id) ?? source;
    resetButton.disabled = sceneSourceEditor.value === canonicalSource;
    renderSelectedMetadata();
    renderGallery();
    patchStatus.value = `${example.title} loaded · ${parityLabel(example.parityStatus)}`;
    patchStatus.dataset.state = "ready";
    patchStatus.dataset.exampleId = example.id;
    patchStatus.dataset.parityStatus = example.parityStatus;
    if (updateUrl) {
      history.pushState({ example: id }, "", exampleUrl(id));
    }
  } finally {
    setBusy(false);
  }

  if (scroll) {
    selectedExampleStrip.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  if (run) await runScene();
}

function refreshGalleryFilters() {
  renderGallery();
}

gallerySearch.addEventListener("input", refreshGalleryFilters);
categorySelect.addEventListener("change", refreshGalleryFilters);
paritySelect.addEventListener("change", refreshGalleryFilters);
sceneButton.addEventListener("click", runScene);
resetButton.addEventListener("click", async () => {
  const example = currentExample();
  if (!example) return;
  drafts.delete(example.id);
  canonicalSource = await loadDemoAuthoringSource(example.path);
  sceneSourceEditor.value = canonicalSource;
  resetButton.disabled = true;
  patchStatus.value = `${example.title} reset to canonical Manim source`;
  patchStatus.dataset.state = "ready";
});
sceneSourceEditor.addEventListener("input", () => {
  const example = currentExample();
  if (example) drafts.set(example.id, sceneSourceEditor.value);
  resetButton.disabled = sceneSourceEditor.value === canonicalSource;
});
window.addEventListener("popstate", () => {
  const id = requestedExampleId();
  if (id && id !== selectedExampleId) {
    void selectExample(id, { run: true, updateUrl: false });
  }
});
document.addEventListener("keydown", (event) => {
  if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
    event.preventDefault();
    void runScene();
  }
});

const EMPTY_SCENE_JSON = '{"version":1,"objects":[],"tracks":[]}';
try {
  player = new AuthoringExecutionClient(canvas, {
    onError(error) {
      showError(error);
    },
  });
  const ready = await player.start(EMPTY_SCENE_JSON, { loopDurationSeconds: 4.0 });
  rendererBackend = ready.render.backend;
  status.dataset.rendererBackend = rendererBackend;
  status.dataset.executionMode = player.mode;
  status.dataset.executionTopology = "authoring-engine-render-workers";

  const requested = requestedExampleId();
  const initialExample = SCENE_EXAMPLES.some((example) => example.id === requested)
    ? requested
    : SCENE_EXAMPLES[0].id;
  history.replaceState({ example: initialExample }, "", exampleUrl(initialExample));
  renderGallery();
  await selectExample(initialExample, { run: true });

  const initialState = await player.state();
  patchStatus.dataset.sequence = String(initialState.nextPatchSequence);

  window.__noonExampleGallery = {
    get selectedExampleId() {
      return selectedExampleId;
    },
    get exampleCount() {
      return SCENE_EXAMPLES.length;
    },
    get executionMode() {
      return player?.mode ?? null;
    },
    async select(id) {
      await selectExample(id, { run: true, updateUrl: true });
    },
  };

  window.addEventListener(
    "pagehide",
    () => {
      authoringClient?.terminate();
      player?.terminate();
    },
    { once: true },
  );

  let lastStatusUpdate = -Infinity;
  let metricsPending = false;
  async function updateWorkerMetrics(timestamp) {
    if (metricsPending || timestamp - lastStatusUpdate <= 200) return;
    metricsPending = true;
    try {
      const report = await player.metrics();
      const metrics = report.metrics;
      const host = report.engineMetrics.host;
      rendererBackend = player.rendererBackend;
      status.dataset.rendererBackend = rendererBackend;
      status.dataset.executionMode = report.executionMode;
      setRuntimeStatus(
        `${metrics.objectCount} objects · ${rendererBackend} ${report.executionMode} worker`,
        "running",
      );
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
