import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { PlaygroundGeneration } from "./playground-generation.js";
import { PlaygroundPlaybackControls } from "./playground-playback-controls.js";
import { SceneIdentityMap } from "./scene-identity.js";
import {
  exampleUrl,
  filterGalleryExamples,
  galleryCategories,
  loadGalleryManifest,
  parityLabel,
  requestedExampleId,
} from "./example-gallery.js";

let canvas = document.querySelector("#scene");
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
const workspace = document.querySelector(".workspace");
const toolbarActions = document.querySelector(".actions");

// The public demo is a source-compatible Python authoring surface. Noon-native patch
// templates and implementation fixtures remain repository assets, not user-facing examples.
patchButton.hidden = true;
patchTab.hidden = true;
patchPanel.hidden = true;
sourceEditor.hidden = true;
sceneTab.hidden = true;
sceneButton.textContent = "Run";

let pythonEditorLoadPromise = null;
function loadEnhancedPythonEditor() {
  pythonEditorLoadPromise ??= import("./python-editor.js").catch((error) => {
    console.warn("Optional Python editor unavailable; using textarea fallback", error);
  });
  return pythonEditorLoadPromise;
}

const GALLERY_PAGE_SIZE = 18;
const METRICS_POLL_MS = 500;

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
  .gallery-pager {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.7rem;
    min-height: 3rem;
    padding: 0.55rem 0.85rem;
    border-top: 1px solid var(--border);
    background: rgb(9 13 21 / 72%);
  }
  .gallery-pager[hidden] { display: none; }
  .gallery-pager-status {
    color: var(--muted-2);
    font: 0.68rem ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .gallery-pager-actions {
    display: flex;
    gap: 0.4rem;
  }
  .gallery-page-button {
    border: 1px solid #303d58;
    border-radius: 0.55rem;
    padding: 0.38rem 0.58rem;
    background: #111722;
    color: #dbe2ef;
    cursor: pointer;
    font-size: 0.7rem;
    font-weight: 700;
  }
  .gallery-page-button:disabled {
    cursor: default;
    opacity: 0.42;
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
    content-visibility: auto;
    contain-intrinsic-size: auto 12rem;
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
    .gallery-pager { align-items: stretch; flex-direction: column; }
    .gallery-pager-actions { width: 100%; }
    .gallery-page-button { flex: 1; }
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
  throw new Error("No runnable examples are ready for the gallery");
}

const gallerySection = document.createElement("section");
gallerySection.className = "example-gallery";
gallerySection.setAttribute("aria-label", "Animation scene examples");
const galleryHead = document.createElement("div");
galleryHead.className = "gallery-head";
const galleryTitle = document.createElement("div");
galleryTitle.className = "gallery-title";
galleryTitle.innerHTML = `<strong>Examples</strong><span>Python-authored animation scenes</span>`;
const galleryControls = document.createElement("div");
galleryControls.className = "gallery-controls";
const gallerySearch = document.createElement("input");
gallerySearch.type = "search";
gallerySearch.placeholder = "Search examples";
gallerySearch.setAttribute("aria-label", "Search examples");
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
const galleryPager = document.createElement("div");
galleryPager.className = "gallery-pager";
const galleryPagerStatus = document.createElement("span");
galleryPagerStatus.className = "gallery-pager-status";
galleryPagerStatus.setAttribute("aria-live", "polite");
const galleryPagerActions = document.createElement("div");
galleryPagerActions.className = "gallery-pager-actions";
const previousGalleryPage = document.createElement("button");
previousGalleryPage.type = "button";
previousGalleryPage.className = "gallery-page-button";
previousGalleryPage.textContent = "Previous";
const nextGalleryPage = document.createElement("button");
nextGalleryPage.type = "button";
nextGalleryPage.className = "gallery-page-button";
nextGalleryPage.textContent = "Next";
galleryPagerActions.append(previousGalleryPage, nextGalleryPage);
galleryPager.append(galleryPagerStatus, galleryPagerActions);
gallerySection.append(galleryHead, galleryGrid, galleryPager);
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
const generations = new PlaygroundGeneration();
let player = null;
let playbackControls = null;
let playbackDurationSeconds = 4.0;
let rendererBackend = "";
let sceneRunPromise = null;
let runtimeStartPromise = null;
let runtimePreparation = null;
let playerNeedsRestart = false;
let busyDepth = 0;
let galleryPage = 0;
let metricsTimer = null;
let metricsPending = false;

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
  previousGalleryPage.disabled = busy || galleryPage === 0;
  if (busy) {
    nextGalleryPage.disabled = true;
  }
  playbackControls?.setBusy(busy);
  for (const card of galleryGrid.querySelectorAll(".example-card")) {
    card.disabled = busy;
  }
  if (!busy) {
    updateGalleryPagerControls();
  }
}

function beginBusy() {
  busyDepth += 1;
  setBusy(true);
  let released = false;
  return () => {
    if (released) return;
    released = true;
    busyDepth = Math.max(0, busyDepth - 1);
    if (busyDepth === 0) setBusy(false);
  };
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

function showRecoverableSceneError(error) {
  console.warn("Recoverable Python callback error", error);
  patchStatus.value = `Python callback failed: ${error}`;
  patchStatus.dataset.state = "error";
}

function showPlaybackError(error) {
  console.error(error);
  patchStatus.value = `Playback failed: ${error}`;
  patchStatus.dataset.state = "error";
}

function recordStale(token, stage) {
  const diagnostics = generations.recordStale(token, stage);
  status.dataset.staleResults = String(diagnostics.staleDrops);
  status.dataset.lastStaleStage = diagnostics.lastStale?.stage ?? "";
  console.debug(
    `[Noon playground] dropped stale ${token?.kind ?? "unknown"} result`,
    diagnostics.lastStale,
  );
  return { stale: true, stage };
}

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function ensureAuthoringClient() {
  if (authoringClient?.terminated) {
    authoringClient = null;
  }
  authoringClient ??= new PythonAuthoringClient();
  return authoringClient;
}

function adoptRuntimeCanvas(candidate) {
  if (canvas !== candidate.canvas) {
    canvas = candidate.canvas;
  }
}

function createRuntimeClient() {
  let candidate = null;
  candidate = new AuthoringExecutionClient(canvas, {
    onError(error) {
      if (player !== candidate) return;
      playerNeedsRestart = true;
      showError(error);
    },
    onRecoverableError(error) {
      showRecoverableSceneError(error);
    },
  });
  return candidate;
}

function updatePlaybackControls({ supported, player: nextPlayer, durationSeconds }) {
  status.dataset.playbackControls = supported ? "available" : "unavailable";
  if (!supported) {
    playbackControls?.destroy();
    playbackControls = null;
    return;
  }
  if (playbackControls === null) {
    playbackControls = new PlaygroundPlaybackControls(
      nextPlayer,
      document.querySelector(".preview-pane"),
      { durationSeconds, onError: showPlaybackError },
    );
  } else {
    playbackControls.setDuration(durationSeconds);
  }
}

function ensureRuntimePreparation() {
  if (player !== null) return null;
  if (runtimePreparation !== null) return runtimePreparation;

  const candidate = createRuntimeClient();
  const ready = candidate.prepare();
  const preparation = { candidate, ready };
  runtimePreparation = preparation;
  status.dataset.runtimeStartup = "preparing-on-run";
  status.dataset.executionTopology = "preparing-render-owner";

  ready.then(
    () => {
      if (runtimePreparation === preparation) {
        status.dataset.runtimeStartup = "prepared-on-run";
        status.dataset.executionTopology = "prepared-render-owner";
      }
    },
    (error) => {
      adoptRuntimeCanvas(candidate);
      if (runtimePreparation === preparation) {
        runtimePreparation = null;
      }
      console.warn("Execution runtime preparation failed", error);
    },
  );
  return preparation;
}

async function ensureRuntimeReady({
  preparation = null,
  semanticExecution = null,
  sceneJson,
  sceneSpecJson,
  startRetained,
  callbacks,
  authoringClient: client,
  loopDurationSeconds,
}) {
  if (runtimeStartPromise !== null) return runtimeStartPromise;
  if (player !== null) return null;

  const task = (async () => {
    if (startRetained && callbacks !== null && callbacks !== undefined) {
      throw new Error(
        "retained authoring with Python host callbacks is not supported yet; " +
          "split the callback work from retained text instead of silently dropping either",
      );
    }

    const prepared = preparation ?? ensureRuntimePreparation();
    if (prepared === null) {
      throw new Error("execution runtime preparation is unavailable");
    }
    const nextPlayer = prepared.candidate;
    try {
      await prepared.ready;
      setRuntimeStatus("Preparing animation…", "running");
      patchStatus.value = "Preparing authored animation…";
      patchStatus.dataset.state = "running";

      const ready = semanticExecution !== null
        ? await nextPlayer.startSemanticExecution(semanticExecution, {
            authoringClient: client,
            loopDurationSeconds,
          })
        : startRetained
          ? await nextPlayer.startRetainedCanonical(sceneSpecJson, {
              loopDurationSeconds,
            })
          : await nextPlayer.start(sceneJson, {
              loopDurationSeconds,
              callbacks,
              authoringClient: client,
            });
      const initialState = await nextPlayer.state();

      player = nextPlayer;
      if (runtimePreparation === prepared) {
        runtimePreparation = null;
      }
      playerNeedsRestart = false;
      rendererBackend = ready.render.backend;
      status.dataset.rendererBackend = rendererBackend;
      status.dataset.executionMode = nextPlayer.mode;
      status.dataset.executionTopology =
        semanticExecution === null
          ? "authoring-engine-render-workers"
          : "python-semantic-engine-render-worker";
      status.dataset.runtimeStartup = "started-on-demand";
      const sourceOwnsExecution = semanticExecution?.continuationGeneration != null;
      status.dataset.playbackControls = sourceOwnsExecution ? "unavailable" : "available";
      if (!sourceOwnsExecution) {
        playbackControls = new PlaygroundPlaybackControls(
          nextPlayer,
          document.querySelector(".preview-pane"),
          { durationSeconds: loopDurationSeconds, onError: showPlaybackError },
        );
      }

      patchStatus.dataset.sequence = String(initialState.nextPatchSequence);
      if (playbackControls !== null) {
        playbackControls.sync({
          time: initialState.time,
          playing: initialState.playing,
          durationSeconds: loopDurationSeconds,
        });
      }
      startMetricsPolling();
      return {
        ...initialState,
        incremental: false,
        rebuilt: true,
        mode: nextPlayer.mode,
      };
    } catch (error) {
      if (runtimePreparation === prepared) {
        runtimePreparation = null;
      }
      nextPlayer.terminate();
      adoptRuntimeCanvas(nextPlayer);
      if (player === nextPlayer) {
        player = null;
        playbackControls?.destroy();
        playbackControls = null;
      }
      throw error;
    }
  })();

  runtimeStartPromise = task;
  try {
    return await task;
  } finally {
    if (runtimeStartPromise === task) {
      runtimeStartPromise = null;
    }
  }
}

async function ensureExecutionReady() {
  if (!playerNeedsRestart) return;
  patchStatus.value = "Restarting runtime workers…";
  patchStatus.dataset.state = "running";
  setRuntimeStatus("Restarting runtime workers…", "running");
  const ready = await player.restart();
  playerNeedsRestart = false;
  rendererBackend = ready.render.backend;
  status.dataset.rendererBackend = rendererBackend;
  status.dataset.executionMode = player.mode;
  const playbackState = await player.state();
  playbackControls?.sync({
    time: playbackState.time,
    playing: playbackState.playing,
    durationSeconds: playbackDurationSeconds,
  });
}

async function runPlaygroundTestHook(name, payload) {
  const hook = globalThis.__NOON_PLAYGROUND_TEST_HOOKS__?.[name];
  if (typeof hook === "function") {
    await hook(payload);
  }
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

function filteredGalleryExamples() {
  return filterGalleryExamples(SCENE_EXAMPLES, {
    query: gallerySearch.value,
    category: categorySelect.value,
    parityStatus: paritySelect.value,
  });
}

function updateGalleryPagerControls(visible = filteredGalleryExamples()) {
  const pageCount = Math.max(1, Math.ceil(visible.length / GALLERY_PAGE_SIZE));
  galleryPage = Math.min(Math.max(0, galleryPage), pageCount - 1);
  const start = visible.length === 0 ? 0 : galleryPage * GALLERY_PAGE_SIZE + 1;
  const end = Math.min((galleryPage + 1) * GALLERY_PAGE_SIZE, visible.length);
  galleryPager.hidden = visible.length <= GALLERY_PAGE_SIZE;
  galleryPagerStatus.textContent = visible.length === 0 ? "0 examples" : `${start}–${end} of ${visible.length}`;
  previousGalleryPage.disabled = busyDepth > 0 || galleryPage === 0;
  nextGalleryPage.disabled = busyDepth > 0 || galleryPage >= pageCount - 1;
}

function renderGallery({ keepSelectedVisible = false } = {}) {
  const visible = filteredGalleryExamples();
  if (keepSelectedVisible && selectedExampleId !== null) {
    const selectedIndex = visible.findIndex((example) => example.id === selectedExampleId);
    if (selectedIndex >= 0) {
      galleryPage = Math.floor(selectedIndex / GALLERY_PAGE_SIZE);
    }
  }

  const pageCount = Math.max(1, Math.ceil(visible.length / GALLERY_PAGE_SIZE));
  galleryPage = Math.min(Math.max(0, galleryPage), pageCount - 1);
  galleryGrid.replaceChildren();
  updateGalleryPagerControls(visible);
  if (visible.length === 0) {
    const empty = document.createElement("div");
    empty.className = "gallery-empty";
    empty.textContent = "No examples match these filters.";
    galleryGrid.append(empty);
    return;
  }

  const start = galleryPage * GALLERY_PAGE_SIZE;
  const pageExamples = visible.slice(start, start + GALLERY_PAGE_SIZE);
  for (const example of pageExamples) {
    const card = document.createElement("button");
    card.type = "button";
    card.className = "example-card";
    card.dataset.exampleId = example.id;
    card.disabled = busyDepth > 0;
    card.setAttribute("aria-selected", String(example.id === selectedExampleId));
    const image = document.createElement("img");
    image.className = "example-thumb";
    image.src = example.thumbnail;
    image.alt = example.thumbnailAlt;
    image.loading = "lazy";
    image.decoding = "async";
    image.fetchPriority = "low";
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

function isCurrentRun(runToken) {
  return generations.isRunCurrent(runToken, selectedExampleId);
}

async function discardSemanticExecution(authored, client) {
  const contextId = authored?.semanticExecution?.contextId;
  if (typeof contextId === "string") {
    try {
      await client.releaseSemanticExecution(contextId);
    } catch (error) {
      console.warn(`Failed to release discarded semantic context ${contextId}`, error);
    }
  }
}

function discardEarlyContinuationRuntime(attachedPlayer) {
  attachedPlayer?.terminate();
  if (player !== attachedPlayer) return;
  playbackControls?.destroy();
  playbackControls = null;
  player = null;
  playerNeedsRestart = false;
  stopMetricsPolling();
}

function sameSemanticContinuation(left, right) {
  return left?.contextId === right?.contextId &&
    left?.continuationGeneration === right?.continuationGeneration;
}

async function runScene() {
  if (sceneRunPromise !== null) return sceneRunPromise;
  const example = currentExample();
  if (!example) return null;

  const runToken = generations.beginRun(example.id);
  const source = sceneSourceEditor.value;
  const releaseBusy = beginBusy();
  const task = (async () => {
    let earlyContinuation = null;
    try {
      setRuntimeStatus("Preparing animation…", "running");
      patchStatus.value = `Building ${example.title} in the Python worker…`;
      patchStatus.dataset.state = "running";
      const preparation = player === null ? ensureRuntimePreparation() : null;
      const client = ensureAuthoringClient();
      status.dataset.authoringWarmup = "started";
      let authored;
      try {
        authored = await client.run(source, {
          playground: {
            example_id: example.id,
            selection_generation: runToken.selectionGeneration,
            run_generation: runToken.runGeneration,
          },
        }, {
          async onSemanticContinuation(registration) {
            if (earlyContinuation !== null) {
              throw new Error("Python source registered more than one semantic continuation");
            }
            if (!isCurrentRun(runToken)) {
              throw new Error("Python semantic continuation belongs to a stale playground run");
            }
            const loopDurationSeconds = registration.duration > 0
              ? registration.duration
              : playbackDurationSeconds;
            let result;
            if (player === null) {
              result = await ensureRuntimeReady({
                preparation,
                semanticExecution: registration.semanticExecution,
                sceneJson: null,
                sceneSpecJson: null,
                startRetained: false,
                callbacks: null,
                authoringClient: client,
                loopDurationSeconds,
              });
            } else {
              await ensureExecutionReady();
              result = await player.reconcileSemanticExecution(
                registration.semanticExecution,
                {
                  authoringClient: client,
                  loopDurationSeconds: registration.duration > 0
                    ? registration.duration
                    : null,
                },
              );
            }
            const attachedPlayer = player;
            if (!isCurrentRun(runToken)) {
              discardEarlyContinuationRuntime(attachedPlayer);
              throw new Error("Python semantic continuation was superseded during startup");
            }
            earlyContinuation = { registration, attachedPlayer, result };
          },
        });
        status.dataset.authoringWarmup = "ready";
      } catch (error) {
        status.dataset.authoringWarmup = "failed";
        if (client.terminated && authoringClient === client) {
          authoringClient = null;
        }
        throw error;
      }

      await runPlaygroundTestHook("afterAuthoring", {
        exampleId: example.id,
        selectionGeneration: runToken.selectionGeneration,
        runGeneration: runToken.runGeneration,
      });
      if (!isCurrentRun(runToken)) {
        discardEarlyContinuationRuntime(earlyContinuation?.attachedPlayer);
        await discardSemanticExecution(authored, client);
        return recordStale(runToken, "after-authoring");
      }
      if (authored.kind !== "scene_document") {
        throw new Error("Python scene source returned a PatchBatch");
      }

      const semanticExecution = authored.semanticExecution ?? null;
      const runtimeDocument =
        semanticExecution !== null
          ? null
          : authored.callbacks === null
            ? sceneIdentities.stabilize(authored.document, authored.identities)
            : authored.document;
      const runtimeSceneSpec =
        semanticExecution !== null ||
        authored.sceneSpec === null ||
        authored.sceneSpec === undefined
          ? null
          : sceneIdentities.stabilizeSceneSpec(authored.sceneSpec, authored.identities);
      const sceneJson = runtimeDocument === null ? null : JSON.stringify(runtimeDocument);
      const sceneSpecJson = runtimeSceneSpec === null ? null : JSON.stringify(runtimeSceneSpec);
      const startRetained = semanticExecution === null && sceneSpecJson !== null;
      const loopDurationSeconds = authored.duration > 0 ? authored.duration : playbackDurationSeconds;

      if (player !== null) {
        updatePlaybackControls({
          supported: semanticExecution?.continuationGeneration == null,
          player,
          durationSeconds: loopDurationSeconds,
        });
      }

      await runPlaygroundTestHook("beforeReconcile", {
        exampleId: example.id,
        selectionGeneration: runToken.selectionGeneration,
        runGeneration: runToken.runGeneration,
      });
      if (!isCurrentRun(runToken)) {
        discardEarlyContinuationRuntime(earlyContinuation?.attachedPlayer);
        await discardSemanticExecution(authored, client);
        return recordStale(runToken, "before-reconcile");
      }

      let result;
      if (earlyContinuation !== null) {
        if (!sameSemanticContinuation(
          authored.semanticExecution,
          earlyContinuation.registration.semanticExecution,
        )) {
          discardEarlyContinuationRuntime(earlyContinuation.attachedPlayer);
          throw new Error("final Python result does not match its early semantic continuation");
        }
        if (player !== earlyContinuation.attachedPlayer) {
          throw new Error("semantic continuation runtime changed before final adoption");
        }
        const finalState = await player.state();
        result = {
          ...earlyContinuation.result,
          ...finalState,
        };
        earlyContinuation = null;
      } else if (player === null) {
        result = await ensureRuntimeReady({
          preparation,
          semanticExecution,
          sceneJson,
          sceneSpecJson,
          startRetained,
          callbacks: authored.callbacks,
          authoringClient: client,
          loopDurationSeconds,
        });
        if (!isCurrentRun(runToken)) return recordStale(runToken, "after-runtime-start");
      } else {
        await ensureExecutionReady();
        if (!isCurrentRun(runToken)) {
          await discardSemanticExecution(authored, client);
          return recordStale(runToken, "after-restart");
        }
        result = semanticExecution !== null
          ? await player.reconcileSemanticExecution(semanticExecution, {
              authoringClient: client,
              loopDurationSeconds: authored.duration > 0 ? authored.duration : null,
            })
          : await player.reconcileScene(sceneJson, {
              sceneSpecJson,
              callbacks: authored.callbacks,
              authoringClient: client,
              loopDurationSeconds: authored.duration > 0 ? authored.duration : null,
            });
        if (!isCurrentRun(runToken)) return recordStale(runToken, "after-reconcile");
      }

      if (authored.duration > 0) {
        playbackDurationSeconds = authored.duration;
      }
      playbackControls?.sync({
        time: result.time,
        playing: result.playing,
        durationSeconds: playbackDurationSeconds,
      });

      rendererBackend = player.rendererBackend;
      status.dataset.rendererBackend = rendererBackend;
      status.dataset.executionMode = player.mode;
      const report = await player.metrics();
      if (!isCurrentRun(runToken)) return recordStale(runToken, "after-metrics");

      const operation = result.incremental ? "Scene updated incrementally" : "Scene rebuilt atomically";
      patchStatus.value = `${operation} · ${example.title} · ${report.metrics.objectCount} objects`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.exampleId = example.id;
      patchStatus.dataset.parityStatus = example.parityStatus;
      patchStatus.dataset.sequence = String(result.nextPatchSequence);
      return { stale: false, result };
    } catch (error) {
      discardEarlyContinuationRuntime(earlyContinuation?.attachedPlayer);
      if (!isCurrentRun(runToken)) {
        return recordStale(runToken, "error");
      }
      if (player === null || playerNeedsRestart) {
        showError(error);
      } else {
        showSceneError(error);
      }
      return { stale: false, error };
    }
  })();

  sceneRunPromise = task;
  try {
    return await task;
  } finally {
    if (sceneRunPromise === task) {
      sceneRunPromise = null;
    }
    releaseBusy();
  }
}

async function selectExample(
  id,
  { run = false, updateUrl = false, scroll = false } = {},
) {
  const example = SCENE_EXAMPLES.find((candidate) => candidate.id === id);
  if (!example) {
    throw new Error(`Unknown example ${id}`);
  }

  const requestToken = generations.beginSelectionRequest(id);
  const releaseBusy = beginBusy();
  let selectionToken = null;
  try {
    const source = await loadDemoAuthoringSource(example.path);
    if (!generations.isSelectionRequestCurrent(requestToken)) {
      return recordStale(requestToken, "after-source-load");
    }

    selectionToken = generations.commitSelection(requestToken);
    if (selectionToken === null) {
      return recordStale(requestToken, "before-selection-commit");
    }

    // If the prior scene has already entered reconciliation, let it settle while
    // the prior selection is still the visible/active one. The generation bump
    // above prevents authoring that has not reconciled yet from committing at all.
    const priorRun = sceneRunPromise;
    if (priorRun !== null) {
      await priorRun;
    }
    if (
      !generations.isSelectionRequestCurrent(requestToken) ||
      !generations.isSelectionCurrent(selectionToken)
    ) {
      return recordStale(selectionToken, "after-prior-run");
    }

    if (selectedExampleId && selectedExampleId !== id && sceneSourceEditor.value !== canonicalSource) {
      drafts.set(selectedExampleId, sceneSourceEditor.value);
    }
    selectedExampleId = id;
    canonicalSource = source;
    sceneSourceEditor.value = drafts.get(id) ?? source;
    resetButton.disabled = sceneSourceEditor.value === canonicalSource;
    renderSelectedMetadata();
    renderGallery({ keepSelectedVisible: true });
    patchStatus.value = `${example.title} loaded · ${parityLabel(example.parityStatus)}`;
    patchStatus.dataset.state = "ready";
    patchStatus.dataset.exampleId = example.id;
    patchStatus.dataset.parityStatus = example.parityStatus;
    if (updateUrl) {
      history.pushState({ example: id }, "", exampleUrl(id));
    }
  } catch (error) {
    if (generations.isSelectionRequestCurrent(requestToken)) {
      showSceneError(error);
    } else {
      recordStale(requestToken, "selection-error");
    }
    return { stale: false, error };
  } finally {
    releaseBusy();
  }

  if (
    selectionToken === null ||
    !generations.isSelectionRequestCurrent(requestToken) ||
    !generations.isSelectionCurrent(selectionToken)
  ) {
    return recordStale(selectionToken ?? requestToken, "before-selection-run");
  }
  if (scroll) {
    selectedExampleStrip.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  if (run) return runScene();
  return { stale: false };
}

function refreshGalleryFilters() {
  galleryPage = 0;
  renderGallery();
}

function moveGalleryPage(delta) {
  const visible = filteredGalleryExamples();
  const pageCount = Math.max(1, Math.ceil(visible.length / GALLERY_PAGE_SIZE));
  galleryPage = Math.min(Math.max(0, galleryPage + delta), pageCount - 1);
  renderGallery();
  gallerySection.scrollIntoView({ block: "start" });
}

gallerySearch.addEventListener("input", refreshGalleryFilters);
categorySelect.addEventListener("change", refreshGalleryFilters);
paritySelect.addEventListener("change", refreshGalleryFilters);
previousGalleryPage.addEventListener("click", () => moveGalleryPage(-1));
nextGalleryPage.addEventListener("click", () => moveGalleryPage(1));
sceneButton.addEventListener("click", runScene);
resetButton.addEventListener("click", () => {
  const example = currentExample();
  if (!example) return;
  drafts.delete(example.id);
  sceneSourceEditor.value = canonicalSource;
  resetButton.disabled = true;
  patchStatus.value = `${example.title} reset to canonical source`;
  patchStatus.dataset.state = "ready";
});
sceneSourceEditor.addEventListener(
  "focus",
  () => {
    void loadEnhancedPythonEditor();
  },
  { once: true },
);
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

async function updateWorkerMetrics() {
  if (
    metricsPending ||
    player === null ||
    document.visibilityState === "hidden" ||
    busyDepth > 0 ||
    sceneRunPromise !== null ||
    playerNeedsRestart
  ) {
    return;
  }
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
    playbackControls?.updateTime(metrics.time);
    status.dataset.instances = String(metrics.instancesDrawn);
    status.dataset.uploadBytes = String(metrics.bytesUploaded);
    status.dataset.geometryCacheMisses = String(metrics.geometryCacheMisses);
    status.dataset.hostMissedDeadlines = String(host.missedDeadlines);
    status.dataset.hostDroppedLateResults = String(host.droppedLateResults);
    status.dataset.presentedFrames = String(metrics.presentedFrames);
  } catch (error) {
    if (!playerNeedsRestart) {
      showError(error);
    }
  } finally {
    metricsPending = false;
  }
}

function stopMetricsPolling() {
  if (metricsTimer !== null) {
    clearTimeout(metricsTimer);
    metricsTimer = null;
  }
}

function startMetricsPolling() {
  if (metricsTimer !== null || player === null || document.visibilityState === "hidden") return;
  const poll = async () => {
    metricsTimer = null;
    await updateWorkerMetrics();
    if (player !== null && document.visibilityState !== "hidden") {
      metricsTimer = setTimeout(poll, METRICS_POLL_MS);
    }
  };
  metricsTimer = setTimeout(poll, METRICS_POLL_MS);
}

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden") {
    stopMetricsPolling();
  } else {
    startMetricsPolling();
  }
});

try {
  const requested = requestedExampleId();
  const initialExample = SCENE_EXAMPLES.some((example) => example.id === requested)
    ? requested
    : SCENE_EXAMPLES[0].id;
  history.replaceState({ example: initialExample }, "", exampleUrl(initialExample));
  renderGallery();
  await selectExample(initialExample, { run: false });
  patchStatus.dataset.sequence = "0";
  status.dataset.runtimeStartup = "deferred";
  status.dataset.executionTopology = "deferred-until-run";
  setRuntimeStatus("Ready · runtime starts when you run an example", "ready");

  window.__noonExampleGallery = {
    get selectedExampleId() {
      return selectedExampleId;
    },
    get exampleCount() {
      return SCENE_EXAMPLES.length;
    },
    get visibleExampleCount() {
      return galleryGrid.querySelectorAll(".example-card").length;
    },
    get executionMode() {
      return player?.mode ?? null;
    },
    get runInFlight() {
      return sceneRunPromise !== null;
    },
    get generationDiagnostics() {
      return generations.diagnostics;
    },
    async select(id) {
      return selectExample(id, { run: true, updateUrl: true });
    },
    async run() {
      return runScene();
    },
  };

  window.addEventListener(
    "pagehide",
    () => {
      stopMetricsPolling();
      playbackControls?.destroy();
      playbackControls = null;
      authoringClient?.terminate();
      authoringClient = null;
      const preparation = runtimePreparation;
      runtimePreparation = null;
      preparation?.candidate.terminate();
      player?.terminate();
      player = null;
    },
    { once: true },
  );
} catch (error) {
  showError(error);
}
