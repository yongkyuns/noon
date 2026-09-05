// Runs an existing semantic session in its authoring worker. Rendering uses the
// same mailbox and delta receiver as other execution producers.
import {
  EXECUTION_TRANSPORT_SHARED, EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaWriter, TransferableExecutionDeltaSender,
  createSharedExecutionMailbox,
} from "./execution-transport.js";

export const MAX_PENDING_SEMANTIC_CONTROLS = 128;

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
  const controls = [];
  const post = (payload) => controlPort.postMessage({ channel: "noon.engine", protocolVersion: 1, ...payload });
  const fail = (error, requestId = null) => post({ type: "error", requestId, message: String(error?.message ?? error) });
  const writable = () => typeof transport.canSend === "function" ? transport.canSend() : transport.inFlight() < 2;
  const send = (json) => {
    if (json == null) return;
    if (!transport.send(json)) throw new Error("semantic transport became backpressured after admission");
    if (transportMode === EXECUTION_TRANSPORT_SHARED) renderPort.postMessage({ type: "shared_delta" });
  };
  const state = (type) => ({ type, time: player.time(), playing: player.isPlaying(), nextPatchSequence: "0" });
  async function publishCallbackPhase(phaseJson, { initial = false } = {}) {
    if (phaseJson !== null && phaseJson !== undefined) {
      if (runRequiredCallbackPhase === null) {
        try { player.failCallbackPhaseJson(phaseJson); } catch { /* preserve original phase error */ }
        throw new Error("canonical execution requires a Python callback phase handler");
      }
      let phase;
      try {
        phase = JSON.parse(phaseJson);
      } catch (error) {
        try { player.failCallbackPhaseJson(phaseJson); } catch { /* preserve parse failure */ }
        throw new Error(`canonical callback phase view was not valid JSON: ${error}`);
      }
      const phaseGeneration = callbackGeneration;
      pendingPhaseJson = phaseJson;
      try {
        const batchJson = await runRequiredCallbackPhase(phase);
        if (stopped || phaseGeneration !== callbackGeneration || player === null) {
          return false;
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
    if (stopped || player === null) return false;
    send(initial ? player.initialDeltaJson() : player.drainDeltaJson());
    return true;
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
          default: throw new Error(`unsupported semantic execution command ${message.type}`);
        }
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
        if (!["pause", "resume", "seek", "restart_playback", "set_loop_duration"].includes(message.type)) {
          throw new Error(`unsupported semantic execution command ${message.type}`);
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
      else if (message?.type === "render_error") fail(new Error(message.message));
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
