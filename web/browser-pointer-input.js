// DOM mechanics for one selected primary pointer. Selection here chooses an
// input source, never a scene target. Runtime signals remain owned by Rust.
// Additional contacts are ignored while the selected pointer has buttons down.
// No OS/DOM capture is requested; leaving the content viewport cancels.
const BUTTON_BITS = [1, 4, 2, 8, 16, 32];

export function attachBrowserPointerInput(canvas, {
  signal, isCurrent, send, allocateSource, viewRevision, advanceView, onError, maxSamples, onView, windowTarget = window,
}) {
  if (!Number.isSafeInteger(maxSamples) || maxSamples < 1) {
    throw new RangeError("browser pointer sample capacity must be a positive safe integer");
  }
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
    if (onView) {
      advanceView();
      onView(viewRevision(), 0, 0);
    }
  };
  const currentView = () => {
    const rect = canvas.getBoundingClientRect();
    const next = [rect.left, rect.top, rect.width, rect.height, windowTarget.devicePixelRatio || 1];
    if (!next.every(Number.isFinite) || rect.width <= 0 || rect.height <= 0 || next[4] <= 0) {
      invalidateView();
      return null;
    }
    const changed = view === null || next.some((value, i) => value !== view[i]);
    if (view !== null && changed) {
      cancel();
      advanceView();
    }
    view = next;
    if (changed) onView?.(viewRevision(), rect.width, rect.height);
    return rect;
  };
  const receive = (type, event, receiptRect = null) => {
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
    const rect = receiptRect ?? currentView();
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
    const contact = selected;
    const admitted = send({
      kind, source_id: contact.source, pointer_id: contact.id,
      surface_x: event.clientX - rect.left, surface_y: event.clientY - rect.top,
      viewport_width: rect.width, viewport_height: rect.height,
      button, view_revision: contact.viewRevision,
      shift: event.shiftKey === true, control: event.ctrlKey === true,
      alt: event.altKey === true, meta: event.metaKey === true,
    });
    // A synchronous direct host can reject a stale displayed frame. Rust has
    // already cancelled its contact. Retire this DOM source without another
    // cancellation, edge reconstruction, queued replay, or fatal detachment.
    if (admitted === false) {
      if (selected === contact) selected = null;
      return false;
    }
    // Delivery can synchronously retire the attachment/contact (for example,
    // when a presentation notification fails after Rust admitted the press).
    // Do not mutate a cancelled or replacement contact on return from send.
    if (!active() || selected !== contact) return;
    contact.buttons = event.buttons;
    // Touch IDs may be recycled; the next contact must receive a new source.
    if (kind === "release" && selected.buttons === 0 && event.pointerType !== "mouse") selected = null;
    return true;
  };
  const collect = (type, event) => {
    if (!active() || event.isPrimary !== true || event.pointerId === -1) return;
    if (selected !== null && selected.id !== event.pointerId && selected.buttons !== 0) return;
    // A held contact without an admitted press cannot resume after cancellation
    // or enter from outside. Keep ordinary parent validation/ignore semantics;
    // do not interpret its coalesced button mask as a new press transition.
    if (type === "pointermove" && event.buttons !== 0 &&
        (selected === null || selected.id !== event.pointerId)) {
      receive(type, event);
      return;
    }
    if (type !== "pointermove" || typeof event.getCoalescedEvents !== "function") {
      receive(type, event);
      return;
    }
    const initialButtons = selected?.id === event.pointerId ? selected.buttons : 0;
    const samples = coalescedSamples(event, initialButtons, maxSamples);
    if (samples.length === 0) {
      receive(type, event);
      return;
    }
    // Coalesced events replace their summary; appending it would duplicate an
    // occurrence (and may reintroduce a browser-adjusted, non-sample position).
    // This is receipt-time mapping, not historical displayed-frame association.
    const rect = currentView();
    if (rect === null || (initialButtons !== 0 && selected === null)) return;
    for (const sample of samples) {
      if (!active() || !receive(type, sample, rect)) break;
    }
  };
  const guard = operation => event => {
    try { operation(event); } catch (error) { onError(error); }
  };
  for (const type of ["pointermove", "pointerdown", "pointerup", "pointercancel", "pointerleave", "lostpointercapture"]) {
    canvas.addEventListener(type, guard(event => collect(type, event)), { signal });
  }
  windowTarget.addEventListener("blur", guard(() => { if (active()) cancel("focus_lost"); }), { signal });
  if (onView) {
    // Register before the first paint; input collection never invents a receipt.
    // Scroll/resize/element resize can invalidate mapping without pointer motion.
    advanceView();
    const refresh = guard(() => { if (active()) currentView(); });
    windowTarget.addEventListener("resize", refresh, { signal });
    windowTarget.addEventListener("scroll", refresh, { capture: true, signal });
    if (typeof windowTarget.ResizeObserver === "function") {
      const observer = new windowTarget.ResizeObserver(refresh);
      observer.observe(canvas);
      signal.addEventListener("abort", () => observer.disconnect(), { once: true });
    }
    refresh();
  }
  return { invalidateView };
}

// Validate and snapshot the complete bounded packet before any delivery. There
// is no retained queue or frontend gesture policy here. Each sample still uses
// the existing session-ordered producer reservation and acknowledgement path.
function coalescedSamples(event, initialButtons, capacity) {
  const events = event.getCoalescedEvents();
  if (!Array.isArray(events)) throw new TypeError("invalid DOM coalesced sample list");
  const count = events.length;
  if (count > capacity) throw new RangeError("DOM coalesced sample capacity exceeded");
  if (count === 0) return [];
  if (!Number.isInteger(event.pointerId) || event.pointerId < -2147483648 ||
      event.pointerId > 2147483647) throw new TypeError("invalid DOM pointer identity");
  if (!Number.isFinite(event.timeStamp) || event.timeStamp < 0) {
    throw new TypeError("invalid DOM coalesced parent timestamp");
  }
  const samples = [];
  let previousTime = 0;
  let buttons = initialButtons;
  for (let i = 0; i < count; i += 1) {
    const value = events[i];
    if (!value || value.pointerId !== event.pointerId || value.pointerType !== event.pointerType ||
        value.isPrimary !== event.isPrimary) throw new TypeError("foreign DOM coalesced pointer sample");
    const { clientX, clientY, timeStamp, buttons: nextButtons } = value;
    if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) {
      throw new TypeError("DOM coalesced pointer coordinates must be finite");
    }
    if (!Number.isFinite(timeStamp) || timeStamp < previousTime || timeStamp > event.timeStamp) {
      throw new TypeError("DOM coalesced timestamps must be ordered within their parent");
    }
    if (!Number.isInteger(nextButtons) || nextButtons < 0 || nextButtons > 63) {
      throw new TypeError("unsupported DOM coalesced pointer button mask");
    }
    // A UA may copy its parent button label into multiple samples. Only the
    // mask transition is an edge; equal masks must preserve motion, not cancel
    // as duplicate presses. Multiple simultaneous changes remain ambiguous.
    const changed = buttons ^ nextButtons;
    const button = changed === 0 ? -1 : BUTTON_BITS.indexOf(changed);
    if (changed !== 0 && (button === -1 || value.button !== button)) {
      throw new TypeError("ambiguous DOM coalesced button transition");
    }
    samples.push({
      pointerId: event.pointerId, pointerType: event.pointerType, isPrimary: event.isPrimary,
      clientX, clientY, button, buttons: nextButtons,
      shiftKey: value.shiftKey === true, ctrlKey: value.ctrlKey === true,
      altKey: value.altKey === true, metaKey: value.metaKey === true,
    });
    buttons = nextButtons;
    previousTime = timeStamp;
  }
  if (buttons !== event.buttons) throw new TypeError("DOM coalesced final button state differs from parent");
  return samples;
}
