import init, {
  RetainedExecutionCanvasRenderer,
} from "./pkg/noon_web.js";
import {
  drainRendererGpuDiagnostics,
  formatGpuDiagnostic,
} from "./render-gpu-diagnostics.js";
import { SampleWindow } from "./frame-metrics.js";
import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaReader,
  TransferableExecutionDeltaReceiver,
} from "./execution-transport.js";

const RENDER_CHANNEL = "noon.render";
const RENDER_PROTOCOL_VERSION = 1;
const MODE_RETAINED = "retained";
const BOOTSTRAP_QUEUE_LIMIT = 1;

let wasmInitializationPromise = null;

function initializeWasmModule() {
  if (wasmInitializationPromise === null) {
    const initialization = Promise.resolve().then(() => init());
    const sharedInitialization = initialization.catch((error) => {
      if (wasmInitializationPromise === sharedInitialization) {
        wasmInitializationPromise = null;
      }
      throw error;
    });
    wasmInitializationPromise = sharedInitialization;
  }
  return wasmInitializationPromise;
}

export function createAuthoringRenderController(host) {
  if (!host || typeof host !== "object") {
    throw new TypeError("authoring render host must be an object");
  }
  if (typeof host.postMessage !== "function") {
    throw new TypeError("authoring render host requires postMessage");
  }

  let renderPort = null;
  let transportMode = null;
  let mode = null;
  let sharedReader = null;
  let transferableReceiver = null;
  let renderer = null;
  let resourceBytes = null;
  let reconnectResourceBundlePending = false;
  let canvas = null;
  let surfaceCreationError = null;
  let width = 1;
  let height = 1;
  let bootstrapQueue = [];
  let bootstrapPromise = null;
  let transitionRequestId = null;
  let transitionResponseType = null;
  let transitionMode = null;
  let transitionResourceBytes = null;
  let transitionFrameLoopWasRunning = false;
  let needsPresent = false;
  // Renderer-derived metadata for the publication that the next successful
  // present exposes. It is an acknowledgement only, never scene or time state.
  let pendingPresentationPublication = null;
  let lastPresentedPublication = null;
  let lastPointerReceipt = null;
  let pendingRendererObservationRequest = null;
  let pendingRendererObservationPublication = null;
  let running = false;
  let stopped = false;
  let frameLoopGeneration = 0;
  let lastFrameTimestamp = null;
  // Optional cross-worker projection of Rust's wake decision. This owns browser
  // timer handles only; scene time and segment completion remain in the engine.
  let engineWake = null;
  let scheduledFrame = null;
  let scheduleTicket = 0;
  let presentedFrames = 0;
  let firstPresentedAtMs = null;
  let lastDeltaApplyMs = null;
  let lastRendererCallMs = null;
  let presentedSession = null;
  let firstPresentedSessionAtMs = null;
  // Observe successful submissions only. Rust's continuous wake scopes the
  // window, so authored holds and paused/replaced sessions are not hitches.
  const presentationIntervals = new SampleWindow(120);
  let lastContinuousPresentationAtMs = null;
  let continuousPresentation = false;
  let rendererReadyAtMs = null;
  let modeSwitches = 0;
  let rendererRebuilds = 0;
  // Profiling is enabled only by an explicit metrics request. Keep a small
  // rolling window so diagnostics cannot grow with a long-running session.
  let publicationStageProfiling = false;
  let renderSubstageProfiling = false;
  const publicationStageSamples = [];
  let pendingPublicationStageSample = null;
  let webglRecoveryPromise = null;
  let webglContextLost = false;

    return Object.freeze({
      dispatch: dispatchAuthoringRenderMessage,
      shutdown: shutdownAuthoringRenderController,
    });

  async function dispatchAuthoringRenderMessage(message) {
    if (stopped) return;
    return handleMainMessage(message);
  }

  function shutdownAuthoringRenderController() {
    if (!stopped) stop();
  }

  async function handleMainMessage(message) {
    try {
      validateMainMessage(message);
      if (webglRecoveryPromise !== null) await webglRecoveryPromise;
      if (!(await flushGpuDiagnostics())) return;
      switch (message.type) {
        case "init":
          await initialize(message);
          return;
        case "prepare":
          await prepare(message);
          return;
        case "start_engine":
          startEngine(message);
          return;
        case "attach_engine":
          attachEngine(message);
          return;
        case "switch_engine":
          switchEngine(message);
          return;
        case "rebuild_engine":
          rebuildEngine(message);
          return;
        case "resize":
          resize(message);
          return;
        case "metrics":
          if (!drainGpuDiagnostics()) return;
          if (message.profilePublicationStages === true) publicationStageProfiling = true;
          if (message.profileRenderSubstages === true) {
            renderSubstageProfiling = true;
            renderer?.setRenderSubstageProfiling(true);
          }
          {
            const metrics = currentMetrics();
            if (message.includeGpuIdentity === true) {
              if (renderer === null || typeof renderer.rendererAdapterInfo !== "function") {
                throw new Error("renderer GPU identity diagnostics are unavailable");
              }
              metrics.rendererGpuIdentity = JSON.parse(renderer.rendererAdapterInfo());
            }
            respond(message.requestId, { type: "metrics", metrics });
          }
          return;
        case "stop":
          stop();
          return;
        default:
          throw new Error(`unknown authoring render command ${message.type}`);
      }
    } catch (error) {
      fail(error, message?.requestId ?? null);
    }
  }

  async function initialize(message) {
    validateEnginePort(message.port, "authoring render init");
    const requestedMode = validateMode(message.mode);
    if (!(await prepareSurface(message, "authoring render init"))) return;
    mode = requestedMode;
    attachRenderPort(message.port);
  }

  async function prepare(message) {
    if (!(await prepareSurface(message, "authoring render prepare"))) return;
    respond(message.requestId, {
      type: "prepared",
      transportMode,
      width,
      height,
    });
  }

  async function prepareSurface(message, operation) {
    if (renderPort !== null || canvas !== null) {
      throw new Error("authoring render controller is already initialized");
    }
    if (!(message.canvas instanceof OffscreenCanvas)) {
      throw new Error(`${operation} requires an OffscreenCanvas`);
    }
    validateTransportMode(message.transportMode);

    canvas = message.canvas;
    width = normalizedDimension(message.width ?? canvas.width);
    height = normalizedDimension(message.height ?? canvas.height);
    transportMode = message.transportMode;
    await initializeWasmModule();
    if (stopped) return false;
    canvas.addEventListener("webglcontextrestored", wakeAfterWebGlContextRestored);
    canvas.addEventListener("webglcontextlost", suspendForWebGlContextLoss);
    canvas.addEventListener("webglcontextcreationerror", recordSurfaceCreationError);
    return true;
  }

  function recordSurfaceCreationError(event) {
    surfaceCreationError = event.statusMessage || "WebGL context creation failed";
  }

  function suspendForWebGlContextLoss(event) {
    event.preventDefault();
    webglContextLost = true;
    resetPresentationIntervals();
    invalidatePointerReceipt();
    cancelScheduledFrame();
  }

  function wakeAfterWebGlContextRestored() {
    invalidatePointerReceipt();
    // Browsers can checkpoint microtasks between event listeners. Use a new
    // task so Rust's later-registered restoration listener has recorded the
    // loss/restoration before recovery checks it, including while idle.
    setTimeout(() => void recoverAndPresentWebGlContext(), 0);
  }

  async function recoverAndPresentWebGlContext() {
    const restoringRenderer = renderer;
    if (stopped || webglRecoveryPromise !== null) return;
    if (restoringRenderer === null) {
      webglContextLost = false;
      drainTransport();
      return;
    }
    const recovery = Promise.resolve().then(() => restoringRenderer.recoverWebGlContext());
    webglRecoveryPromise = recovery;
    try {
      const recovered = await recovery;
      if (stopped || renderer !== restoringRenderer || !recovered) return;
      webglRecoveryPromise = null;
      webglContextLost = false;
      renderer.resize(width, height);
      if (!drainGpuDiagnostics()) return;
      needsPresent = true;
      if (tryPresent()) {
        flushBootstrapQueue();
        drainTransport();
      }
      scheduleFrame();
    } catch (error) {
      if (!stopped && renderer === restoringRenderer) fail(error, null);
    } finally {
      if (webglRecoveryPromise === recovery) webglRecoveryPromise = null;
    }
  }

  function startEngine(message) {
    if (canvas === null || transportMode === null) {
      throw new Error("authoring render controller cannot start an engine before prepare");
    }
    if (
      renderPort !== null ||
      renderer !== null ||
      mode !== null ||
      bootstrapPromise !== null ||
      transitionRequestId !== null ||
      transitionMode !== null
    ) {
      throw new Error("authoring render controller can start its initial engine only once");
    }
    validateEnginePort(message.port, "authoring render initial engine");
    validateMatchingTransport(message.transportMode, "authoring render initial engine");

    mode = validateMode(message.mode);
    transitionRequestId = validateRequestId(message.requestId);
    transitionResponseType = "engine_started";
    attachRenderPort(message.port);
  }

  function attachEngine(message) {
    requireBootstrappedRenderer("reconnect");
    validateEnginePort(message.port, "authoring render reconnect");
    validateMatchingTransport(message.transportMode, "authoring render reconnect");
    const requestedMode = validateMode(message.mode ?? mode);
    if (requestedMode !== mode) {
      throw new Error(
        `authoring render reconnect mode ${requestedMode} does not match active mode ${mode}`,
      );
    }

    detachRenderPort();
    resetTransportState();
    if (mode === MODE_RETAINED) {
      reconnectResourceBundlePending = true;
    }
    attachRenderPort(message.port);
    scheduleFrame();
    respond(message.requestId, {
      type: "engine_port_attached",
      mode,
      ...modeFlags(),
      transportMode,
      backend: renderer.rendererBackend(),
      gpuGeneration: renderer.gpuGeneration(),
    });
  }

  function switchEngine(message) {
    requireBootstrappedRenderer("switch mode");
    const nextMode = validateMode(message.mode);
    if (nextMode === mode) {
      throw new Error(
        `authoring render mode switch requires a different mode; ${mode} is already active`,
      );
    }
    beginRendererTransition(message, nextMode, "mode_switched");
    modeSwitches += 1;
  }

  function rebuildEngine(message) {
    requireBootstrappedRenderer("rebuild renderer");
    const requestedMode = validateMode(message.mode ?? mode);
    if (requestedMode !== mode) {
      throw new Error(
        `authoring render rebuild mode ${requestedMode} does not match active mode ${mode}`,
      );
    }
    beginRendererTransition(message, mode, "renderer_rebuilt");
    rendererRebuilds += 1;
  }

  function beginRendererTransition(message, nextMode, responseType) {
    validateEnginePort(message.port, "authoring render engine transition");
    validateMatchingTransport(message.transportMode, "authoring render engine transition");
    if (
      transitionRequestId !== null ||
      transitionMode !== null ||
      bootstrapPromise !== null
    ) {
      throw new Error("authoring render engine transition is already in progress");
    }

    detachRenderPort();
    resetTransportState();
    transitionFrameLoopWasRunning = running;
    frameLoopGeneration += 1;
    running = false;
    reconnectResourceBundlePending = false;
    transitionMode = nextMode;
    transitionResourceBytes = null;
    transitionRequestId = validateRequestId(message.requestId);
    transitionResponseType = responseType;
    attachRenderPort(message.port);
  }

  function resize(message) {
    const nextWidth = normalizedDimension(message.width);
    const nextHeight = normalizedDimension(message.height);
    const dimensionsChanged = nextWidth !== width || nextHeight !== height;
    if (dimensionsChanged) invalidatePointerReceipt();
    width = nextWidth;
    height = nextHeight;
    if (renderer === null) {
      return;
    }
    if (webglContextLost || webglRecoveryPromise !== null) {
      needsPresent = true;
      return;
    }
    renderer.resize(width, height);
    if (!drainGpuDiagnostics()) return;
    // Surface changes need a presentation even when a semantic continuation has
    // completed and its engine is idle. Redundant resizes must remain a no-op:
    // render() correctly returns false while an unnecessary needsPresent would
    // wedge transport backpressure.
    if (dimensionsChanged) {
      needsPresent = true;
      tryPresent();
      if (engineWake !== null) scheduleFrame();
    }
  }

  function stop() {
    stopped = true;
    frameLoopGeneration += 1;
    running = false;
    bootstrapQueue = [];
    transitionMode = null;
    transitionResourceBytes = null;
    transitionFrameLoopWasRunning = false;
    detachRenderPort();
    canvas?.removeEventListener?.("webglcontextrestored", wakeAfterWebGlContextRestored);
    canvas?.removeEventListener?.("webglcontextlost", suspendForWebGlContextLoss);
    canvas?.removeEventListener?.("webglcontextcreationerror", recordSurfaceCreationError);
    disposeRenderer();
    canvas = null;
    host?.close?.();
  }

  function attachRenderPort(port) {
    renderPort = port;
    port.addEventListener("message", (event) => {
      if (renderPort !== port) {
        return;
      }
      handleEngineMessage(event.data);
    });
    if (transportMode === EXECUTION_TRANSPORT_TRANSFERABLE) {
      transferableReceiver = new TransferableExecutionDeltaReceiver(
        port,
        (json, metadata) => (renderPort === port ? consumeDelta(json, metadata) : true),
      );
    }
    port.start();
  }

  function detachRenderPort() {
    cancelScheduledFrame();
    engineWake = null;
    continuousPresentation = false;
    resetPresentationIntervals();
    renderPort?.close?.();
    renderPort = null;
  }

  function resetTransportState() {
    sharedReader = null;
    transferableReceiver = null;
    bootstrapQueue = [];
    bootstrapPromise = null;
    pendingPresentationPublication = null;
    pendingPublicationStageSample = null;
    lastPresentedPublication = null;
    lastPointerReceipt = null;
    pendingRendererObservationRequest = null;
    pendingRendererObservationPublication = null;
  }

  function handleEngineMessage(message) {
    if (!message || typeof message !== "object") {
      return;
    }
    if (message.type === "execution_wake") {
      try {
        const { cadence, timerAfterMilliseconds } = message;
        if (!["animation_frame", "timer", "idle"].includes(cadence) ||
            (cadence === "timer" &&
             (!Number.isFinite(timerAfterMilliseconds) || timerAfterMilliseconds < 0))) {
          throw new Error("invalid semantic execution wake directive");
        }
        // This directive replaces the in-flight drive's reserved refresh.
        // Animation reuses the RAF; idle/timer retires it through scheduleFrame.
        if (scheduledFrame !== null) scheduledFrame.reservedRefresh = false;
        engineWake = {
          cadence,
          deadline: cadence === "timer" ? performance.now() + timerAfterMilliseconds : null,
        };
        continuousPresentation = cadence === "animation_frame";
        if (!continuousPresentation) resetPresentationIntervals();
        scheduleFrame();
      } catch (error) {
        fail(error, null);
      }
      return;
    }
    if (message.type === "retained_resources") {
      handleRetainedResources(message);
      return;
    }
    if (message.type === "transport_setup") {
      if (transportMode !== EXECUTION_TRANSPORT_SHARED || message.mode !== transportMode) {
        fail(new Error("authoring render controller received an unexpected shared transport setup"), null);
        return;
      }
      try {
        sharedReader = new SharedExecutionDeltaReader(message.mailbox);
        drainTransport();
      } catch (error) {
        fail(error, null);
      }
      return;
    }
    if (message.type === "shared_delta") {
      drainTransport();
      return;
    }
    if (message.type === "renderer_observation_request") {
      try {
        receiveRendererObservationRequest(message);
      } catch (error) {
        fail(error, null);
      }
      return;
    }
    if (message.type === "renderer_observation_cancel") {
      try {
        cancelRendererObservationRequest(message);
      } catch (error) {
        fail(error, null);
      }
    }
  }

  function handleRetainedResources(message) {
    try {
      if (!(message.bytes instanceof Uint8Array) || message.bytes.byteLength === 0) {
        throw new Error("retained resource bundle must be a non-empty Uint8Array");
      }
      if (transitionMode !== null) {
        if (transitionMode !== MODE_RETAINED) {
          throw new Error("legacy authoring render transition cannot accept retained resources");
        }
        if (transitionResourceBytes !== null || bootstrapPromise !== null) {
          throw new Error(
            "retained transition resource bundle may be installed only once before the snapshot",
          );
        }
        transitionResourceBytes = message.bytes;
        return;
      }
      if (mode !== MODE_RETAINED) {
        throw new Error("legacy authoring render mode cannot accept retained resources");
      }
      if (renderer !== null) {
        if (!reconnectResourceBundlePending) {
          throw new Error("retained resource bundle may be installed only once before the snapshot");
        }
        reconnectResourceBundlePending = false;
        return;
      }
      if (resourceBytes !== null || bootstrapPromise !== null) {
        throw new Error("retained resource bundle may be installed only once before the snapshot");
      }
      resourceBytes = message.bytes;
    } catch (error) {
      fail(error, null);
    }
  }

  function drainTransport() {
    try {
      if (sharedReader !== null) {
        const drained = sharedReader.drain((json, metadata) => consumeDelta(json, metadata));
        if (drained > 0) {
          renderPort?.postMessage({ type: "transport_writable" });
        }
      }
      transferableReceiver?.drain();
    } catch (error) {
      fail(error, null);
    }
  }

  function consumeDelta(json, publication = null) {
    // Backpressure applies to bootstrap and replay handoff too: constructing a
    // new GPU device against a lost canvas can trap inside the WebGL backend.
    if (webglContextLost || webglRecoveryPromise !== null) return false;
    if (transitionMode !== null) {
      return commitRendererTransition(json, publication);
    }
    if (renderer === null) {
      if (mode === MODE_RETAINED && resourceBytes === null) {
        throw new Error("retained authoring snapshot arrived before its resource bundle");
      }
      if (bootstrapPromise === null) {
        bootstrapPromise = bootstrapRenderer(json, true, publication);
        return true;
      }
      if (bootstrapQueue.length >= BOOTSTRAP_QUEUE_LIMIT) {
        return false;
      }
      bootstrapQueue.push({ json, publication });
      return true;
    }

    if (mode === MODE_RETAINED && reconnectResourceBundlePending) {
      throw new Error("retained authoring reconnect snapshot arrived before its resource bundle");
    }
    if (needsPresent) {
      return false;
    }
    const stageSample = publicationStageProfiling && publication !== null
      ? { session: publication.session, sequence: publication.sequence, receivedAtMs: performance.now() }
      : null;
    const applied = applyRendererDelta(json);
    if (!applied) {
      acknowledgeAlreadyPresented(publication);
      return true;
    }
    armRendererObservation(publication);
    pendingPresentationPublication = publication;
    pendingPublicationStageSample = stageSample;
    needsPresent = true;
    tryPresent();
    if (engineWake !== null) scheduleFrame();
    return true;
  }

  function commitRendererTransition(initial, publication = null) {
    const nextMode = transitionMode;
    if (nextMode === null) {
      throw new Error("authoring render transition has no pending mode");
    }
    if (nextMode === MODE_RETAINED && transitionResourceBytes === null) {
      throw new Error("retained authoring transition snapshot arrived before its resource bundle");
    }

    const nextResourceBytes = transitionResourceBytes;
    const resumeFrameLoop = transitionFrameLoopWasRunning;
    transitionMode = null;
    transitionResourceBytes = null;
    transitionFrameLoopWasRunning = false;

    // The replacement engine has already queued a complete bootstrap payload.
    // Retire the currently presenting renderer only once that payload reaches
    // this host; renderer/device construction is the only remaining blank-window seam.
    disposeRenderer();
    resourceBytes = nextResourceBytes;
    reconnectResourceBundlePending = false;
    needsPresent = false;
    pendingPresentationPublication = null;
    pendingPublicationStageSample = null;
    lastPresentedPublication = null;
    lastPointerReceipt = null;
    pendingRendererObservationPublication = null;
    mode = nextMode;
    bootstrapPromise = bootstrapRenderer(initial, resumeFrameLoop, publication);
    return true;
  }

  async function bootstrapRenderer(initial, resumeFrameLoop = true, publication = null) {
    const bootstrapGeneration = frameLoopGeneration;
    surfaceCreationError = null;
    try {
      const createdRenderer = await RetainedExecutionCanvasRenderer.create(canvas, resourceBytes);
      if (renderSubstageProfiling) createdRenderer.setRenderSubstageProfiling(true);
      // create() resolves after GPU setup; keep this separate from first render.
      rendererReadyAtMs ??= performance.now();
      if (stopped) {
        createdRenderer.free?.();
        return;
      }
      renderer = createdRenderer;
      lastDeltaApplyMs = null;
      lastRendererCallMs = null;
      resourceBytes = null;
      const applied = applyRendererDelta(initial);
      if (!applied) {
        throw new Error("retained authoring renderer must begin from an applied snapshot");
      }
      armRendererObservation(publication);
      renderer.resize(width, height);
      if (!drainGpuDiagnostics()) return;
      pendingPresentationPublication = publication;
      pendingPublicationStageSample = null;
      needsPresent = true;
      while (!tryPresent()) {
        if (
          stopped ||
          bootstrapGeneration !== frameLoopGeneration ||
          !drainGpuDiagnostics()
        ) {
          return;
        }
        await nextRenderOpportunity();
        if (stopped || bootstrapGeneration !== frameLoopGeneration) {
          return;
        }
      }
      if (stopped || bootstrapGeneration !== frameLoopGeneration || !drainGpuDiagnostics()) return;

      const ready = {
        mode,
        ...modeFlags(),
        transportMode,
        backend: renderer.rendererBackend(),
        gpuGeneration: renderer.gpuGeneration(),
        time: renderer.time(),
        presentedFrames,
      };
      if (mode === MODE_RETAINED) {
        ready.preloadedGeometryCount = renderer.preloadedGeometryCount();
        ready.preloadBytesUploaded = renderer.preloadBytesUploaded();
      }
      if (transitionRequestId === null) {
        postMain({ type: "ready", ...ready });
      } else {
        const requestId = transitionRequestId;
        const responseType = transitionResponseType;
        transitionRequestId = null;
        transitionResponseType = null;
        respond(requestId, { type: responseType, ...ready });
      }

      flushBootstrapQueue();
      drainTransport();
      if (resumeFrameLoop) {
        running = true;
        scheduleFrame(bootstrapGeneration);
      } else {
        running = false;
      }
    } catch (error) {
      const requestId = transitionRequestId;
      transitionRequestId = null;
      transitionResponseType = null;
      fail(surfaceCreationError === null ? error : new Error(
        `${error instanceof Error ? error.message : String(error)}; ${surfaceCreationError}`,
      ), requestId);
    } finally {
      bootstrapPromise = null;
    }
  }

  function tryPresent() {
    if (
      renderer === null ||
      webglContextLost ||
      webglRecoveryPromise !== null ||
      !needsPresent ||
      !drainGpuDiagnostics()
    ) {
      return false;
    }
    const renderStartedAtMs = performance.now();
    if (!renderer.render()) {
      invalidatePointerReceipt();
      drainGpuDiagnostics();
      return false;
    }
    const presentedAtMs = performance.now();
    lastRendererCallMs = Math.max(0, presentedAtMs - renderStartedAtMs);
    needsPresent = false;
    presentedFrames += 1;
    firstPresentedAtMs ??= presentedAtMs;
    const publication = pendingPresentationPublication;
    if (publication !== null && presentedSession !== publication.session) {
      resetPresentationIntervals();
    }
    if (continuousPresentation) {
      if (lastContinuousPresentationAtMs !== null) {
        presentationIntervals.record(presentedAtMs - lastContinuousPresentationAtMs);
      }
      lastContinuousPresentationAtMs = presentedAtMs;
    }
    const candidateStageSample = pendingPublicationStageSample;
    const stageSample = samePublication(candidateStageSample, publication)
      ? candidateStageSample
      : null;
    const observationPublication = pendingRendererObservationPublication;
    pendingPresentationPublication = null;
    pendingPublicationStageSample = null;
    if (stageSample !== null) {
      stageSample.presentedAtMs = presentedAtMs;
      stageSample.applyMs = lastDeltaApplyMs;
      stageSample.receiveToPresentMs = Math.max(0, presentedAtMs - stageSample.receivedAtMs);
      stageSample.renderMs = lastRendererCallMs;
    }
    pendingRendererObservationPublication = null;
    if (publication !== null) {
      lastPresentedPublication = publication;
      if (presentedSession !== publication.session) {
        presentedSession = publication.session;
        firstPresentedSessionAtMs = performance.now();
      }
    }
    try {
      acknowledgeRendererObservation(observationPublication, publication);
    } catch (error) {
      fail(error, null);
      return false;
    }
    const displayed = publication ?? lastPresentedPublication;
    if (displayed?.pointerView) {
      if (!Number.isSafeInteger(presentedFrames)) throw new Error("presentation counter exhausted");
      lastPointerReceipt = Object.freeze({
        session: displayed.session, sequence: displayed.sequence,
        presentation: presentedFrames, view_revision: displayed.pointerView.revision,
      });
    } else lastPointerReceipt = null;
    const ackStartedAtMs = stageSample === null ? 0 : performance.now();
    acknowledgePresented(displayed);
    if (stageSample !== null) {
      stageSample.ackPostMs = Math.max(0, performance.now() - ackStartedAtMs);
      stageSample.renderToAckMs = Math.max(0, performance.now() - presentedAtMs);
      publicationStageSamples.push(stageSample);
      if (publicationStageSamples.length > 32) publicationStageSamples.shift();
    }
    return drainGpuDiagnostics();
  }

  function applyRendererDelta(json) {
    const startedAtMs = performance.now();
    const applied = renderer.applyDeltaJson(json);
    if (applied) lastDeltaApplyMs = Math.max(0, performance.now() - startedAtMs);
    return applied;
  }

  function samePublication(left, right) {
    return left !== null && right !== null &&
      left.session === right.session && left.sequence === right.sequence;
  }

  function acknowledgePresented(publication) {
    if (publication === null || renderPort === null) {
      return;
    }
    renderPort.postMessage({
      type: "execution_presented",
      session: publication.session,
      sequence: publication.sequence,
      ...(lastPointerReceipt === null ? {} : { pointerReceipt: lastPointerReceipt }),
    });
  }

  function invalidatePointerReceipt() {
    if (lastPointerReceipt === null) return;
    renderPort?.postMessage({ type: "pointer_presentation_invalidated", receipt: lastPointerReceipt });
    lastPointerReceipt = null;
  }

  function acknowledgeRendererObservation(observationPublication, presentedPublication) {
    if (observationPublication === null) {
      return;
    }
    if (!samePublication(observationPublication, presentedPublication) || renderPort === null) {
      throw new Error("renderer observation presentation does not match its publication");
    }
    const json = renderer.takeRendererObservationJson();
    if (typeof json !== "string") {
      throw new Error("retained renderer did not publish its requested observation");
    }
    renderPort.postMessage({
      type: "renderer_observation",
      session: observationPublication.session,
      sequence: observationPublication.sequence,
      json,
    });
  }

  function acknowledgeAlreadyPresented(publication) {
    // `applyDeltaJson` returns false only for a typed stale transport envelope.
    // It cannot prove a new publication reached the surface. A duplicate of the
    // exact already-presented envelope is safe to acknowledge without redrawing;
    // any older or foreign envelope remains unacknowledged.
    if (!needsPresent && samePublication(publication, lastPresentedPublication)) {
      acknowledgePresented(publication);
    }
  }

  function flushBootstrapQueue() {
    if (renderer === null || webglContextLost || webglRecoveryPromise !== null) {
      return;
    }
    while (!needsPresent && bootstrapQueue.length > 0) {
      const { json, publication } = bootstrapQueue.shift();
      const applied = applyRendererDelta(json);
      if (!applied) {
        acknowledgeAlreadyPresented(publication);
        continue;
      }
      armRendererObservation(publication);
      pendingPresentationPublication = publication;
      pendingPublicationStageSample = null;
      needsPresent = true;
      if (!tryPresent()) {
        break;
      }
    }
  }

  function receiveRendererObservationRequest(message) {
    const publication = rendererObservationMessagePublication(message);
    if (mode !== MODE_RETAINED) {
      throw new Error("renderer observations require retained execution");
    }
    if (pendingRendererObservationRequest !== null ||
        pendingRendererObservationPublication !== null) {
      throw new Error("render host already has a pending renderer observation");
    }
    if (typeof message.json !== "string") {
      throw new Error("renderer observation request must be JSON");
    }
    pendingRendererObservationRequest = { publication, json: message.json };
  }

  function cancelRendererObservationRequest(message) {
    const publication = rendererObservationMessagePublication(message);
    if (pendingRendererObservationRequest !== null &&
        samePublication(pendingRendererObservationRequest.publication, publication)) {
      pendingRendererObservationRequest = null;
    }
  }

  function rendererObservationMessagePublication(message) {
    if (!Number.isSafeInteger(message?.session) || message.session < 0 ||
        !Number.isSafeInteger(message?.sequence) || message.sequence < 0) {
      throw new Error("renderer observation publication is invalid");
    }
    return { session: message.session, sequence: message.sequence };
  }

  function armRendererObservation(publication) {
    if (pendingRendererObservationRequest === null) {
      return;
    }
    const requested = pendingRendererObservationRequest.publication;
    if (!samePublication(requested, publication)) {
      if (publication !== null &&
          (publication.session !== requested.session || publication.sequence > requested.sequence)) {
        throw new Error("renderer observation publication was skipped or replaced");
      }
      return;
    }
    if (renderer === null || typeof renderer.setRendererObservationRequestJson !== "function") {
      throw new Error("retained renderer observation support is unavailable");
    }
    renderer.setRendererObservationRequestJson(pendingRendererObservationRequest.json);
    pendingRendererObservationPublication = publication;
    pendingRendererObservationRequest = null;
  }

  function nextRenderOpportunity() {
    return new Promise((resolve) => {
      if (typeof host?.requestAnimationFrame === "function") {
        host.requestAnimationFrame(resolve);
      } else {
        setTimeout(() => resolve(performance.now()), 16);
      }
    });
  }

  function cancelScheduledFrame() {
    scheduleTicket += 1;
    if (scheduledFrame !== null) {
      if (scheduledFrame.kind === "animation") {
        host?.cancelAnimationFrame?.(scheduledFrame.handle);
      } else {
        clearTimeout(scheduledFrame.handle);
      }
      scheduledFrame = null;
    }
  }

  function scheduleFrame(generation = frameLoopGeneration, reserveRefresh = false) {
    if (!running || webglContextLost || webglRecoveryPromise !== null) {
      cancelScheduledFrame();
      return;
    }
    const needsAnimationFrame = reserveRefresh || scheduledFrame?.reservedRefresh === true ||
      needsPresent || engineWake === null ||
      engineWake.cadence === "animation_frame";
    // Keep an already-requested refresh opportunity when a wake or delta still
    // needs it. Canceling after the browser snapshots callbacks can defer the
    // replacement to the following refresh. Only lifecycle/cadence changes retire it.
    if (needsAnimationFrame && scheduledFrame?.kind === "animation" &&
        scheduledFrame.generation === generation) return;
    cancelScheduledFrame();
    if (!needsAnimationFrame && engineWake.cadence === "idle") return;
    const ticket = scheduleTicket;
    if (needsAnimationFrame && typeof host?.requestAnimationFrame === "function") {
      scheduledFrame = {
        kind: "animation",
        generation,
        reservedRefresh: reserveRefresh,
        handle: host.requestAnimationFrame((timestamp) => frame(timestamp, generation, ticket)),
      };
    } else {
      const delay = needsAnimationFrame ? 16 : Math.max(0, engineWake.deadline - performance.now());
      scheduledFrame = {
        kind: "timer",
        handle: setTimeout(() => frame(performance.now(), generation, ticket), delay),
      };
    }
  }

  function frame(timestamp, generation, ticket) {
    if (ticket !== scheduleTicket || generation !== frameLoopGeneration || !running) return;
    scheduledFrame = null;
    if (webglContextLost || webglRecoveryPromise !== null || !drainGpuDiagnostics()) return;
    lastFrameTimestamp = timestamp;
    if (needsPresent && tryPresent()) {
      flushBootstrapQueue();
    }
    drainTransport();
    if (!needsPresent) {
      flushBootstrapQueue();
      drainTransport();
    }
    if (!running || !drainGpuDiagnostics()) return;
    // Reserve one browser refresh before the source response arrives. It may
    // present that response or service its next Rust directive; without one it
    // sends no tick and settles. This never polls a blocked required callback.
    const reserveRefresh = engineWake?.cadence === "animation_frame" &&
      typeof host?.requestAnimationFrame === "function";
    const tickDue = engineWake === null || engineWake.cadence === "animation_frame" ||
      (engineWake.cadence === "timer" && performance.now() >= engineWake.deadline);
    if (tickDue) {
      // One directive admits one engine drive. The response supplies the next
      // Rust-derived directive, so an idle/waiting continuation never RAF-polls.
      if (engineWake !== null) engineWake = { cadence: "idle", deadline: null };
      renderPort?.postMessage({ type: "tick", timestamp });
    }
    scheduleFrame(generation, tickDue && reserveRefresh);
  }

  async function flushGpuDiagnostics() {
    if (renderer === null) return true;
    try {
      await renderer.flushGpuDiagnostics?.();
      return drainGpuDiagnostics();
    } catch (error) {
      fail(error, null);
      return false;
    }
  }

  function drainGpuDiagnostics() {
    if (renderer === null) return true;
    try {
      return drainRendererGpuDiagnostics(renderer, {
        onRecoverable(diagnostic) {
          postMain({
            type: "recoverable_error",
            owner: "render",
            message: formatGpuDiagnostic(diagnostic),
            diagnostic,
          });
        },
        onFatal(diagnostic) {
          fail(new Error(formatGpuDiagnostic(diagnostic)), null);
        },
      });
    } catch (error) {
      fail(error, null);
      return false;
    }
  }

  function currentMetrics() {
    const base = {
      ready: renderer !== null,
      mode,
      ...modeFlags(),
      transportMode,
      presentedFrames,
      presentationIntervalMs: presentationIntervals.summary(),
      presentationIntervalSamples: presentationIntervals.size,
      lastDeltaApplyMs,
      lastRendererCallMs,
      modeSwitches,
      rendererRebuilds,
      sampledAtMs: performance.now(),
      performanceTimeOriginMs: performance.timeOrigin,
      rendererReadyAtMs,
      firstPresentedAtMs,
      presentedSession,
      firstPresentedSessionAtMs,
      transitionMode,
      lastFrameTimestamp,
      bufferedDeltas: bootstrapQueue.length + (transferableReceiver?.pendingCount() ?? 0),
      needsPresent,
      ...(publicationStageProfiling ? {
        publicationStageProfiling: true,
      publicationStageSamples: publicationStageSamples.slice(),
    } : {}),
    ...(renderSubstageProfiling ? {
      renderSubstageProfiling: true,
      renderSubstageSamples: renderer === null
        ? []
        : JSON.parse(renderer.takeRenderSubstageSamplesJson()),
    } : {}),
    };
    if (renderer === null) {
      return {
        ...base,
        resourceBundlePending: mode === MODE_RETAINED && resourceBytes !== null,
      };
    }
    const metrics = {
      ...base,
      backend: renderer.rendererBackend(),
      gpuGeneration: renderer.gpuGeneration(),
      time: renderer.time(),
      objectCount: renderer.objectCount(),
      drawCalls: renderer.lastDrawCalls(),
      instancesDrawn: renderer.lastInstancesDrawn(),
      bytesUploaded: renderer.lastBytesUploaded(),
      geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    };
    if (mode === MODE_RETAINED) {
      metrics.outlineCacheMisses = renderer.lastOutlineCacheMisses();
      metrics.resourceBundlePending = reconnectResourceBundlePending;
      metrics.preloadedGeometryCount = renderer.preloadedGeometryCount();
      metrics.preloadBytesUploaded = renderer.preloadBytesUploaded();
    }
    return metrics;
  }

  function resetPresentationIntervals() {
    lastContinuousPresentationAtMs = null;
    presentationIntervals.reset();
  }

  function disposeRenderer() {
    if (renderer === null) {
      return;
    }
    pendingPublicationStageSample = null;
    const retiredRenderer = renderer;
    renderer = null;
    lastDeltaApplyMs = null;
    lastRendererCallMs = null;
    if (webglRecoveryPromise !== null) {
      // An async WASM recovery holds a mutable borrow until it settles.
      void webglRecoveryPromise.then(
        () => retiredRenderer.free?.(),
        () => retiredRenderer.free?.(),
      );
    } else {
      retiredRenderer.free?.();
    }
  }

  function modeFlags() {
    return mode === MODE_RETAINED ? { retained: true, mixed: true } : {};
  }

  function requireBootstrappedRenderer(operation) {
    if (renderer === null || renderPort === null) {
      throw new Error(`authoring render controller cannot ${operation} before renderer bootstrap`);
    }
  }

  function validateEnginePort(port, operation) {
    if (!(port instanceof MessagePort)) {
      throw new Error(`${operation} requires an engine MessagePort`);
    }
  }

  function validateMatchingTransport(candidate, operation) {
    validateTransportMode(candidate);
    if (candidate !== transportMode) {
      throw new Error(`${operation} transport ${candidate} does not match ${transportMode}`);
    }
  }

  function validateTransportMode(candidate) {
    if (
      candidate !== EXECUTION_TRANSPORT_SHARED &&
      candidate !== EXECUTION_TRANSPORT_TRANSFERABLE
    ) {
      throw new Error(`unsupported authoring render transport mode ${candidate}`);
    }
  }

  function validateMode(candidate) {
    if (candidate !== MODE_RETAINED) {
      throw new Error(`unsupported authoring render mode ${candidate}`);
    }
    return candidate;
  }

  function respond(requestId, payload) {
    postMain({ requestId: validateRequestId(requestId), ...payload });
  }

  function validateRequestId(requestId) {
    if (!Number.isSafeInteger(requestId) || requestId < 0) {
      throw new Error("render request ID must be a non-negative safe integer");
    }
    return requestId;
  }

  function fail(error, requestId) {
    frameLoopGeneration += 1;
    running = false;
    const effectiveRequestId = requestId ?? transitionRequestId;
    transitionRequestId = null;
    transitionResponseType = null;
    transitionMode = null;
    transitionResourceBytes = null;
    transitionFrameLoopWasRunning = false;
    const message = String(error?.message ?? error);
    renderPort?.postMessage({ type: "render_error", message });
    postMain({ type: "error", requestId: effectiveRequestId, message });
  }

  function postMain(payload) {
    if (host === null) {
      throw new Error("authoring render host is not configured");
    }
    host.postMessage({
      channel: RENDER_CHANNEL,
      protocolVersion: RENDER_PROTOCOL_VERSION,
      ...payload,
    });
  }

  function validateMainMessage(message) {
    if (
      !message ||
      typeof message !== "object" ||
      message.channel !== RENDER_CHANNEL ||
      message.protocolVersion !== RENDER_PROTOCOL_VERSION
    ) {
      throw new Error("invalid authoring render control envelope");
    }
  }

  function normalizedDimension(value) {
    if (!Number.isFinite(value)) {
      throw new Error(`invalid render surface dimension ${value}`);
    }
    return Math.max(1, Math.round(value));
  }
}
