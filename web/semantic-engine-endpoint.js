// Runs an existing semantic session in its authoring worker. Rendering uses the
// same mailbox and delta receiver as other execution producers.
import {
  EXECUTION_TRANSPORT_SHARED, EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaWriter, TransferableExecutionDeltaSender,
  createSharedExecutionMailbox, executionDeltaMetadata,
} from "./execution-transport.js";

export const MAX_PENDING_SEMANTIC_CONTROLS = 128;
export const MAX_REQUIRED_CALLBACK_PHASES_PER_ADVANCE = 128;
const SOURCE_CONTINUATION_PLAYBACK_CONTROLS = new Set([
  "pause",
  "resume",
  "seek",
  "restart_playback",
  "set_loop_duration",
  "advance_to",
]);

export async function attachSemanticEngine(
  context,
  request,
  onStop = () => {},
  runRequiredCallbackPhase = null,
  continuation = null,
) {
  const { controlPort, renderPort, transportMode, loopDurationSeconds, session } = request;
  if (!(controlPort instanceof MessagePort) || !(renderPort instanceof MessagePort)) {
    throw new Error("semantic execution requires control and render ports");
  }
  if (typeof context?.createExecutionPlayer !== "function" ||
      typeof context.returnExecutionPlayer !== "function") {
    throw new Error("semantic execution requires a context player lease API");
  }
  let player = null;
  // A player is leased from the authoring context. The transport session only
  // frames deltas; it never selects or creates a second runtime for a scene.
  let transport;
  let stopped = false;
  let latestTick = null;
  let draining = false;
  let callbackGeneration = 0;
  let pendingPhaseJson = null;
  let callbackFault = null;
  let lastSentPublication = null;
  let lastPresentedPublication = null;
  let pendingPresentation = null;
  let continuationActive = false;
  let continuationGeneration = continuation?.generation ?? null;
  let continuationWakeCadence = null;
  const controls = [];
  const post = (payload) => controlPort.postMessage({ channel: "noon.engine", protocolVersion: 1, ...payload });
  const fail = (error, requestId = null) => post({ type: "error", requestId, message: String(error?.message ?? error) });
  const writable = () => typeof transport.canSend === "function" ? transport.canSend() : transport.inFlight() < 2;
  const send = (json) => {
    if (json == null) return null;
    const publication = executionDeltaMetadata(json);
    if (!transport.send(json)) throw new Error("semantic transport became backpressured after admission");
    lastSentPublication = publication;
    if (transportMode === EXECUTION_TRANSPORT_SHARED) renderPort.postMessage({ type: "shared_delta" });
    return publication;
  };
  const samePublication = (left, right) =>
    left !== null && right !== null &&
    left.session === right.session && left.sequence === right.sequence;
  const publicationAlreadyPresented = (publication) =>
    samePublication(lastPresentedPublication, publication);
  const rejectPendingPresentation = (error) => {
    if (pendingPresentation === null) return;
    const { reject } = pendingPresentation;
    pendingPresentation = null;
    reject(error);
  };
  const awaitPresentation = (publication) => {
    // An unchanged coherent frame emits no delta, but the most recently sent
    // publication may still be waiting for the surface. Reuse its transport
    // metadata as the barrier without retaining a scene or frame mirror.
    const requiredPublication = publication ?? lastSentPublication;
    if (requiredPublication === null || publicationAlreadyPresented(requiredPublication)) {
      return Promise.resolve();
    }
    if (pendingPresentation !== null) {
      throw new Error("semantic endpoint already awaits a renderer publication");
    }
    return new Promise((resolve, reject) => {
      pendingPresentation = { publication: requiredPublication, resolve, reject };
    });
  };
  const notePresentedPublication = (publication) => {
    if (!publication || !Number.isSafeInteger(publication.session) ||
        publication.session < 0 || !Number.isSafeInteger(publication.sequence) ||
        publication.sequence < 0 || publication.session !== session ||
        lastSentPublication === null ||
        publication.session !== lastSentPublication.session ||
        publication.sequence > lastSentPublication.sequence) {
      fail(new Error("renderer reported an invalid execution publication"));
      return;
    }
    if (lastPresentedPublication !== null &&
        publication.session === lastPresentedPublication.session &&
        publication.sequence < lastPresentedPublication.sequence) {
      return;
    }
    lastPresentedPublication = publication;
    if (pendingPresentation !== null &&
        samePublication(pendingPresentation.publication, publication)) {
      const { resolve } = pendingPresentation;
      pendingPresentation = null;
      resolve();
    }
  };
  const state = (type) => {
    if (player === null) {
      if (continuation === null) {
        throw new Error("semantic continuation is idle in its authoring context");
      }
      const time = context.liveHandoffDuration();
      if (!Number.isFinite(time) || time < 0) {
        throw new Error("returned semantic continuation has no valid authored time");
      }
      return { type, time, playing: false, nextPatchSequence: "0" };
    }
    return { type, time: player.time(), playing: player.isPlaying(), nextPatchSequence: "0" };
  };
  const emitContinuationWake = (cadence, timerAfterMilliseconds, force = false) => {
    if (continuation === null) return;
    if (!force && cadence === "idle" && continuationWakeCadence === "idle") return;
    continuationWakeCadence = cadence;
    renderPort.postMessage({
      type: "execution_wake",
      cadence,
      timerAfterMilliseconds: timerAfterMilliseconds ?? null,
    });
  };
  const observeContinuationWake = (wallTime, force = false, emit = true, reanchor = false) => {
    const wake = reanchor
      ? player.reanchorLiveSegmentWake(wallTime)
      : player.liveSegmentWake(wallTime);
    const cadence = wake.cadence;
    const timerAfterMilliseconds = wake.timerAfterMilliseconds;
    wake.free?.();
    if (cadence !== "animation_frame" && cadence !== "timer" && cadence !== "idle") {
      throw new Error(`unknown semantic continuation wake cadence ${cadence}`);
    }
    if (cadence === "timer" &&
        (!Number.isFinite(timerAfterMilliseconds) || timerAfterMilliseconds < 0)) {
      throw new Error("semantic continuation timer wake has an invalid delay");
    }
    if (emit) emitContinuationWake(cadence, timerAfterMilliseconds, force);
    return { cadence, timerAfterMilliseconds };
  };
  async function publishCallbackPhase(
    phaseJson,
    { initial = false, emitDelta = true, onPhaseToken = null } = {},
  ) {
    let phaseToken = null;
    if (phaseJson !== null && phaseJson !== undefined) {
      if (runRequiredCallbackPhase === null) {
        try { player.failCallbackPhaseJson(phaseJson); } catch { /* preserve original phase error */ }
        throw new Error("canonical execution requires a Python callback phase handler");
      }
      let phase;
      try {
        phase = JSON.parse(phaseJson);
        phaseToken = JSON.stringify(phase?.token);
        if (phaseToken === undefined) {
          throw new Error("canonical callback phase is missing its token");
        }
        onPhaseToken?.(phaseToken);
      } catch (error) {
        try { player.failCallbackPhaseJson(phaseJson); } catch { /* preserve parse failure */ }
        throw new Error(`canonical callback phase view was not valid JSON: ${error}`);
      }
      const phaseGeneration = callbackGeneration;
      pendingPhaseJson = phaseJson;
      try {
        const batchJson = await runRequiredCallbackPhase(phase);
        if (stopped || phaseGeneration !== callbackGeneration || player === null) {
          return { phaseToken, publication: null, interrupted: true };
        }
        player.commitCallbackPhaseJson(batchJson);
        pendingPhaseJson = null;
      } catch (error) {
        if (!stopped && phaseGeneration === callbackGeneration && player !== null) {
          try { player.failCallbackPhaseJson(phaseJson); } catch { /* preserve callback failure */ }
          pendingPhaseJson = null;
          callbackFault = error instanceof Error ? error : new Error(String(error));
        }
        throw error;
      }
    }
    if (stopped || player === null) {
      return { phaseToken, publication: null, interrupted: true };
    }
    return {
      phaseToken,
      publication: emitDelta
        ? send(initial ? player.initialDeltaJson() : player.drainDeltaJson())
        : null,
      interrupted: false,
    };
  }

  async function advanceToAuthoredTime(time) {
    const phaseTokens = new Set();
    let phaseCount = 0;
    while (!stopped && player !== null) {
      if (phaseCount >= MAX_REQUIRED_CALLBACK_PHASES_PER_ADVANCE && player.time() < time) {
        throw new Error(
          "forward authored-time advance reached its callback phase bound before the requested time",
        );
      }
      const result = await publishCallbackPhase(
        player.advanceForwardToCallbackPhaseJson(time),
        {
          emitDelta: false,
          onPhaseToken(token) {
            if (phaseCount >= MAX_REQUIRED_CALLBACK_PHASES_PER_ADVANCE) {
              throw new Error(
                "forward authored-time advance exceeded its callback phase bound",
              );
            }
            if (phaseTokens.has(token)) {
              throw new Error("canonical callback advance repeated a phase token");
            }
            phaseTokens.add(token);
            phaseCount += 1;
          },
        },
      );
      if (result.interrupted) return null;
      if (result.phaseToken !== null) continue;
      if (Math.abs(player.time() - time) > 1e-9) {
        throw new Error(
          `forward authored-time advance stopped at ${player.time()} before requested time ${time}`,
        );
      }
      return send(player.drainDeltaJson());
    }
    return null;
  }

  function applyNativeInput(message) {
    if (message.type === "native_state_input") {
      player.setNativeStateInputJson(JSON.stringify({
        source: message.source,
        value: message.value,
      }));
    } else if (message.type === "native_event") {
      player.emitNativeEventJson(JSON.stringify({ source: message.source }));
    } else {
      throw new Error(`unsupported continuation input ${message.type}`);
    }
  }

  async function settleContinuationPublication(publication) {
    while (!stopped && player !== null && continuationActive) {
      await awaitPresentation(publication);
      if (stopped || player === null || !continuationActive) return false;
      if (controls.length === 0) return true;
      // Input admitted while presentation was pending belongs to this lease.
      // Apply its bounded ordered queue at the same authored time, then wait
      // for the resulting coherent publication before completion or return.
      for (const message of controls.splice(0)) {
        try {
          applyNativeInput(message);
          post({ requestId: message.requestId, ...state(message.type) });
        } catch (error) { fail(error, message.requestId); }
      }
      publication = send(player.drainDeltaJson());
    }
    return false;
  }

  async function driveContinuation(wallTime) {
    if (!continuationActive || player === null) return;
    const { cadence, timerAfterMilliseconds } = observeContinuationWake(wallTime, false, false);
    if (cadence === "idle" || (cadence === "timer" && timerAfterMilliseconds > 0)) {
      emitContinuationWake(cadence, timerAfterMilliseconds);
      return;
    }

    // Rust chooses every barrier and retains the exact phase token. The source
    // stack merely services it, then this loop asks Rust to continue with the
    // same wall timestamp. It is deliberately not a JavaScript time cursor.
    const phaseTokens = new Set();
    let phaseCount = 0;
    while (!stopped && continuationActive && player !== null) {
      const drive = player.driveLiveSegmentFromWallTime(wallTime);
      if (drive === null || typeof drive !== "object") {
        throw new Error("semantic continuation drive returned an invalid result");
      }
      const phaseJson = drive.callbackPhaseJson;
      const reachedEndpoint = drive.reachedEndpoint === true;
      drive.free?.();

      if (phaseJson !== null && phaseJson !== undefined) {
        const result = await publishCallbackPhase(phaseJson, {
          emitDelta: false,
          onPhaseToken(token) {
            if (phaseCount >= MAX_REQUIRED_CALLBACK_PHASES_PER_ADVANCE) {
              throw new Error("semantic continuation exceeded its callback phase bound");
            }
            if (phaseTokens.has(token)) {
              throw new Error("semantic continuation repeated a callback phase token");
            }
            phaseTokens.add(token);
            phaseCount += 1;
          },
        });
        if (result.interrupted) return;
        continue;
      }
      if (!reachedEndpoint) {
        // A required callback stalls simulation; exclude its wall latency from
        // the next Rust wake anchor after all same-timestamp retries finish.
        observeContinuationWake(
          phaseCount > 0 ? performance.now() : wallTime, true, true, phaseCount > 0,
        );
        return;
      }

      const endpointPublication = send(player.drainDeltaJson());

      emitContinuationWake("idle", null, true);
      if (!await settleContinuationPublication(endpointPublication)) return;
      player.completeLiveSegment();
      const completionPublication = send(player.drainDeltaJson());
      if (!await settleContinuationPublication(completionPublication)) return;
      const completedPlayer = player;
      player = null;
      continuationActive = false;
      context.returnExecutionPlayer(completedPlayer);
      emitContinuationWake("idle", null);
      continuation?.onComplete(continuationGeneration);
      return;
    }
  }

  async function drain() {
    if (stopped || !transport || draining) return;
    draining = true;
    try {
      while (controls.length && writable()) {
      const message = controls.shift();
      try {
        switch (message.type) {
          case "pause": player.pause(); break;
          case "resume": player.resume(); break;
          case "set_loop_duration": player.setLoopDuration(message.loopDurationSeconds); break;
          case "seek": latestTick = null; send(player.seekDeltaJson(message.time)); break;
          case "restart_playback": latestTick = null; send(player.seekDeltaJson(0)); break;
          case "advance_to": {
            if (callbackFault !== null) throw callbackFault;
            if (player.isPlaying()) {
              throw new Error(
                "pause semantic execution before forward authored-time advancement",
              );
            }
            latestTick = null;
            const publication = await advanceToAuthoredTime(message.time);
            await awaitPresentation(publication);
            break;
          }
          case "native_state_input":
          case "native_event":
            applyNativeInput(message);
            send(player.drainDeltaJson());
            break;
          default: throw new Error(`unsupported semantic execution command ${message.type}`);
        }
        if (stopped || player === null) break;
        post({ requestId: message.requestId, ...state(message.type) });
      } catch (error) { fail(error, message.requestId); }
      }
      if (!controls.length && latestTick !== null && writable()) {
        const timestamp = latestTick;
        latestTick = null;
        // An opaque callback failure/interruption is terminal in the retained
        // session. Do not emit repeated errors or retry its externally visible
        // side effects while this player is retained for recovery.
        if (callbackFault === null) {
          try {
            if (continuationActive) {
              // A render-worker timestamp may use a different time origin.
              // Its tick is only a wake opportunity; Rust progression stays
              // anchored to this authoring worker's monotonic clock.
              await driveContinuation(performance.now());
            } else if (player !== null) {
              await publishCallbackPhase(player.tickCallbackPhaseJson(timestamp));
            }
          } catch (error) {
            // Session progression errors are terminal for opaque callbacks too:
            // retrying the same tick could repeat externally visible Python work.
            callbackFault = error instanceof Error ? error : new Error(String(error));
            fail(callbackFault);
            continuation?.onError(continuationGeneration, callbackFault);
            stop();
          }
        }
      }
    } finally {
      draining = false;
      if (!stopped && ((controls.length && writable()) || (latestTick !== null && writable()))) {
        void drain();
      }
    }
  }
  function stop() {
    if (stopped) return;
    stopped = true;
    callbackGeneration += 1;
    rejectPendingPresentation(new Error("semantic execution stopped before renderer publication"));
    controls.length = 0;
    latestTick = null;
    if (player !== null) {
      if (pendingPhaseJson !== null) {
        const interruption = new Error("canonical callback phase interrupted by endpoint stop");
        try {
          if (typeof player.interruptCallbackPhaseJson === "function") {
            player.interruptCallbackPhaseJson(pendingPhaseJson);
          } else {
            player.failCallbackPhaseJson(pendingPhaseJson);
          }
        } catch { /* teardown owns final release */ }
        callbackFault = interruption;
        pendingPhaseJson = null;
      }
      // The authoring context retains this exact runtime for renderer recovery
      // and setup retries. Dropping the context is the only genuine teardown.
      if (!continuationActive) context.returnExecutionPlayer(player);
      player = null;
    }
    transport?.close?.();
    renderPort.close();
    controlPort.close();
    onStop();
  }
  try {
    if (!Number.isInteger(session) || session < 0 || session > 0xffffffff) {
      throw new Error("semantic execution session must fit u32");
    }
    if (![EXECUTION_TRANSPORT_SHARED, EXECUTION_TRANSPORT_TRANSFERABLE].includes(transportMode)) {
      throw new Error("unsupported semantic execution transport");
    }
    player = context.createExecutionPlayer(loopDurationSeconds, session);
    if (typeof player.resourceBundleBytes !== "function") {
      throw new Error("semantic execution requires retained resource bundle support");
    }
    controlPort.addEventListener("message", ({ data: message }) => {
      if (stopped) return;
      try {
        if (message?.channel !== "noon.engine" || message.protocolVersion !== 1) throw new Error("invalid semantic engine protocol");
        if (message.type === "stop") { stop(); return; }
        if (!Number.isSafeInteger(message.requestId) || message.requestId < 0) throw new Error("invalid engine request ID");
        if (message.type === "state") { post({ requestId: message.requestId, ...state("state"), sceneJson: null }); return; }
        if (message.type === "metrics") {
          post({ requestId: message.requestId, type: "metrics", metrics: { host: { enabled: false, missedDeadlines: 0, droppedLateResults: 0 } } });
          return;
        }
        if (![
          "pause", "resume", "seek", "restart_playback", "set_loop_duration", "advance_to",
          "native_state_input", "native_event",
        ].includes(message.type)) {
          throw new Error(`unsupported semantic execution command ${message.type}`);
        }
        if (continuation !== null && SOURCE_CONTINUATION_PLAYBACK_CONTROLS.has(message.type)) {
          throw new Error(
            "playback controls are unavailable while a Python source continuation owns execution",
          );
        }
        if (player === null &&
            (message.type === "native_state_input" || message.type === "native_event")) {
          throw new Error("native input requires an active Python source continuation segment");
        }
        if (controls.length >= MAX_PENDING_SEMANTIC_CONTROLS) {
          throw new Error("semantic control queue is full; wait for pending commands before retrying");
        }
        controls.push(message);
        void drain();
      } catch (error) { fail(error, message?.requestId ?? null); }
    });
    renderPort.addEventListener("message", ({ data: message }) => {
      if (stopped) return;
      if (message?.type === "tick") {
        if (!Number.isFinite(message.timestamp)) { fail(new Error("invalid render timestamp")); return; }
        latestTick = message.timestamp;
        void drain();
      } else if (message?.type === "transport_writable") void drain();
      else if (message?.type === "execution_presented") notePresentedPublication(message);
      else if (message?.type === "render_error") {
        const error = new Error(message.message);
        rejectPendingPresentation(error);
        fail(error);
      }
    });
    const resources = Uint8Array.from(player.resourceBundleBytes());
    if (resources.byteLength === 0) {
      throw new Error("semantic execution emitted an empty retained resource bundle");
    }
    renderPort.postMessage({ type: "retained_resources", bytes: resources }, [resources.buffer]);
    // A shared mailbox may already contain its initial snapshot when setup is
    // received. Install resources before exposing that mailbox to the renderer.
    if (transportMode === EXECUTION_TRANSPORT_SHARED) {
      const mailbox = createSharedExecutionMailbox(request.sharedSlotCapacity ?? 1024 * 1024);
      transport = new SharedExecutionDeltaWriter(mailbox);
      renderPort.postMessage({ type: "transport_setup", mode: transportMode, mailbox });
    } else transport = new TransferableExecutionDeltaSender(renderPort, { maxInFlight: 2, onWritable: drain });
    await publishCallbackPhase(player.initialCallbackPhaseJson(), { initial: true });
    controlPort.start();
    renderPort.start();
    continuationActive = continuation !== null;
    if (continuationActive) observeContinuationWake(performance.now(), true);
    post({ type: "ready", transportMode });
  } catch (error) { stop(); throw error; }
  return {
    stop,
    startContinuation(generation) {
      if (continuation === null) {
        throw new Error("semantic endpoint was not attached for continuation execution");
      }
      if (stopped) throw new Error("semantic continuation endpoint is stopped");
      if (continuationActive || player !== null) {
        throw new Error("semantic continuation endpoint already owns an active segment");
      }
      if (!Number.isSafeInteger(generation) || generation !== continuation.generation) {
        throw new Error("stale semantic continuation generation");
      }
      player = context.resumeExecutionPlayer();
      continuationGeneration = generation;
      continuationActive = true;
      callbackFault = null;
      latestTick = null;
      send(player.drainDeltaJson());
      observeContinuationWake(performance.now(), true);
      void drain();
    },
  };
}
