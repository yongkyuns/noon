// Platform RAF callbacks across the document/worker boundary. Pulses carry no
// scene state or time: the worker samples its existing local presentation clock.
const CHANNEL = "noon.render";
const VERSION = 1;
const REQUEST = "animation_frame_request";
const CANCEL = "animation_frame_cancel";
const FRAME = "animation_frame";
const MAX_CALLBACKS = 64;

function envelope(type, frameId) {
  return { channel: CHANNEL, protocolVersion: VERSION, type, frameId };
}

function isMessage(message, type, alternate = null) {
  return message?.channel === CHANNEL && message.protocolVersion === VERSION &&
    (message.type === type || (alternate !== null && message.type === alternate));
}

function checkedId(id) {
  if (!Number.isSafeInteger(id) || id <= 0) throw new Error("invalid render frame ID");
  return id;
}

export function createDocumentFrameSource({ request, cancel, send }) {
  let pending = null, closed = false;
  return {
    receive(message) {
      if (!isMessage(message, REQUEST, CANCEL)) return false;
      const frameId = checkedId(message.frameId);
      if (closed) return true;
      if (message.type === CANCEL) {
        if (pending?.frameId === frameId) {
          cancel(pending.handle);
          pending = null;
        }
      } else {
        if (pending !== null) throw new Error("render frame request is already pending");
        const reservation = { frameId, handle: null };
        pending = reservation;
        try {
          reservation.handle = request(() => {
            if (pending !== reservation || closed) return;
            pending = null;
            send(envelope(FRAME, frameId));
          });
        } catch (error) {
          pending = null;
          throw error;
        }
      }
      return true;
    },
    close() {
      closed = true;
      if (pending !== null) cancel(pending.handle);
      pending = null;
    },
  };
}

export function createWorkerFrameSource({ send, now }) {
  const callbacks = new Map();
  let nextId = 0, pulse = null, closed = false;
  const cancel = (handle) => {
    if (!callbacks.delete(handle)) return;
    if (callbacks.size === 0 && pulse !== null) {
      const frameId = pulse;
      pulse = null;
      send(envelope(CANCEL, frameId));
    }
  };
  return {
    request(callback) {
      if (closed) throw new Error("render frame source is closed");
      if (typeof callback !== "function") throw new TypeError("render frame callback must be a function");
      if (callbacks.size >= MAX_CALLBACKS) throw new Error("render frame callback limit exceeded");
      if (nextId >= Number.MAX_SAFE_INTEGER) throw new Error("render frame ID space exhausted");
      const handle = ++nextId;
      callbacks.set(handle, callback);
      if (pulse === null) {
        pulse = handle;
        try {
          send(envelope(REQUEST, pulse));
        } catch (error) {
          pulse = null;
          callbacks.delete(handle);
          throw error;
        }
      }
      return handle;
    },
    cancel,
    receive(message) {
      if (!isMessage(message, FRAME)) return false;
      const frameId = checkedId(message.frameId);
      if (closed || pulse !== frameId) return true;
      pulse = null;
      // Snapshot handles, not callbacks. A callback may cancel a later callback
      // in this same native frame; newly requested callbacks wait for the next.
      const handles = [...callbacks.keys()];
      const timestamp = now();
      let failed = false, firstError;
      for (const handle of handles) {
        const callback = callbacks.get(handle);
        if (callback === undefined) continue;
        callbacks.delete(handle);
        try { callback(timestamp); } catch (error) {
          if (!failed) { failed = true; firstError = error; }
        }
      }
      if (failed) throw firstError;
      return true;
    },
    close() {
      if (closed) return;
      closed = true;
      callbacks.clear();
      if (pulse !== null) {
        const frameId = pulse;
        pulse = null;
        send(envelope(CANCEL, frameId));
      }
    },
  };
}
