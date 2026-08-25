from pathlib import Path
import re

main = Path("web/main.js")
text = main.read_text()
text = text.replace(
    'import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";\n',
    'import { ExecutionWorkerClient } from "./execution-worker-client.js";\n',
    1,
)
text = text.replace('import { SampleWindow } from "./frame-metrics.js";\n', "", 1)
block = r"try \{\n  await init\(\);.*?\n\} catch \(error\) \{\n  showError\(error\);\n\}\n?$"
replacement = r'''const EMPTY_SCENE_JSON = '{"version":1,"objects":[],"tracks":[]}';

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
'''
text, count = re.subn(block, replacement, text, flags=re.S)
if count != 1:
    raise SystemExit(f"playground runtime replacement count {count}")
main.write_text(text)

build = Path("scripts/build-web-demo.sh")
text = build.read_text()
anchor = "node --check web/native-inputs.js\n"
checks = """node --check web/execution-transport.js
node --check web/execution-engine-worker.js
node --check web/execution-render-worker.js
node --check web/execution-worker-client.js
node --check scripts/execution-worker-smoke.mjs
node --check scripts/execution-worker-host-smoke.mjs
"""
if "node --check web/execution-engine-worker.js" not in text:
    if anchor not in text:
        raise SystemExit("build script JS anchor missing")
    text = text.replace(anchor, anchor + checks, 1)
if "node --test web/execution-transport.test.mjs" not in text:
    text = text.replace(
        "node --test web/authoring-client.test.mjs\n",
        "node --test web/execution-transport.test.mjs\nnode --test web/authoring-client.test.mjs\n",
        1,
    )
build.write_text(text)

ci = Path(".github/workflows/ci.yml")
text = ci.read_text()
anchor = """      - name: Test Python updater callback bridge
        run: node scripts/updater-callback-smoke.mjs
"""
if "Test engine/render worker transports" not in text:
    if anchor not in text:
        raise SystemExit("CI updater anchor missing")
    text = text.replace(
        anchor,
        anchor
        + """
      - name: Test engine/render worker transports
        run: node scripts/execution-worker-smoke.mjs

      - name: Test slow Python callbacks do not block rendering
        run: node scripts/execution-worker-host-smoke.mjs
""",
        1,
    )
ci.write_text(text)

docs = Path("docs/execution-worker-transport.md")
text = docs.read_text()
heading = "## Current migration boundary"
if heading in text:
    text = text[: text.index(heading)].rstrip() + "\n\n"
text += """## Default browser path and host callbacks

The playground uses `ExecutionWorkerClient` by default. `NoonCanvasPlayer` remains as a compatibility/profiling path, but the normal UI thread no longer evaluates scene state or submits GPU work. It authors scenes, forwards explicit edits, collects DOM input, and polls worker metrics.

Arbitrary Python updater closures remain in the lazy Pyodide authoring worker. When a scene registers callbacks, the UI transfers a dedicated `MessagePort` between that Python worker and the engine worker. Callback snapshots and patch results then travel directly between those workers; the main thread is not in the per-frame callback loop. The engine launches at most one callback phase at a time, records missed presentation deadlines, drops results that became stale while native time advanced, and commits on-time callback batches through a sequence domain separate from interactive user patches. The render worker never waits for that callback.

Callback scenes currently retain authoring-local object IDs so Python closures and their coherent snapshot table refer to the same identities. Stable callback-aware hot-reload identity reconciliation belongs to #64; ordinary callback-free scenes continue using the playground stable identity adapter.
"""
docs.write_text(text)
