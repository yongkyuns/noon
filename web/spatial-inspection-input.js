// Collect normalized platform occurrences; Rust interprets gestures and picks.
import { wheelDeltaCssPixels, wheelLinePixels } from "./browser-wheel-units.js";
import { MAX_BROWSER_POINTER_SAMPLES } from "./browser-pointer-input.js";

const RELEASE_GRACE_MS = 40;
const MAX_PENDING_PACKETS = 32;
const phaseFor = {
  pointerdown: "press", pointermove: "move", pointerup: "release", pointercancel: "cancel",
};

export function attachSpatialInspectionInput(canvas, { signal, enabled, spatial = () => true, beginPlanarGesture = () => {}, viewRevision, presentation, send, onError }) {
  const contacts = new Map();
  let firstPlanarTouch = null;
  const queue = [];
  let inFlight = false;

  const retire = id => {
    clearTimeout(contacts.get(id)?.timer);
    contacts.delete(id);
    if (canvas.hasPointerCapture?.(id)) canvas.releasePointerCapture(id);
  };
  const drain = async () => {
    if (inFlight || signal.aborted) return;
    inFlight = true;
    try {
      while (queue.length && !signal.aborted) await send(queue.shift());
    } catch (error) {
      queue.length = 0;
      if (!signal.aborted) onError(error);
    } finally {
      inFlight = false;
    }
  };
  const enqueue = packet => {
    // Bound dense motion without racing ordered press, release and cancel.
    const tail = queue.at(-1);
    if (packet.samples?.every(sample => sample.phase === "move") &&
        tail?.view_revision === packet.view_revision &&
        tail.samples?.every(sample => sample.phase === "move")) {
      const capacity = MAX_BROWSER_POINTER_SAMPLES - tail.samples.length;
      tail.samples.push(...packet.samples.slice(0, capacity));
      if (capacity < packet.samples.length) tail.samples[tail.samples.length - 1] = packet.samples.at(-1);
    } else if (queue.length < MAX_PENDING_PACKETS) {
      queue.push(packet);
    } else {
      // A release may already have retired its DOM capture while still queued.
      // Clear the worker gesture too when dropping ordered edges.
      queue.length = 0;
      queue.push({ view_revision: viewRevision(), cancel_contacts: true });
      firstPlanarTouch = null;
      for (const id of contacts.keys()) retire(id);
    }
    void drain();
  };
  const sendCancel = id => {
    const contact = contacts.get(id);
    if (contact === undefined) return;
    enqueue({
      view_revision: contact.revision,
      samples: [{ id, phase: "cancel", x: 0, y: 0, touch: false, pan: false, timestamp_ms: 0 }],
    });
    retire(id);
  };
  const cancel = () => {
    firstPlanarTouch = null;
    for (const id of contacts.keys()) sendCancel(id);
  };
  const listen = (type, handler, capture = () => true) => {
    canvas.addEventListener(type, event => {
      if (!enabled()) return;
      try {
        if (!capture(event)) return;
        event.stopImmediatePropagation();
        if (event.cancelable) event.preventDefault();
        handler(event);
      } catch (error) {
        cancel();
        onError(error);
      }
    }, { capture: true, passive: false, signal });
  };
  const collect = event => {
    const id = event.pointerId;
    const phase = phaseFor[event.type];
    const touch = event.pointerType === "touch";
    const revision = viewRevision();
    const rect = canvas.getBoundingClientRect();
    if (!Number.isFinite(rect.height) || rect.height <= 0) {
      cancel();
      return;
    }
    if (phase === "press") {
      if (contacts.has(id) || contacts.size >= 2 || (!touch && ![0, 1, 2].includes(event.button))) return;
      contacts.set(id, { revision });
      canvas.setPointerCapture(id);
    }
    const contact = contacts.get(id);
    if (contact === undefined) return;
    if (contact.revision !== revision || phase === "cancel") {
      sendCancel(id);
      return;
    }
    const sample = value => ({
      id, phase, touch, pan: phase === "press" && (!spatial() || event.button !== 0 || event.shiftKey === true),
      x: (value.clientX - rect.left) / rect.height,
      y: (value.clientY - rect.top) / rect.height,
      timestamp_ms: value.timeStamp,
    });
    const coalesced = phase === "move" ? event.getCoalescedEvents?.() : null;
    const samples = coalesced?.length
      ? coalesced.slice(-MAX_BROWSER_POINTER_SAMPLES).map(sample)
      : [sample(event)];
    enqueue({ view_revision: revision, samples, ...(phase === "release" ? { presentation: presentation() } : {}) });
    if (phase === "release") retire(id);
  };
  const capturePointer = event => {
    if (spatial() || contacts.has(event.pointerId)) return true;
    if (event.pointerType !== "touch") {
      if (event.type !== "pointerdown" || !(event.button === 1 || event.button === 2 ||
          (event.button === 0 && event.shiftKey))) return false;
      beginPlanarGesture();
      return true;
    }

    // One finger retains the ordinary authored input path. A second finger
    // cancels that contact before acquiring a camera gesture.
    if (event.type === "pointerup" || event.type === "pointercancel") {
      if (firstPlanarTouch?.pointerId === event.pointerId) firstPlanarTouch = null;
    } else if (event.type === "pointermove" && firstPlanarTouch?.pointerId === event.pointerId) {
      firstPlanarTouch = event;
    } else if (event.type === "pointerdown") {
      if (firstPlanarTouch === null) {
        firstPlanarTouch = event;
      } else if (firstPlanarTouch.pointerId !== event.pointerId) {
        const first = firstPlanarTouch;
        firstPlanarTouch = null;
        beginPlanarGesture();
        collect({
          type: "pointerdown", pointerId: first.pointerId, pointerType: "touch", button: 0,
          clientX: first.clientX, clientY: first.clientY, timeStamp: event.timeStamp,
        });
        return true;
      }
    }
    return false;
  };
  for (const type of Object.keys(phaseFor)) listen(type, collect, capturePointer);
  listen("lostpointercapture", event => {
    const contact = contacts.get(event.pointerId);
    if (contact === undefined || canvas.hasPointerCapture?.(event.pointerId)) return;
    // Chrome can retarget release after reporting capture loss with no button.
    if (event.pointerType === "mouse" && event.buttons === 0 && event.button === -1) {
      contact.timer = setTimeout(() => sendCancel(event.pointerId), RELEASE_GRACE_MS);
    } else {
      sendCancel(event.pointerId);
    }
  }, event => {
    if (spatial() || contacts.has(event.pointerId)) return true;
    if (firstPlanarTouch?.pointerId === event.pointerId) firstPlanarTouch = null;
    return false;
  });
  listen("contextmenu", () => {});
  const reset = () => {
    if (!spatial()) beginPlanarGesture();
    enqueue({ view_revision: viewRevision(), orbit: [0, 0], pan: [0, 0], zoom: 0, reset: true });
  };
  listen("dblclick", reset, () => !spatial());
  listen("wheel", event => {
    const rect = canvas.getBoundingClientRect();
    const delta = wheelDeltaCssPixels(canvas, event, () => wheelLinePixels(canvas), rect);
    if (delta.y === 0) return;
    const revision = viewRevision();
    const tail = queue.at(-1);
    if (tail?.view_revision === revision && !tail.reset && tail.zoom !== undefined) {
      tail.zoom += delta.y / 500;
    } else {
      enqueue({ view_revision: revision, orbit: [0, 0], pan: [0, 0], zoom: delta.y / 500, reset: false });
    }
  }, spatial);

  globalThis.window.addEventListener("pointerup", event => {
    if (enabled() && event.target !== canvas && contacts.has(event.pointerId)) collect(event);
  }, { signal, capture: true });
  globalThis.window.addEventListener("blur", cancel, { signal });
  signal.addEventListener("abort", () => {
    for (const id of contacts.keys()) retire(id);
    queue.length = 0;
  }, { once: true });
  return { cancel, reset };
}
