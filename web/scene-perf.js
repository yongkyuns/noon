import { PythonAuthoringClient } from "./authoring-client.js";
import { ProvenancedPythonAuthoringClient } from "./provenanced-authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { FrameMetrics } from "./frame-metrics.js";
import { shouldSampleRendererStageFrame } from "./renderer-stage-sampling.js";

const parameters = new URLSearchParams(location.search);
const sourcePath = parameters.get("source") ?? "./python/demo_scene.py";
if (!sourcePath.startsWith("./python/") || !sourcePath.endsWith(".py")) {
  throw new Error("scene performance source must be a local ./python/*.py file");
}
const warmupFrames = positiveInteger("warmup", 30, 0);
const measuredFrames = positiveInteger("frames", 180);
const targetHz = positiveNumber("targetHz", 60);
const transportMode = parameters.get("transportMode") ?? "transferable";
if (!["transferable", "shared"].includes(transportMode)) throw new Error("unsupported transportMode");
const sharedSlotCapacity = parameters.has("sharedSlotCapacity")
  ? positiveInteger("sharedSlotCapacity") : undefined;
const samples = parameters.get("includeSamples") === "1" ? [] : null;
const rendererSamples = parameters.get("includeRendererSamples") === "1" ? [] : null;
const rendererMetricsSampling = parameters.get("rendererMetricsSampling") ?? "dense";
if (!["dense", "sparse"].includes(rendererMetricsSampling)) {
  throw new Error("unsupported renderer metrics sampling mode");
}
const rendererPublicationStageSamples = rendererSamples === null ? null : [];
const rendererSubstageSamples = rendererSamples === null ? null : [];
let rendererPublicationStageCursor = null;
let rendererSubstageCursor = null;
const MAX_RENDERER_PUBLICATION_STAGE_SAMPLES = 32;
const MAX_RENDERER_SUBSTAGE_SAMPLES = 32;
const stageTimingSamples = parameters.get("includeStageTimings") === "1" ? [] : null;
const context = parseContext(parameters.get("context"));
const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const output = document.querySelector("#json");

let client = null;
let execution = null;
let jank = null;
let sourceError = null;
let completedSource = null;
let continuation = false;
let sourceCompleted = false;
let runtimeBuildIdentity = null;
let lastSampleTime = 0;
let firstMeasuredTime = null;
let rejectSourceFailure;
const sourceFailure = new Promise((_, reject) => { rejectSourceFailure = reject; });
// Source and execution are independent asynchronous endpoints. Observe failure
// even while an execution request is waiting for a continuation that has failed.
sourceFailure.catch(() => {});
function failSource(error) {
  sourceError = error;
  rejectSourceFailure(error);
}
try {
  const source = await loadText(sourcePath);
  const workerStarted = performance.now();
  client = rendererSamples === null
    ? new PythonAuthoringClient()
    : new ProvenancedPythonAuthoringClient();
  const readyIdentity = await client.ready();
  if (rendererSamples !== null) runtimeBuildIdentity = readyIdentity;
  const workerStartupMs = performance.now() - workerStarted;
  execution = new AuthoringExecutionClient(canvas, {
    onError: failSource,
  });
  status.value = `Authoring ${sourcePath}…`;
  const authorStarted = performance.now();
  let resolveAttached, rejectAttached;
  const attached = new Promise((resolve, reject) => {
    resolveAttached = resolve;
    rejectAttached = reject;
  });
  let attaching = false;
  async function attach(descriptor, isContinuation) {
    if (attaching) return;
    attaching = true;
    if (!descriptor) throw new Error("scene profiler requires shared semantic execution");
    continuation = isContinuation;
    const ready = await execution.startSemanticExecution(descriptor, {
      authoringClient: client,
      transportMode,
      ...(sharedSlotCapacity === undefined ? {} : { sharedSlotCapacity }),
      ...(continuation ? { pacing: "external_samples" } : { initiallyPaused: true }),
    });
    resolveAttached(ready);
  }
  // Source execution may remain suspended across play/wait. Attach to its
  // existing session, then let exact samples advance Rust's continuation lane.
  void client.run(source, context, {
    onSemanticContinuation: (registration) => attach(registration.semanticExecution, true),
  }).then(async (result) => {
    completedSource = result;
    await attach(result.semanticExecution, false);
  }).catch((error) => {
    failSource(error);
    rejectAttached(error);
  });
  const ready = await Promise.race([attached, sourceFailure]);
  if (stageTimingSamples !== null && !continuation) {
    throw new Error("stage timing samples require a source-owned semantic continuation");
  }
  const initialExecutionReadyMs = performance.now() - authorStarted;
  if (!continuation && (await execution.state()).time > 0) {
    // Predeclared deterministic sources may hand off an already-completed
    // session. Its ordinary seek API validates replay eligibility; opaque
    // callback programs still fail explicitly instead of being replayed.
    await execution.seek(0);
  }
  await advanceSample(0);

  // Continue forward through warmup: arbitrary host callbacks cannot be
  // implicitly rewound/replayed to reset a benchmark clock.
  for (let frame = 0; frame < warmupFrames && !sourceCompleted; frame += 1) {
    status.value = `Warm-up ${frame + 1}/${warmupFrames} · ${sourcePath}…`;
    await nextAnimationFrame();
    await advanceSample((frame + 1) / targetHz);
  }
  const before = (await execution.metrics({
    profilePublicationStages: rendererSamples !== null,
    profileRenderSubstages: rendererSamples !== null,
  })).metrics;
  const cadence = new FrameMetrics({ targetHz });
  jank = new BrowserJankMonitor();
  const measurementStart = performance.now();
  jank.start();
  for (let frame = 0; frame < measuredFrames && !sourceCompleted; frame += 1) {
    status.value = `Measuring ${frame + 1}/${measuredFrames} · ${sourcePath}…`;
    const timestamp = await nextAnimationFrame();
    const started = performance.now();
    const advanceResult = await advanceSample(
      (warmupFrames + frame + 1) / targetHz,
      stageTimingSamples !== null,
    );
    firstMeasuredTime ??= lastSampleTime;
    const advanceRoundTripMs = performance.now() - started;
    cadence.record(timestamp, advanceRoundTripMs);
    samples?.push({ sceneTime: lastSampleTime, advanceRoundTripMs });
    if (stageTimingSamples !== null) {
      stageTimingSamples.push({
        sceneTime: lastSampleTime,
        advanceRoundTripMs,
        ...advanceResult.sampleTiming,
      });
    }
    if (rendererSamples !== null && (rendererMetricsSampling === "dense" ||
        shouldSampleRendererStageFrame(frame, measuredFrames))) {
      const metricsStarted = performance.now();
      const renderer = (await execution.metrics({
        profilePublicationStages: rendererSamples !== null,
        profileRenderSubstages: rendererSamples !== null,
      })).metrics;
      const renderSubstageWindow = renderer.renderSubstageSamples;
      const latestRenderSubstage = renderSubstageWindow?.[renderSubstageWindow.length - 1];
      const hasSubstageIdentity = Number.isSafeInteger(latestRenderSubstage?.session) &&
        Number.isSafeInteger(latestRenderSubstage?.sequence);
      const isNewSubstage = hasSubstageIdentity && (rendererSubstageCursor === null ||
        latestRenderSubstage.session > rendererSubstageCursor.session ||
        (latestRenderSubstage.session === rendererSubstageCursor.session &&
         latestRenderSubstage.sequence > rendererSubstageCursor.sequence));
      if (isNewSubstage && rendererSubstageSamples.length < MAX_RENDERER_SUBSTAGE_SAMPLES) {
        rendererSubstageCursor = {
          session: latestRenderSubstage.session,
          sequence: latestRenderSubstage.sequence,
        };
        rendererSubstageSamples.push({
          ...latestRenderSubstage,
          measuredFrameIndex: frame,
        });
      }
      rendererSamples.push({
        sceneTime: lastSampleTime,
        metricsQueryMs: performance.now() - metricsStarted,
        lastDeltaApplyMs: renderer.lastDeltaApplyMs,
        lastRendererCallMs: renderer.lastRendererCallMs,
        drawCalls: renderer.drawCalls,
        instances: renderer.instancesDrawn,
        uploadBytes: renderer.bytesUploaded,
      });
      if (rendererPublicationStageSamples !== null) {
        let latestNewPublicationStageSample = null;
        for (const sample of renderer.publicationStageSamples ?? []) {
          const isNew = rendererPublicationStageCursor === null ||
            sample.session > rendererPublicationStageCursor.session ||
            (sample.session === rendererPublicationStageCursor.session &&
             sample.sequence > rendererPublicationStageCursor.sequence);
          if (!isNew) continue;
          rendererPublicationStageCursor = { session: sample.session, sequence: sample.sequence };
          latestNewPublicationStageSample = sample;
        }
        if (rendererPublicationStageSamples.length < MAX_RENDERER_PUBLICATION_STAGE_SAMPLES &&
            shouldSampleRendererStageFrame(frame, measuredFrames) &&
            latestNewPublicationStageSample !== null) {
          rendererPublicationStageSamples.push({
            ...latestNewPublicationStageSample,
            measuredFrameIndex: frame,
          });
        }
      }
    }
  }
  const measurementEnd = performance.now();
  jank.stop();
  const metrics = (await execution.metrics()).metrics;
  if (sourceError) throw sourceError;
  const frame = cadence.summary();
  const report = {
    schemaVersion: 2,
    ...(samples === null ? {} : { samples }),
    ...(rendererSamples === null ? {} : { rendererSamples }),
    ...(rendererSubstageSamples === null ? {} : {
      rendererSubstageSamples,
      rendererSubstageNotes: {
        timing: "CPU wall time inside one successful render call, split at the existing renderer-host boundaries",
        scope: "small observation and bookkeeping work between/after measured stages is omitted, so stage totals need not equal lastRendererCallMs",
        surfaceAcquireCpuWallMs: "surface texture acquisition call; does not measure later physical presentation",
        prepareCpuWallMs: "retained frame preparation and inset setup",
        uploadCpuWallMs: "synchronous renderer upload calls; not GPU transfer completion",
        encodeCpuWallMs: "command encoder creation, retained draw encoding, and command-buffer finish",
        submitPresentCpuWallMs: "host queue submit and present call duration; not GPU completion or scanout",
        collection: "latest exact session/sequence sample at each renderer metrics poll, capped at 32; enabling diagnostics adds timing calls to every successful render",
      },
    }),
    ...(rendererSamples === null ? {} : { rendererMetricsSampling }),
    ...(rendererPublicationStageSamples === null ? {} : {
      rendererPublicationStageSamples,
      rendererPublicationStageNotes: {
        applyMs: "render-worker WASM delta application for this exact session/sequence",
        renderMs: "synchronous retained renderer render call; not GPU completion",
        receiveToPresentMs: "render-worker consume entry through successful render return",
        ackPostMs: "synchronous execution_presented MessagePort post duration",
        capture: "latest unique publication at evenly spaced measured-frame slots, capped at 32; sparse mode polls renderer metrics only at those slots",
      },
    }),
    ...(stageTimingSamples === null ? {} : { stageTimingSamples }),
    ...(stageTimingSamples === null ? {} : {
      stageTimingNotes: {
        endpointMs: "sample endpoint handling through continuation completion, before response transport",
        rustDriveMs: "synchronous authored-time drive call",
        deltaDrainMs: "delta drain and JSON generation",
        deltaMetadataMs: "producer-side retained delta metadata validation",
        deltaSendMs: "transport encoding and publication send after metadata validation",
        callbackPhaseMs: "required callback-phase service and commit",
        presentationWaitMs: "render-worker presented acknowledgement wait; not GPU completion",
        segmentHandoffMs: "segment completion and player lease return",
        authoringBoundaryWaitMs: "wait for Python source continuation and next segment attachment",
      },
    }),
    benchmark: "Noon shared authored scene profile",
    generatedAt: new Date().toISOString(),
    scene: { source: sourcePath, context, objects: metrics.objectCount, camera: "authored" },
    ...(runtimeBuildIdentity === null ? {} : { runtimeBuild: runtimeBuildIdentity }),
    environment: {
      userAgent: navigator.userAgent,
      rendererBackend: execution.rendererBackend,
      devicePixelRatio: window.devicePixelRatio || 1,
      viewportCssPixels: [canvas.clientWidth, canvas.clientHeight],
      targetHz,
    },
    setup: { workerStartupMs, initialExecutionReadyMs, warmupFrames },
    execution: {
      mode: execution.mode,
      transportMode: ready.transportMode,
      sourceContinuation: continuation,
      sourceCompleted: completedSource !== null,
      authoredDuration: completedSource?.duration ?? null,
      firstMeasuredTime,
      lastMeasuredTime: lastSampleTime,
      requestedMeasuredFrames: measuredFrames,
      requestedSampleStepSeconds: 1 / targetHz,
    },
    cadence: {
      frames: frame.frames,
      frameIntervalMs: frame.interval,
      effective: frame.cadence,
    },
    pipeline: {
      // This includes worker round trips, callbacks, publication and rendering.
      // It is deliberately not labeled isolated CPU or GPU execution time.
      advanceRoundTripMs: frame.submission,
    },
    renderer: {
      presentedFrames: metrics.presentedFrames - before.presentedFrames,
      // Latest accepted WASM delta application; excludes host/engine worker time.
      lastDeltaApplyMs: metrics.lastDeltaApplyMs,
      // One successful render call near the end of the run, not a frame-time
      // distribution or physical presentation latency.
      lastRendererCallMs: metrics.lastRendererCallMs,
      drawCalls: metrics.drawCalls,
      instances: metrics.instancesDrawn,
      lastUploadBytes: metrics.bytesUploaded,
      geometryCacheMisses: metrics.geometryCacheMisses,
    },
    browser: { longTasks: jank.summary(measurementStart, measurementEnd) },
    unavailableMetrics: ["cpuFrameP95Ms", "gpuP95Ms"],
  };
  window.__NOON_SCENE_PERF__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value = `Complete · ${sourcePath} · ${format(report.cadence.effective?.effectiveFps)} FPS · ` +
    `p95 ${format(report.pipeline.advanceRoundTripMs?.p95)} ms advance round trip`;
  status.dataset.state = "complete";
  console.log("NOON_SCENE_PERF", report);
} catch (error) {
  console.error(error);
  status.value = `Scene profile failed: ${error}`;
  status.dataset.state = "error";
} finally {
  jank?.stop();
  execution?.terminate();
  client?.terminate();
}

async function advanceSample(time, collectTimings = false) {
  if (sourceError) throw sourceError;
  // Predeclared timelines have a finite replay interval. Static scenes (zero
  // duration) remain useful steady-state measurements and keep sampling.
  const replayEnd = !continuation && completedSource?.duration > 0
    ? completedSource.duration : null;
  const sampleTime = replayEnd === null ? time : Math.min(time, replayEnd);
  const request = continuation
    ? execution.sampleToAuthoredTime(sampleTime, {
      stopAtSourceCompletion: true,
      collectTimings,
    })
    : execution.advanceTo(sampleTime);
  const result = await Promise.race([request, sourceFailure]);
  if (sourceError) throw sourceError;
  sourceCompleted = result.sourceCompleted === true ||
    (replayEnd !== null && sampleTime >= replayEnd);
  lastSampleTime = result.time;
  return result;
}

function parseContext(value) {
  if (value === null || value === "") return {};
  const parsed = JSON.parse(value);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    throw new Error("context must decode to an object");
  }
  return parsed;
}

async function loadText(path) {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`Unable to load ${path}: HTTP ${response.status}`);
  return response.text();
}

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function positiveInteger(name, fallback, minimum = 1) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < minimum) throw new Error(`${name} must be an integer >= ${minimum}`);
  return parsed;
}

function positiveNumber(name, fallback) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) throw new Error(`${name} must be positive`);
  return parsed;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(2) : "—";
}
