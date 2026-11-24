import {
  capturePausedFeedback, copyDisplayedCanvas, feedbackImageBounds, feedbackPoint, feedbackRegion,
} from "./playground-feedback-capture.js";

// The preview overlay consumes annotation gestures while playback is paused.
export function installPlaygroundFeedback(button, { getContext, onPaused = () => {} }) {
  const document = button?.ownerDocument;
  if (!document || document.documentElement.dataset.noonEmbed === "true") return null;
  const window = document.defaultView;
  const preview = button.closest(".preview-pane");
  const stage = document.createElement("div");
  stage.className = "noon-feedback-image";
  stage.hidden = true;
  stage.tabIndex = 0;
  stage.setAttribute("role", "group");
  stage.setAttribute("aria-label", "Captured frame. Drag an area, click a point, or press Enter to mark the center.");
  stage.innerHTML = `
    <img alt="Frozen frame from the displayed Noon Run" draggable="false" hidden>
    <span class="noon-feedback-mark" aria-hidden="true" hidden></span>`;
  const panel = document.createElement("section");
  panel.id = "noon-feedback-panel";
  panel.className = "noon-feedback-panel";
  panel.setAttribute("aria-label", "Frame feedback");
  panel.hidden = true;
  panel.innerHTML = `
    <p class="noon-feedback-status" role="status" aria-live="polite"></p>
    <form novalidate>
      <label for="noon-feedback-comment">Comment</label>
      <textarea id="noon-feedback-comment" rows="2" maxlength="4000"
        placeholder="Keep this arrow visible until the explanation finishes…" required></textarea>
      <div class="noon-feedback-footer">
        <button type="button" data-action="close">Cancel</button>
        <button type="submit" disabled>Add to source</button>
      </div>
    </form>`;
  const style = document.createElement("style");
  style.textContent = `
    .noon-feedback-panel { flex: none; padding: .6rem .8rem; background: #101521;
      color: #f7f8fb; border-top: 1px solid var(--border); }
    .preview-pane:has(.noon-feedback-panel:not([hidden])) :is(.metrics, .playback-controls) { display: none; }
    .noon-feedback-footer { display: flex; flex-wrap: wrap; gap: 6px; }
    .noon-feedback-panel button { min-height: 34px; padding: 6px 10px; border: 1px solid #655894;
      border-radius: 7px; background: #292244; color: #f7f8fb; font: 600 13px/1.4 system-ui; cursor: pointer; }
    .noon-feedback-panel button:disabled { opacity: .45; cursor: not-allowed; }
    .noon-feedback-panel [type="submit"] { background: #534081; }
    .noon-feedback-panel :focus-visible, .noon-feedback-image:focus-visible { outline: 2px solid #b3a6ff; }
    .noon-feedback-image [hidden] { display: none !important; }
    .noon-feedback-status { margin: 0 0 .35rem; color: #c8c3df;
      font: 13px/1.5 system-ui; overflow-wrap: anywhere; }
    .noon-feedback-status[data-error="true"] { color: #ffadb7; }
    .noon-feedback-image { position: absolute; inset: 1px; z-index: 3; border-radius: .8rem;
      cursor: crosshair; touch-action: none; }
    .noon-feedback-image img { display: block; width: 100%; height: 100%; background: #090c13;
      object-fit: contain; border-radius: inherit; user-select: none; -webkit-user-drag: none; }
    .noon-feedback-mark { position: absolute; box-sizing: border-box; border: 2px solid #fff;
      pointer-events: none; box-shadow: 0 0 0 2px #5238a6; background: rgb(128 102 237 / 25%); }
    .noon-feedback-mark[data-shape="point"] { width: 16px; height: 16px; border-radius: 50%;
      background: #8066ed; transform: translate(-50%, -50%); }
    .noon-feedback-panel label { display: block; margin-bottom: 4px; font: 600 13px/1.5 system-ui; }
    .noon-feedback-panel textarea { position: static; display: block; box-sizing: border-box;
      width: 100%; height: auto; padding: 6px 8px; margin-bottom: 6px;
      color: #f7f8fb; background: #080b12; border: 1px solid #3a4968; border-radius: 7px;
      font: 14px/1.5 system-ui; resize: vertical; }
  `;
  document.head.append(style);
  preview.querySelector(".canvas-frame").append(stage);
  preview.append(panel);
  button.setAttribute("aria-controls", panel.id);
  button.setAttribute("aria-pressed", "false");
  button.hidden = false;
  const image = stage.querySelector("img");
  const mark = stage.querySelector(".noon-feedback-mark");
  const status = panel.querySelector(".noon-feedback-status");
  const text = panel.querySelector("textarea");
  const form = panel.querySelector("form");
  const submit = panel.querySelector('[type="submit"]');
  let capture = null, selection = null, drag = null, imageUrl = null, controller = null;
  let addToSource = null;
  let findTargets = null, identified = null, lookupPending = false;
  let releaseCapture = null;
  let generation = 0, disposed = false, timer = null, workInFlight = null;
  const update = () => {
    const size = new TextEncoder().encode(text.value).length;
    text.setCustomValidity(size > 4000 ? "Use at most 4,000 UTF-8 bytes." : "");
    submit.disabled = !capture || !selection || lookupPending ||
      !text.value.trim() || text.value.includes("\0") || size > 4000;
  };
  const message = (value, error = false) => {
    status.textContent = value;
    status.dataset.error = String(error);
  };
  const release = () => {
    generation++;
    controller?.abort(); controller = null;
    releaseCapture?.(); releaseCapture = null;
    if (timer !== null) window.clearTimeout(timer);
    timer = null;
    image.removeAttribute("src");
    if (imageUrl !== null) window.URL.revokeObjectURL(imageUrl);
    imageUrl = null;
    capture = null; selection = null; drag = null;
    addToSource = null;
    findTargets = null; identified = null; lookupPending = false;
    image.hidden = true; mark.hidden = true;
    text.value = "";
    update();
  };
  function drawSelection() {
    const region = drag?.region ?? selection?.region;
    const target = region ?? selection?.point;
    mark.hidden = !target || !capture;
    if (mark.hidden) return;
    const bounds = feedbackImageBounds(image.getBoundingClientRect(), capture.width, capture.height);
    if (!bounds) { mark.hidden = true; return; }
    const outer = stage.getBoundingClientRect();
    mark.dataset.shape = region ? "region" : "point";
    mark.style.left = `${bounds.left - outer.left + target.x * bounds.width}px`;
    mark.style.top = `${bounds.top - outer.top + target.y * bounds.height}px`;
    mark.style.width = region ? `${region.width * bounds.width}px` : "";
    mark.style.height = region ? `${region.height * bounds.height}px` : "";
  }
  async function select(next) {
    selection = next; identified = null;
    lookupPending = Boolean(next.region && findTargets);
    drawSelection(); update();
    text.focus();
    if (!lookupPending) return;
    const ticket = generation;
    let timeout;
    try {
      const result = await Promise.race([
        findTargets(capture, next.region),
        new Promise(resolve => { timeout = window.setTimeout(() => resolve(null), 750); }),
      ]);
      if (ticket === generation && selection === next) identified = result;
    } catch {
      // The rectangle remains usable if the captured publication has retired.
    } finally {
      window.clearTimeout(timeout);
      if (ticket === generation && selection === next) { lookupPending = false; update(); }
    }
  }
  function imagePoint(event, clamp = false) {
    return feedbackPoint(event.clientX, event.clientY, image.getBoundingClientRect(),
      capture.width, capture.height, clamp);
  }
  function updateDrag(event) {
    if (drag?.id !== event.pointerId) return;
    drag.moved ||= Math.hypot(event.clientX - drag.x, event.clientY - drag.y) >= 4;
    const end = imagePoint(event, true);
    drag.region = drag.moved && end ? feedbackRegion(drag.start, end) : null;
    drawSelection();
  }
  stage.addEventListener("pointerdown", event => {
    if (!capture || drag || !event.isPrimary || event.button !== 0) return;
    const start = imagePoint(event);
    if (!start) return;
    drag = { id: event.pointerId, start, x: event.clientX, y: event.clientY, moved: false, region: null };
    stage.setPointerCapture(event.pointerId);
    event.preventDefault();
  });
  stage.addEventListener("pointermove", updateDrag);
  stage.addEventListener("pointerup", event => {
    if (drag?.id !== event.pointerId) return;
    updateDrag(event);
    const { region, start, moved } = drag;
    drag = null;
    if (region) select({ region });
    else if (!moved) select({ point: start });
    else drawSelection();
  });
  stage.addEventListener("lostpointercapture", event => {
    if (drag?.id !== event.pointerId) return;
    drag = null; drawSelection();
  });
  stage.addEventListener("keydown", event => {
    if (capture && event.target === stage && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault(); select({ point: { x: 0.5, y: 0.5 } });
    }
  });
  function handleFeedbackKey(event) {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      closeAndFocus();
    }
  }
  stage.addEventListener("keydown", handleFeedbackKey);
  panel.addEventListener("keydown", handleFeedbackKey);
  panel.querySelector('[data-action="close"]').addEventListener("click", closeAndFocus);
  text.addEventListener("input", update);
  const resize = typeof window.ResizeObserver === "function" ? new window.ResizeObserver(drawSelection) : null;
  resize?.observe(stage);

  async function openComment() {
    if (disposed || !panel.hidden) return;
    release();
    const ticket = generation;
    const activeController = new AbortController();
    controller = activeController;
    const signal = activeController.signal;
    stage.hidden = false;
    panel.hidden = false;
    button.setAttribute("aria-pressed", "true");
    message("Pausing and capturing the displayed Run…");
    try {
      if (workInFlight !== null) throw new Error("The previous capture is still retiring. Try again after the preview responds.");
      const context = getContext();
      releaseCapture = context.release;
      const aborted = new Promise((_, reject) => {
        signal.addEventListener("abort", () => reject(signal.reason), { once: true });
      });
      timer = window.setTimeout(() => activeController.abort(new Error(
        "Capture timed out. Close this note and try again once the preview is ready.")), 10_000);
      const afterPaint = () => new Promise(resolve => {
        window.requestAnimationFrame(() => window.requestAnimationFrame(resolve));
      });
      const work = (async () => {
        const candidate = await capturePausedFeedback(context, {
          copyImage: copyDisplayedCanvas, afterPaint, signal, onPaused,
        });
        signal.throwIfAborted();
        if (disposed || ticket !== generation) throw new DOMException("Capture was superseded.", "AbortError");
        imageUrl = window.URL.createObjectURL(candidate.png);
        image.src = imageUrl;
        image.hidden = false;
        await image.decode();
        signal.throwIfAborted();
        if (image.naturalWidth !== candidate.width || image.naturalHeight !== candidate.height) {
          throw new Error("The captured image could not be verified.");
        }
        return candidate;
      })();
      workInFlight = work;
      void work.finally(() => { if (workInFlight === work) workInFlight = null; }).catch(() => {});
      const candidate = await Promise.race([work, aborted]);
      if (disposed || ticket !== generation || signal.aborted) return;
      capture = candidate;
      addToSource = context.addToSource;
      findTargets = context.findTargets;
      message(`Paused at ${candidate.presentationReceipt.time.toFixed(3)} s. Drag an area or click a point.`);
      stage.focus();
      update();
    } catch (error) {
      if (disposed || ticket !== generation) return;
      release();
      stage.hidden = true;
      message(error?.message ?? "The preview could not be captured.", true);
      update();
    } finally {
      if (ticket === generation && timer !== null) {
        window.clearTimeout(timer); timer = null;
      }
    }
  }
  const toggleComment = () => {
    if (panel.hidden) void openComment();
    else close();
  };
  button.addEventListener("click", toggleComment);
  form.addEventListener("submit", event => {
    event.preventDefault();
    update();
    if (submit.disabled) return;
    try {
      addToSource(capture, selection, text.value, identified);
      closeAndFocus();
    } catch (error) { message(error.message, true); }
  });
  function close() {
    if (panel.hidden) return;
    panel.hidden = true;
    stage.hidden = true;
    button.setAttribute("aria-pressed", "false");
    release();
  }
  function closeAndFocus() {
    close();
    button.focus();
  }
  return Object.freeze({
    close,
    dispose() {
      if (disposed) return;
      disposed = true; close(); resize?.disconnect();
      button.removeEventListener("click", toggleComment);
      button.hidden = true; stage.remove(); panel.remove(); style.remove();
    },
  });
}
