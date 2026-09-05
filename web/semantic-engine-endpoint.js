// Runs an existing semantic session in its authoring worker. Rendering uses the
// same mailbox and delta receiver as other execution producers.
import {
  EXECUTION_TRANSPORT_SHARED, EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaWriter, TransferableExecutionDeltaSender,
  createSharedExecutionMailbox,
} from "./execution-transport.js";

export function attachSemanticEngine(context, request, onStop = () => {}) {
  const { controlPort, renderPort, transportMode, loopDurationSeconds, session } = request;
  if (!(controlPort instanceof MessagePort) || !(renderPort instanceof MessagePort)) {
    throw new Error("semantic execution requires control and render ports");
  }
  let player = null;
  let transport;
  let stopped = false;
  let latestTick = null;
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
  function drain() {
    if (stopped || !transport) return;
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
      try { send(player.tickDeltaJson(timestamp)); } catch (error) { fail(error); }
    }
  }
  function stop() {
    if (stopped) return;
    stopped = true;
    controls.length = 0;
    latestTick = null;
    transport?.close?.();
    renderPort.close();
    controlPort.close();
    player?.free();
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
        controls.push(message);
        drain();
      } catch (error) { fail(error, message?.requestId ?? null); }
    });
    renderPort.addEventListener("message", ({ data: message }) => {
      if (stopped) return;
      if (message?.type === "tick") {
        if (!Number.isFinite(message.timestamp)) { fail(new Error("invalid render timestamp")); return; }
        latestTick = message.timestamp;
        drain();
      } else if (message?.type === "transport_writable") drain();
      else if (message?.type === "render_error") fail(new Error(message.message));
    });
    if (transportMode === EXECUTION_TRANSPORT_SHARED) {
      const mailbox = createSharedExecutionMailbox(request.sharedSlotCapacity ?? 1024 * 1024);
      transport = new SharedExecutionDeltaWriter(mailbox);
      renderPort.postMessage({ type: "transport_setup", mode: transportMode, mailbox });
    } else transport = new TransferableExecutionDeltaSender(renderPort, { maxInFlight: 2, onWritable: drain });
    send(player.initialDeltaJson());
    controlPort.start();
    renderPort.start();
    post({ type: "ready", transportMode });
  } catch (error) { stop(); throw error; }
  return { stop };
}
