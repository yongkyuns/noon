// Runs an existing semantic session in its authoring worker. Rendering uses the
// same mailbox and delta receiver as other execution producers.
import {
  EXECUTION_TRANSPORT_SHARED, EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaWriter, TransferableExecutionDeltaSender,
  createSharedExecutionMailbox, executionDeltaMetadata,
} from "./execution-transport.js";

export const MAX_PENDING_SEMANTIC_CONTROLS = 128;
export const MAX_REQUIRED_CALLBACK_PHASES_PER_ADVANCE = 128;

export async function attachSemanticEngine(
  context,
  request,
  onStop = () => {},
  runRequiredCallbackPhase = null,
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
  let pendingRendererObservation = null;
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
  const sendRendererObservationPublication = (json) => {
    if (typeof json !== "string") {
      throw new Error("renderer observation publication must be JSON");
    }
    let envelope;
    try {
      envelope = JSON.parse(json);
    } catch (error) {
      throw new Error(`renderer observation publication is invalid JSON: ${error.message}`);
    }
    const deltaJson = JSON.stringify(envelope?.delta);
    const publication = executionDeltaMetadata(deltaJson);
    const requested = envelope?.observation?.publication;
    if (!samePublication(requested, publication)) {
      throw new Error("renderer observation does not match its retained publication");
    }
    if (pendingRendererObservation !== null) {
      throw new Error("semantic endpoint already awaits a renderer observation");
    }
    let resolveObservation;
    let rejectObservation;
    const observation = new Promise((resolve, reject) => {
      resolveObservation = resolve;
      rejectObservation = reject;
    });
    renderPort.postMessage({
      type: "renderer_observation_request",
      session: publication.session,
      sequence: publication.sequence,
      json: JSON.stringify(envelope.observation),
    });
    try {
      const sent = send(deltaJson);
      if (!samePublication(sent, publication)) {
        throw new Error("renderer observation publication changed during transport send");
      }
    } catch (error) {
      renderPort.postMessage({
        type: "renderer_observation_cancel",
        session: publication.session,
        sequence: publication.sequence,
      });
      throw error;
    }
    pendingRendererObservation = {
      publication,
      resolve: resolveObservation,
      reject: rejectObservation,
    };
    return { publication, observation };
  };
  const samePublication = (left, right) =>
    left != null && right != null &&
    left.session === right.session && left.sequence === right.sequence;
  const publicationAlreadyPresented = (publication) =>
    samePublication(lastPresentedPublication, publication);
  const rejectPendingPresentation = (error) => {
    if (pendingPresentation === null) return;
    const { reject } = pendingPresentation;
    pendingPresentation = null;
    reject(error);
  };
  const rejectPendingRendererObservation = (error) => {
    if (pendingRendererObservation === null) return;
    const { reject } = pendingRendererObservation;
    pendingRendererObservation = null;
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
  const noteRendererObservation = (message) => {
    const publication = message && {
      session: message.session,
      sequence: message.sequence,
    };
    if (pendingRendererObservation === null ||
        !samePublication(publication, pendingRendererObservation.publication)) {
      fail(new Error("renderer reported an invalid publication observation"));
      return;
    }
    if (typeof message.json !== "string") {
      const error = new Error("renderer observation result must be JSON");
      rejectPendingRendererObservation(error);
      fail(error);
      return;
    }
    let observation;
    try {
      observation = JSON.parse(message.json);
    } catch (error) {
      const invalid = new Error(`renderer observation is invalid JSON: ${error.message}`);
      rejectPendingRendererObservation(invalid);
      fail(invalid);
      return;
    }
    const observedPublication = observation?.publication ?? observation?.requested;
    if (!samePublication(observedPublication, publication)) {
      const error = new Error("renderer observation result does not match its publication");
      rejectPendingRendererObservation(error);
      fail(error);
      return;
    }
    const { resolve } = pendingRendererObservation;
    pendingRendererObservation = null;
    resolve(observation);
  };
  const state = (type) => ({ type, time: player.time(), playing: player.isPlaying(), nextPatchSequence: "0" });
  async function publishCallbackPhase(
    phaseJson,
    { initial = false, emitDelta = true, onPhaseToken = null, observeAtTime = null } = {},
  ) {
    let phaseToken = null;
    let rendererObservation = null;
    let phase = null;
    if (phaseJson !== null && phaseJson !== undefined) {
      if (runRequiredCallbackPhase === null) {
        try { player.failCallbackPhaseJson(phaseJson); } catch { /* preserve original phase error */ }
        throw new Error("canonical execution requires a Python callback phase handler");
      }
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
      if (observeAtTime !== null && Math.abs(phase.time - observeAtTime) <= 1e-9) {
        const target = callbackObservationTarget(phase);
        rendererObservation = sendRendererObservationPublication(
          player.drainRendererObservationPublicationJson(
            phaseJson,
            target.slot,
            target.generation,
          ),
        );
      }
    }
    if (stopped || player === null) {
      return { phaseToken, publication: null, interrupted: true };
    }
    return {
      phaseToken,
      publication: rendererObservation?.publication ?? (emitDelta
        ? send(initial ? player.initialDeltaJson() : player.drainDeltaJson())
        : null),
      rendererObservation,
      interrupted: false,
    };
  }

  async function advanceToAuthoredTime(time, observeRenderer) {
    const phaseTokens = new Set();
    let phaseCount = 0;
    let rendererObservation = null;
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
          observeAtTime: observeRenderer && rendererObservation === null ? time : null,
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
      rendererObservation ??= result.rendererObservation;
      if (result.phaseToken !== null) continue;
      if (Math.abs(player.time() - time) > 1e-9) {
        throw new Error(
          `forward authored-time advance stopped at ${player.time()} before requested time ${time}`,
        );
      }
      const publication = send(player.drainDeltaJson());
      return {
        publication: publication ?? rendererObservation?.publication ?? null,
        rendererObservation,
      };
    }
    return null;
  }

  async function drain() {
    if (stopped || !transport || draining) return;
    draining = true;
    try {
      while (controls.length && writable()) {
      const message = controls.shift();
      let rendererObservation = null;
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
            const advanced = await advanceToAuthoredTime(
              message.time,
              message.observeRenderer === true,
            );
            const observation = advanced?.rendererObservation?.observation ??
              Promise.resolve(null);
            [, rendererObservation] = await Promise.all([
              awaitPresentation(advanced?.publication ?? null),
              observation,
            ]);
            break;
          }
          case "native_state_input":
            player.setNativeStateInputJson(JSON.stringify({
              source: message.source,
              value: message.value,
            }));
            send(player.drainDeltaJson());
            break;
          case "native_event":
            player.emitNativeEventJson(JSON.stringify({ source: message.source }));
            send(player.drainDeltaJson());
            break;
          default: throw new Error(`unsupported semantic execution command ${message.type}`);
        }
        if (stopped || player === null) break;
        post({
          requestId: message.requestId,
          ...state(message.type),
          ...(rendererObservation === null ? {} : { rendererObservation }),
        });
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
            await publishCallbackPhase(player.tickCallbackPhaseJson(timestamp));
          } catch (error) {
            // Session progression errors are terminal for opaque callbacks too:
            // retrying the same tick could repeat externally visible Python work.
            callbackFault = error instanceof Error ? error : new Error(String(error));
            fail(callbackFault);
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
    rejectPendingRendererObservation(
      new Error("semantic execution stopped before renderer observation"),
    );
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
      context.returnExecutionPlayer(player);
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
        if (message.type === "advance_to" && message.observeRenderer !== undefined &&
            typeof message.observeRenderer !== "boolean") {
          throw new Error("renderer observation flag must be boolean");
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
      else if (message?.type === "renderer_observation") noteRendererObservation(message);
      else if (message?.type === "render_error") {
        const error = new Error(message.message);
        rejectPendingPresentation(error);
        rejectPendingRendererObservation(error);
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
    post({ type: "ready", transportMode });
  } catch (error) { stop(); throw error; }
  return { stop };
}

function callbackObservationTarget(phase) {
  if (!Array.isArray(phase?.invocations) || phase.invocations.length === 0) {
    throw new Error("renderer observation requires a callback target");
  }
  const target = phase.invocations[0]?.target;
  if (!Number.isSafeInteger(target?.slot) || target.slot < 0 ||
      !Number.isSafeInteger(target?.generation) || target.generation < 0) {
    throw new Error("renderer observation callback target is invalid");
  }
  // Observation remains bounded to one target even when the committed phase
  // contains other callback targets. Authoring order makes the first target a
  // deterministic diagnostic selection without copying or exposing the full
  // callback object set to the renderer.
  return target;
}
