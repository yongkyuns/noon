// DOM mechanics for one selected primary pointer. Selection here chooses an
// input source, never a scene target. Runtime signals remain owned by Rust.
// Additional contacts are ignored while the selected pointer has buttons down.
// No OS/DOM capture is requested; leaving the content viewport cancels.
const BUTTON_BITS = [1, 4, 2, 8, 16, 32];

export function attachBrowserPointerInput(canvas, {
  signal, isCurrent, send, allocateSource, viewRevision, advanceView, onError,
}) {
  let selected = null;
  let view = null;
  const active = () => !signal.aborted && isCurrent();
  const cancel = (kind = "cancel") => {
    if (selected === null) return;
    const previous = selected;
    selected = null;
    send({
      kind, source_id: previous.source, pointer_id: previous.id,
      view_revision: previous.viewRevision,
      surface_x: null, surface_y: null,
    });
  };
  const invalidateView = () => {
    if (!active()) return;
    cancel();
    view = null;
  };
  const currentView = () => {
    const rect = canvas.getBoundingClientRect();
    const next = [rect.left, rect.top, rect.width, rect.height, window.devicePixelRatio || 1];
    if (!next.every(Number.isFinite) || rect.width <= 0 || rect.height <= 0 || next[4] <= 0) {
      invalidateView();
      return null;
    }
    if (view !== null && next.some((value, i) => value !== view[i])) {
      cancel();
      advanceView();
    }
    view = next;
    return rect;
  };
  const receive = (type, event) => {
    if (!active()) return;
    // -1 is the non-pointing-device ID, not a source for pointer gestures.
    if (event.isPrimary !== true || event.pointerId === -1) return;
    if (!Number.isInteger(event.pointerId) || event.pointerId < -2147483648 ||
        event.pointerId > 2147483647) throw new TypeError("invalid DOM pointer identity");
    const matches = selected !== null && event.pointerId === selected.id;
    if (type === "pointercancel" || type === "pointerleave" || type === "lostpointercapture") {
      // A foreign contact must never release/cancel the selected pointer. After
      // a successful release, implicit capture loss adds no cancellation edge.
      if (matches && (type !== "lostpointercapture" || selected.buttons !== 0)) {
        cancel(type === "lostpointercapture" ? "capture_lost" : "cancel");
      }
      return;
    }
    if (selected !== null && !matches && selected.buttons !== 0) return;
    if (!Number.isInteger(event.buttons) || event.buttons < 0 || event.buttons > 63) {
      throw new TypeError("unsupported DOM pointer button mask");
    }
    if (!Number.isFinite(event.clientX) || !Number.isFinite(event.clientY)) {
      throw new TypeError("DOM pointer coordinates must be finite");
    }
    const rect = currentView();
    if (rect === null) return;
    // View invalidation and pointer replacement retire the previous contact.
    // Never turn a release or an in-progress outside contact into a new press.
    if (selected === null || event.pointerId !== selected.id) {
      if (type === "pointerup" || (type === "pointermove" && event.buttons !== 0)) return;
      cancel();
      selected = { id: event.pointerId, source: allocateSource(), buttons: 0, viewRevision: viewRevision() };
    }
    let kind = "move";
    let button = null;
    if (type === "pointerdown" || type === "pointerup" || event.button !== -1) {
      button = event.button;
      const bit = BUTTON_BITS[button];
      if (bit === undefined) throw new TypeError("unsupported DOM pointer button");
      const pressed = (event.buttons & bit) !== 0;
      const wasPressed = (selected.buttons & bit) !== 0;
      if (pressed === wasPressed || (selected.buttons ^ event.buttons) !== bit) {
        // Missing platform history cannot be reconstructed into successful edges.
        cancel();
        return;
      }
      kind = pressed ? "press" : "release";
    } else if (event.buttons !== selected.buttons) {
      cancel();
      return;
    }
    send({
      kind, source_id: selected.source, pointer_id: selected.id,
      surface_x: event.clientX - rect.left, surface_y: event.clientY - rect.top,
      viewport_width: rect.width, viewport_height: rect.height,
      button, view_revision: selected.viewRevision,
      shift: event.shiftKey === true, control: event.ctrlKey === true,
      alt: event.altKey === true, meta: event.metaKey === true,
    });
    selected.buttons = event.buttons;
    // Touch IDs may be recycled; the next contact must receive a new source.
    if (kind === "release" && selected.buttons === 0 && event.pointerType !== "mouse") selected = null;
  };
  const guard = operation => event => {
    try { operation(event); } catch (error) { onError(error); }
  };
  for (const type of ["pointermove", "pointerdown", "pointerup", "pointercancel", "pointerleave", "lostpointercapture"]) {
    canvas.addEventListener(type, guard(event => receive(type, event)), { signal });
  }
  window.addEventListener("blur", guard(() => { if (active()) cancel("focus_lost"); }), { signal });
  return { invalidateView };
}
