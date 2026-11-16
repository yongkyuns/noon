(() => {
  "use strict";
  const get = (id) => document.getElementById(id);
  const stage = get("frame-stage"), image = get("frame-image"), overlay = get("region-overlay");
  const commentPin = get("comment-pin");
  const markButton = get("mark-region"), clearButton = get("clear-region"), regionEnabled = get("region-enabled");
  const startInput = get("interval-start"), endInput = get("interval-end");
  const observation = get("observation"), desiredChange = get("desired-change"), preserve = get("preserve");
  const downloadButton = get("download-note"), formStatus = get("form-status"), imageStatus = get("image-status");
  const regionFields = ["x", "y", "width", "height"].map((key) => get(`region-${key}`));
  const drafts = new Map(), buttons = new Map(), encoder = new TextEncoder();
  let data, selected = null, imageReady = false, generation = 0, marking = false, drag = null, downloadUrl = null;
  let point = null, suppressClick = false;
  const numeric = (input) => input.value.trim() === "" ? NaN : Number(input.value);
  const boundedTime = (value) => Number.isFinite(value) && value >= 0 && value <= 600;
  const clamp = (value) => Math.max(0, Math.min(1, value));
  const seconds = (value) => `${new Intl.NumberFormat("en", { maximumFractionDigits: 3 }).format(value)} s`;
  const status = (element, text, state = "") => { element.textContent = text; element.dataset.state = state; };

  function initialDraft(entry) {
    const time = String(entry.frame.provenance.publishedTime);
    return { start: time, end: time, observation: "", desiredChange: "", preserve: "", point: null,
      region: false, coordinates: ["0", "0", "100", "100"] };
  }
  function saveDraft() {
    if (!selected) return;
    drafts.set(selected.sampleIndex, { start: startInput.value, end: endInput.value,
      observation: observation.value, desiredChange: desiredChange.value, preserve: preserve.value,
      point: point ? { ...point } : null,
      region: regionEnabled.checked, coordinates: regionFields.map((input) => input.value) });
  }
  function readRegion() {
    if (!regionEnabled.checked) return null;
    let [x, y, width, height] = regionFields.map((input) => numeric(input) / 100);
    // A valid edge-reaching drag can gain one ULP through the percentage
    // fields. Canonicalize only that conversion roundoff; real overflow stays
    // invalid and the shared importer retains its strict normalized bounds.
    const extent = (origin, size) => origin >= 0 && origin <= 1 &&
      size > 1 - origin && size - (1 - origin) <= 4 * Number.EPSILON ? 1 - origin : size;
    width = extent(x, width); height = extent(y, height);
    return { x, y, width, height };
  }
  function validRegion(value) {
    return value && Object.values(value).every(Number.isFinite) && value.x >= 0 && value.y >= 0 &&
      value.width > 0 && value.height > 0 && value.x + value.width <= 1 && value.y + value.height <= 1;
  }
  function fittedImage() {
    if (!imageReady || !image.naturalWidth || !image.naturalHeight) return null;
    const box = image.getBoundingClientRect();
    const scale = Math.min(box.width / image.naturalWidth, box.height / image.naturalHeight);
    const width = image.naturalWidth * scale, height = image.naturalHeight * scale;
    if (!(width > 0 && height > 0)) return null;
    return { left: box.left + (box.width - width) / 2, top: box.top + (box.height - height) / 2, width, height };
  }
  function drawOverlay(value = readRegion()) {
    const bounds = fittedImage();
    const box = stage.getBoundingClientRect();
    overlay.hidden = !bounds || !validRegion(value) || Boolean(point);
    if (!overlay.hidden) {
      overlay.style.left = `${bounds.left - box.left - stage.clientLeft + value.x * bounds.width}px`;
      overlay.style.top = `${bounds.top - box.top - stage.clientTop + value.y * bounds.height}px`;
      overlay.style.width = `${value.width * bounds.width}px`;
      overlay.style.height = `${value.height * bounds.height}px`;
    }
    commentPin.hidden = !bounds || !point || marking;
    if (!commentPin.hidden) {
      commentPin.style.left = `${bounds.left - box.left - stage.clientLeft + point.x * bounds.width}px`;
      commentPin.style.top = `${bounds.top - box.top - stage.clientTop + point.y * bounds.height}px`;
    }
  }
  function setMarking(value) {
    marking = value && imageReady;
    stage.classList.toggle("is-marking", marking);
    markButton.setAttribute("aria-pressed", String(marking));
    markButton.textContent = marking ? "Cancel drawing" : "Draw region";
  }
  function updateRegion() {
    get("region-grid").hidden = !regionEnabled.checked;
    for (const input of regionFields) input.disabled = !imageReady || !regionEnabled.checked;
    regionEnabled.disabled = !imageReady;
    markButton.disabled = !imageReady;
    clearButton.disabled = !regionEnabled.checked && !marking && !point;
    const value = readRegion();
    for (const input of regionFields) input.setAttribute("aria-invalid", String(regionEnabled.checked && !validRegion(value)));
    if (marking) status(get("region-status"), "Drag on the image to mark an area. Escape cancels drawing.");
    else if (point) status(get("region-status"), "Comment pin placed. Write your note to the right; click again to move it.");
    else if (!regionEnabled.checked) status(get("region-status"), "Click the image to place a comment pin, or leave it unmarked for whole-frame feedback.");
    else if (!validRegion(value)) status(get("region-status"), "Use a positive width and height that fit within the image.", "error");
    else status(get("region-status"), "Region marked. Adjust the percentages to refine it, or draw again.");
    drawOverlay();
  }
  function cancelDrag() {
    const prior = drag;
    drag = null;
    if (prior && stage.hasPointerCapture(prior.id)) stage.releasePointerCapture(prior.id);
    setMarking(false);
    updateRegion();
  }
  function feedback() {
    const interval = { start: numeric(startInput), end: numeric(endInput) };
    const value = { interval, observation: observation.value,
      desiredChange: desiredChange.value.trim() || "Propose a focused correction for this comment." };
    if (point) value.point = { ...point };
    else if (regionEnabled.checked) value.region = readRegion();
    if (preserve.value.trim() !== "") value.preserve = preserve.value;
    return value;
  }
  function validate() {
    let message = "", state = "";
    const value = feedback(), time = selected?.frame.provenance.publishedTime;
    const intervalValid = boundedTime(value.interval.start) && boundedTime(value.interval.end) &&
      value.interval.start <= value.interval.end && value.interval.start <= time && value.interval.end >= time;
    for (const input of [startInput, endInput]) input.setAttribute("aria-invalid", String(!intervalValid));
    if (!selected) message = "This review has no selected capture.";
    else if (!imageReady) message = "Wait for the captured image before downloading a note.";
    else if (!intervalValid) { message = "Set an interval from 0 to 600 seconds that includes the captured moment."; state = "error"; }
    else if (regionEnabled.checked && !validRegion(value.region)) { message = "Adjust the marked region so it fits within the image."; state = "error"; }
    for (const [input, countId, required] of [[observation, "observation-count", true],
      [desiredChange, "desired-change-count", false], [preserve, "preserve-count", false]]) {
      const count = encoder.encode(input.value).length;
      const invalid = count > 4000 || input.value.includes("\0");
      const counter = get(countId);
      counter.textContent = `${count.toLocaleString("en")} / 4,000 bytes`;
      counter.classList.toggle("over-limit", invalid);
      input.setAttribute("aria-invalid", String(invalid));
      if (!message && invalid) { message = "Shorten the highlighted text to 4,000 bytes and remove any NUL characters."; state = "error"; }
      if (!message && required && input.value.trim() === "") message = "Describe the problem and the correction to prepare your note.";
    }
    downloadButton.disabled = Boolean(message) || Boolean(drag) || marking;
    if (!message && marking) message = "Finish or cancel drawing before downloading your note.";
    if (!message) { message = "Ready to download. Your note will stay attached to this capture."; state = "ready"; }
    status(formStatus, message, state);
    return !downloadButton.disabled;
  }
  function describe(entry) {
    const { frame } = entry, p = frame.provenance;
    const rows = [
      ["Source", data.source.sha256], ["Run", p.sessionId], ["Artifact", frame.id],
      ["Requested time", `${p.requestedTime} s`], ["Published time", `${p.publishedTime} s`],
      ["Image", `${frame.width} × ${frame.height} · ${frame.byteLength.toLocaleString("en")} bytes`],
      ["Image hash", frame.sha256], ["Backend", p.backend], ["Build", p.build.buildId],
      ["Engine", p.build.engineRevision], ["WASM", p.build.wasmSha256], ["Worker", p.build.workerSha256],
      ["Scene revision", p.sceneRevision], ["Frame revision", p.frameRevision],
    ];
    const list = get("provenance");
    list.replaceChildren();
    for (const [label, value] of rows) {
      const term = document.createElement("dt"), description = document.createElement("dd");
      term.textContent = label; description.textContent = value === null ? "Unknown" : String(value);
      list.append(term, description);
    }
  }
  async function select(entry) {
    saveDraft(); cancelDrag();
    selected = entry; imageReady = false;
    const ticket = ++generation, draft = drafts.get(entry.sampleIndex) ?? initialDraft(entry);
    startInput.value = draft.start; endInput.value = draft.end;
    observation.value = draft.observation; desiredChange.value = draft.desiredChange; preserve.value = draft.preserve;
    point = draft.point ? { ...draft.point } : null;
    regionEnabled.checked = draft.region && !point;
    regionFields.forEach((input, index) => { input.value = draft.coordinates[index]; });
    for (const [index, button] of buttons) button.setAttribute("aria-pressed", String(index === entry.sampleIndex));
    get("frame-position").textContent = `${data.frames.indexOf(entry) + 1} of ${data.frames.length}`;
    get("capture-time").textContent = `Captured at ${seconds(entry.frame.provenance.publishedTime)}`;
    image.alt = `Captured frame ${data.frames.indexOf(entry) + 1} at ${seconds(entry.frame.provenance.publishedTime)}`;
    stage.setAttribute("aria-busy", "true");
    status(imageStatus, "Loading captured image…", "loading");
    describe(entry); updateRegion(); validate();
    image.src = entry.filename;
    try {
      await image.decode();
      if (ticket !== generation) return;
      if (image.naturalWidth !== entry.frame.width || image.naturalHeight !== entry.frame.height) {
        throw new Error("image dimensions differ from capture");
      }
      imageReady = true;
      status(imageStatus, "Captured image ready", "ready");
    } catch {
      if (ticket !== generation) return;
      status(imageStatus, "Image unavailable or changed. Regenerate this review folder.", "error");
    }
    stage.setAttribute("aria-busy", "false");
    updateRegion(); validate();
  }
  function coordinates(event, bounds) {
    return { x: clamp((event.clientX - bounds.left) / bounds.width), y: clamp((event.clientY - bounds.top) / bounds.height) };
  }
  function dragRegion(event) {
    const bounds = fittedImage();
    if (!drag || !bounds) return null;
    const end = coordinates(event, bounds);
    return { x: Math.min(drag.start.x, end.x), y: Math.min(drag.start.y, end.y),
      width: Math.abs(drag.start.x - end.x), height: Math.abs(drag.start.y - end.y) };
  }

  // Ordinary click-to-comment is the primary interaction; drawing is optional.
  function placePin(position) {
    point = position;
    regionEnabled.checked = false;
    updateRegion(); saveDraft(); validate();
    observation.focus({ preventScroll: true });
  }
  stage.addEventListener("click", (event) => {
    if (suppressClick) { suppressClick = false; return; }
    if (!imageReady || marking || drag || event.button !== 0) return;
    const bounds = fittedImage();
    if (!bounds || event.clientX < bounds.left || event.clientX > bounds.left + bounds.width ||
        event.clientY < bounds.top || event.clientY > bounds.top + bounds.height) return;
    placePin(coordinates(event, bounds));
  });
  stage.addEventListener("keydown", (event) => {
    if (event.target === stage && !marking && imageReady && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      placePin(point ?? { x: 0.5, y: 0.5 });
    }
  });
  stage.addEventListener("pointerdown", (event) => {
    if (!marking || !imageReady || drag || !event.isPrimary || event.button !== 0) return;
    const bounds = fittedImage();
    if (!bounds || event.clientX < bounds.left || event.clientX > bounds.left + bounds.width ||
        event.clientY < bounds.top || event.clientY > bounds.top + bounds.height) return;
    event.preventDefault(); stage.focus({ preventScroll: true });
    drag = { id: event.pointerId, start: coordinates(event, bounds) };
    suppressClick = true; // A successful drag's synthetic click must not replace the rectangle with a pin.
    stage.setPointerCapture(event.pointerId);
    overlay.hidden = true; validate();
  });
  stage.addEventListener("pointermove", (event) => {
    if (drag?.id !== event.pointerId) return;
    event.preventDefault(); drawOverlay(dragRegion(event));
  });
  stage.addEventListener("pointerup", (event) => {
    if (drag?.id !== event.pointerId) return;
    const value = dragRegion(event), id = drag.id;
    drag = null;
    if (stage.hasPointerCapture(id)) stage.releasePointerCapture(id);
    if (validRegion(value)) {
      point = null;
      regionEnabled.checked = true;
      regionFields.forEach((input, index) => { input.value = String([value.x, value.y, value.width, value.height][index] * 100); });
    }
    setMarking(false); updateRegion(); saveDraft(); validate();
  });
  for (const type of ["pointercancel", "lostpointercapture"]) stage.addEventListener(type, (event) => {
    if (drag?.id === event.pointerId) { cancelDrag(); validate(); }
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && (marking || drag)) { event.preventDefault(); cancelDrag(); validate(); }
  });
  markButton.addEventListener("click", () => {
    if (marking) cancelDrag();
    else { setMarking(true); updateRegion(); }
    validate();
  });
  clearButton.addEventListener("click", () => {
    cancelDrag(); point = null; regionEnabled.checked = false;
    updateRegion(); saveDraft(); validate();
  });
  regionEnabled.addEventListener("change", () => {
    cancelDrag();
    if (regionEnabled.checked) point = null;
    if (regionEnabled.checked && !validRegion(readRegion())) {
      regionFields.forEach((input, index) => { input.value = index < 2 ? "0" : "100"; });
    }
    updateRegion(); saveDraft(); validate();
  });
  for (const input of regionFields) input.addEventListener("input", () => {
    cancelDrag(); updateRegion(); saveDraft(); validate();
  });
  get("feedback-form").addEventListener("input", () => { saveDraft(); validate(); });
  get("feedback-form").addEventListener("submit", (event) => {
    event.preventDefault();
    if (!validate()) return;
    const note = { kind: "noon-frame-feedback-note", schemaVersion: 1, sourceSha256: data.source.sha256,
      sampleIndex: selected.sampleIndex, frameIdentity: selected.identity, feedback: feedback() };
    if (downloadUrl) URL.revokeObjectURL(downloadUrl);
    downloadUrl = URL.createObjectURL(new Blob([`${JSON.stringify(note, null, 2)}\n`], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = downloadUrl; link.download = "noon-feedback-note.json";
    document.body.append(link); link.click(); link.remove();
    status(formStatus, "Note prepared for download. Keep it with this review folder.", "downloaded");
  });
  const resize = () => {
    if (drag) { cancelDrag(); validate(); }
    else drawOverlay();
  };
  window.addEventListener("resize", resize);
  if (typeof ResizeObserver === "function") new ResizeObserver(resize).observe(stage);
  window.addEventListener("pagehide", () => { if (downloadUrl) URL.revokeObjectURL(downloadUrl); });

  try {
    data = JSON.parse(get("noon-review-data").textContent);
    if (data?.kind !== "noon-frame-review" || data.schemaVersion !== 1 || !/^[a-f0-9]{64}$/.test(data.source?.sha256) ||
        typeof data.source?.name !== "string" || !Array.isArray(data.frames) || !data.frames.length || data.frames.length > 32) {
      throw new Error("invalid review data");
    }
    const indexes = new Set();
    for (const entry of data.frames) {
      if (!Number.isInteger(entry.sampleIndex) || entry.sampleIndex < 0 || entry.sampleIndex > 31 || indexes.has(entry.sampleIndex) ||
          !/^[a-f0-9]{64}$/.test(entry.identity) || !/^frame-[0-9]{3}\.png$/.test(entry.filename) ||
          !boundedTime(entry.frame?.provenance?.publishedTime) || entry.frame.provenance.sourceSha256 !== data.source.sha256 ||
          !Number.isSafeInteger(entry.frame.width) || !Number.isSafeInteger(entry.frame.height) || entry.frame.width < 1 || entry.frame.height < 1) {
        throw new Error("invalid captured frame");
      }
      indexes.add(entry.sampleIndex);
      const button = document.createElement("button"), number = document.createElement("span"), time = document.createElement("span");
      button.type = "button"; button.className = "frame-button"; button.dataset.sampleIndex = String(entry.sampleIndex);
      button.setAttribute("aria-pressed", "false");
      button.setAttribute("aria-label", `Frame ${data.frames.indexOf(entry) + 1}, captured at ${entry.frame.provenance.publishedTime} seconds`);
      number.className = "frame-number"; number.textContent = `Frame ${data.frames.indexOf(entry) + 1}`;
      time.className = "frame-time"; time.textContent = seconds(entry.frame.provenance.publishedTime);
      button.append(number, time); button.addEventListener("click", () => { if (selected !== entry) void select(entry); });
      buttons.set(entry.sampleIndex, button); get("frame-list").append(button);
    }
    get("source-name").textContent = data.source.name;
    void select(data.frames[0]);
  } catch {
    status(imageStatus, "This review could not be loaded. Regenerate the review folder.", "error");
    status(formStatus, "The review data is unavailable. No note can be downloaded.", "error");
    stage.setAttribute("aria-busy", "false"); downloadButton.disabled = true;
  }
})();
