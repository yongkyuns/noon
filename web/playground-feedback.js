import {
  capturePausedFeedback, copyDisplayedCanvas, createLiveFeedbackExport,
  feedbackImageBounds, feedbackPoint, feedbackRegion,
} from "./playground-feedback-capture.js";

// Optional review chrome. This module never evaluates source or owns playback.
// Opening the modal blocks scene pointer gestures before a capture is requested.
export function installPlaygroundFeedback(button, { getContext, onPaused = () => {} }) {
  const document = button?.ownerDocument;
  if (!document || document.documentElement.dataset.noonEmbed === "true") return null;
  const window = document.defaultView;
  const dialog = document.createElement("dialog");
  dialog.className = "noon-feedback-dialog";
  dialog.setAttribute("aria-labelledby", "noon-feedback-title");
  dialog.innerHTML = `
    <header><h2 id="noon-feedback-title">Comment on this frame</h2>
      <button type="button" data-action="close" aria-label="Close feedback">Close</button></header>
    <p class="noon-feedback-status" role="status" aria-live="polite"></p>
    <div class="noon-feedback-image" tabindex="0" role="group"
      aria-label="Captured frame. Drag an area, click a point, or press Enter to mark the center.">
      <img alt="Frozen frame from the displayed Noon Run" draggable="false" hidden>
      <span class="noon-feedback-mark" aria-hidden="true" hidden></span>
    </div>
    <form novalidate>
      <label for="noon-feedback-comment">Comment</label>
      <textarea id="noon-feedback-comment" rows="3" maxlength="4000"
        placeholder="Keep this arrow visible until the explanation finishes…" required></textarea>
      <div class="noon-feedback-footer">
        <span class="noon-feedback-count"></span>
        <button type="button" data-action="download" disabled>Download evidence</button>
        <button type="submit" disabled>Add to source</button>
      </div>
    </form>`;
  const style = document.createElement("style");
  style.textContent = `
    .noon-feedback-dialog { color: #f7f8fb; background: #101521; border: 1px solid #3a4968;
      border-radius: 14px; padding: 20px; width: min(860px, calc(100vw - 24px));
      max-height: calc(100dvh - 24px); overflow: auto; box-sizing: border-box; }
    .noon-feedback-dialog::backdrop { background: rgb(0 0 0 / 65%); }
    .noon-feedback-dialog header, .noon-feedback-footer { display: flex; align-items: center;
      justify-content: space-between; gap: 12px; }
    .noon-feedback-dialog h2 { margin: 0; font: 600 19px/1.4 system-ui; }
    .noon-feedback-dialog button { min-height: 38px; padding: 8px 14px; border: 1px solid #655894;
      border-radius: 7px; background: #292244; color: #f7f8fb; font: 600 13px/1.4 system-ui; cursor: pointer; }
    .noon-feedback-dialog button:disabled { opacity: .45; cursor: not-allowed; }
    .noon-feedback-dialog [type="submit"] { background: #534081; }
    .noon-feedback-dialog [data-action="download"] { margin-inline-start: auto; background: transparent; }
    .noon-feedback-dialog :focus-visible { outline: 2px solid #b3a6ff; outline-offset: 3px; }
    .noon-feedback-dialog [hidden] { display: none !important; }
    .noon-feedback-dialog .noon-feedback-status { min-height: 1.5em; margin: 12px 0; color: #c8c3df;
      font: 13px/1.5 system-ui; overflow-wrap: anywhere; }
    .noon-feedback-status[data-error="true"] { color: #ffadb7; }
    .noon-feedback-image { position: relative; background: #090c13; border-radius: 8px;
      cursor: crosshair; min-height: 100px; touch-action: none; }
    .noon-feedback-image img { display: block; width: 100%; height: min(54vh, 480px);
      object-fit: contain; border-radius: 8px; user-select: none; -webkit-user-drag: none; }
    .noon-feedback-mark { position: absolute; box-sizing: border-box; border: 2px solid #fff;
      pointer-events: none; box-shadow: 0 0 0 2px #5238a6; background: rgb(128 102 237 / 25%); }
    .noon-feedback-mark[data-shape="point"] { width: 16px; height: 16px; border-radius: 50%;
      background: #8066ed; transform: translate(-50%, -50%); }
    .noon-feedback-dialog form { margin-top: 16px; }
    .noon-feedback-dialog label { display: block; margin-bottom: 6px; font: 600 14px/1.5 system-ui; }
    .noon-feedback-dialog textarea { position: static; display: block; box-sizing: border-box;
      width: 100%; height: auto; min-height: 90px; padding: 10px; margin-bottom: 12px;
      color: #f7f8fb; background: #080b12; border: 1px solid #3a4968; border-radius: 7px;
      font: 14px/1.5 system-ui; resize: vertical; }
    .noon-feedback-count { color: #a5afc3; font: 12px/1.5 system-ui; }
  `;
  document.head.append(style);
  // Keep the dialog inside the preview pane so it also works in fullscreen.
  button.closest(".preview-pane").append(dialog);
  button.hidden = false;
  const image = dialog.querySelector("img");
  const stage = dialog.querySelector(".noon-feedback-image");
  const mark = dialog.querySelector(".noon-feedback-mark");
  const status = dialog.querySelector(".noon-feedback-status");
  const text = dialog.querySelector("textarea");
  const form = dialog.querySelector("form");
  const submit = dialog.querySelector('[type="submit"]');
  const download = dialog.querySelector('[data-action="download"]');
  const count = dialog.querySelector(".noon-feedback-count");
  let capture = null, selection = null, drag = null, imageUrl = null, downloadUrl = null, controller = null;
  let addToSource = null;
  let generation = 0, disposed = false, timer = null, workInFlight = null;
  const update = () => {
    const size = new TextEncoder().encode(text.value).length;
    count.textContent = `${size.toLocaleString()} / 4,000 bytes`;
    submit.disabled = !capture || !selection || !text.value.trim() || text.value.includes("\0") || size > 4000;
    download.disabled = submit.disabled;
  };
  const message = (value, error = false) => {
    status.textContent = value;
    status.dataset.error = String(error);
  };
  const release = () => {
    generation++;
    controller?.abort(); controller = null;
    if (timer !== null) window.clearTimeout(timer);
    timer = null;
    image.removeAttribute("src");
    if (imageUrl !== null) window.URL.revokeObjectURL(imageUrl);
    if (downloadUrl !== null) window.URL.revokeObjectURL(downloadUrl);
    imageUrl = null; downloadUrl = null;
    capture = null; selection = null; drag = null;
    addToSource = null;
    image.hidden = true; mark.hidden = true;
    text.value = "";
    update();
  };
  function drawSelection() {
    const region = drag?.region ?? selection?.region;
    const target = region ?? selection?.point;
    mark.hidden = !target || !capture;
    if (mark.hidden) return;
    const bounds = feedbackImageBounds(image.getBoundingClientRect(), capture.frame.width, capture.frame.height);
    if (!bounds) { mark.hidden = true; return; }
    const outer = stage.getBoundingClientRect();
    mark.dataset.shape = region ? "region" : "point";
    mark.style.left = `${bounds.left - outer.left + target.x * bounds.width}px`;
    mark.style.top = `${bounds.top - outer.top + target.y * bounds.height}px`;
    mark.style.width = region ? `${region.width * bounds.width}px` : "";
    mark.style.height = region ? `${region.height * bounds.height}px` : "";
  }
  function select(next) {
    selection = next; drawSelection(); update();
    text.focus();
  }
  function imagePoint(event, clamp = false) {
    return feedbackPoint(event.clientX, event.clientY, image.getBoundingClientRect(),
      capture.frame.width, capture.frame.height, clamp);
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
  // Do not send the playground's global Ctrl/Command+Enter Run shortcut while
  // typing a review. Native dialog Escape/Tab behavior is intentionally retained.
  dialog.addEventListener("keydown", event => event.stopPropagation());
  dialog.addEventListener("close", () => { if (!dialog.open) release(); });
  dialog.addEventListener("cancel", event => { event.preventDefault(); close(); });
  dialog.querySelector('[data-action="close"]').addEventListener("click", close);
  text.addEventListener("input", update);
  const resize = typeof window.ResizeObserver === "function" ? new window.ResizeObserver(drawSelection) : null;
  resize?.observe(stage);

  async function openReview() {
    if (disposed || dialog.open) return;
    release();
    const ticket = generation;
    const activeController = new AbortController();
    controller = activeController;
    const signal = activeController.signal;
    dialog.showModal();
    message("Pausing and capturing the displayed Run…");
    let context = null;
    try {
      if (workInFlight !== null) throw new Error("The previous capture is still retiring. Try again after the preview responds.");
      context = getContext();
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
        imageUrl = window.URL.createObjectURL(new Blob([candidate.png], { type: "image/png" }));
        image.src = imageUrl;
        image.hidden = false;
        await image.decode();
        signal.throwIfAborted();
        if (image.naturalWidth !== candidate.frame.width || image.naturalHeight !== candidate.frame.height) {
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
      message(`Paused at ${candidate.frame.provenance.publishedTime.toFixed(3)} s. Drag an area or click a point.`);
      stage.focus();
      update();
    } catch (error) {
      if (disposed || ticket !== generation) return;
      release();
      message(error?.message ?? "The preview could not be captured.", true);
      update();
    } finally {
      context?.release?.();
      if (ticket === generation && timer !== null) {
        window.clearTimeout(timer); timer = null;
      }
    }
  }
  button.addEventListener("click", openReview);
  form.addEventListener("submit", event => {
    event.preventDefault();
    update();
    if (submit.disabled) return;
    try {
      addToSource(capture, selection, text.value);
      close();
    } catch (error) { message(error.message, true); }
  });
  download.addEventListener("click", () => {
    update();
    if (download.disabled) return;
    try {
      const exported = createLiveFeedbackExport(capture, selection, text.value);
      if (downloadUrl !== null) window.URL.revokeObjectURL(downloadUrl);
      downloadUrl = window.URL.createObjectURL(new Blob([`${JSON.stringify(exported)}\n`], { type: "application/json" }));
      const link = document.createElement("a");
      link.href = downloadUrl;
      link.download = `noon-feedback-${capture.frame.id}.json`;
      document.body.append(link); link.click(); link.remove();
      message("Evidence downloaded. Select Add to source to keep the comment with the scene.");
    } catch (error) { message(error.message, true); }
  });
  function close() {
    if (dialog.open) dialog.close();
    // Release immediately even if the native close event is queued.
    release();
  }
  return Object.freeze({
    close,
    dispose() {
      if (disposed) return;
      disposed = true; close(); resize?.disconnect();
      button.removeEventListener("click", openReview);
      button.hidden = true; dialog.remove(); style.remove();
    },
  });
}
