// One UI-only image of the last successful run. No retained Scene, renderer,
// source text, callbacks, clock or worker is kept alive by this view.
export function createPlaygroundPreview(frame) {
  const document = frame.ownerDocument;
  const overlay = document.createElement("div");
  overlay.id = "last-successful-preview";
  overlay.hidden = true;
  overlay.setAttribute("aria-label", "Previous successful preview");
  const label = document.createElement("p");
  label.setAttribute("role", "status");
  overlay.append(label);
  frame.append(overlay);
  const style = document.createElement("style");
  style.textContent = `
    #last-successful-preview { position: absolute; inset: 0; z-index: 2;
      overflow: hidden; border-radius: .8rem; background: #090c13; }
    #last-successful-preview[hidden] { display: none; }
    #last-successful-preview img { display: block; width: 100%; height: 100%; object-fit: contain; }
    #last-successful-preview p { position: absolute; bottom: 0; inset-inline: 0; margin: 0;
      padding: .45rem .65rem; background: rgb(9 12 19 / 90%); color: #e5e7eb;
      font: .75rem/1.4 system-ui; overflow-wrap: anywhere; }
  `;
  document.head.append(style);
  let epoch = 0;
  let successful = null;
  let cached = null;
  let pending = null;
  let stale = false;
  let reason = "Edits not applied";
  let disposed = false;
  let captureInFlight = false;
  const urls = document.defaultView.URL;
  const releaseUrl = (entry) => {
    entry.image.removeAttribute("src");
    urls.revokeObjectURL(entry.url);
  };
  const release = (entry) => {
    if (entry === null || entry.released) return;
    entry.released = true;
    entry.image.remove();
    // Detach immediately, but let the active decoder retain its request. Revoking
    // its URL or clearing src here can emit WebKitBlobResource load errors.
    if (!entry.decoding) releaseUrl(entry);
  };
  const update = () => {
    overlay.hidden = !stale || cached === null;
    label.textContent = `Previous successful preview · ${reason}`;
    if (cached !== null) {
      overlay.dataset.exampleId = cached.identity.exampleId;
      overlay.dataset.runGeneration = String(cached.identity.runGeneration);
    }
  };
  const clear = () => {
    epoch += 1;
    successful = null;
    release(pending); pending = null;
    release(cached); cached = null;
    stale = false;
    delete overlay.dataset.exampleId;
    delete overlay.dataset.runGeneration;
    update();
  };
  const remember = async (identity, canvas, ticket) => {
    captureInFlight = true;
    let candidate = null;
    let copy = null;
    try {
      // Noon renders through an OffscreenCanvas. Its HTML placeholder keeps the
      // last displayed bitmap. Copy it synchronously, before the caller hides or
      // replaces the placeholder; encoding/decoding never blocks cancellation.
      const { width, height } = canvas;
      if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width <= 0 || height <= 0) return;
      const scale = Math.min(1, 1536 / width, 1536 / height, Math.sqrt(1048576 / width / height));
      copy = document.createElement("canvas");
      copy.width = Math.max(1, Math.floor(width * scale));
      copy.height = Math.max(1, Math.floor(height * scale));
      const context = copy.getContext("2d");
      if (context === null) return;
      context.drawImage(canvas, 0, 0, copy.width, copy.height);
      const blob = await new Promise(resolve => copy.toBlob(resolve, "image/png"));
      if (disposed || ticket !== epoch || blob === null || blob.size > 8 * 1024 * 1024) return;
      const image = document.createElement("img");
      image.alt = "Frozen image from a successful run; current edits are not shown";
      candidate = { image, url: urls.createObjectURL(blob), identity, released: false, decoding: false };
      pending = candidate;
      image.src = candidate.url;
      candidate.decoding = true;
      try {
        await image.decode();
      } finally {
        candidate.decoding = false;
        if (candidate.released) releaseUrl(candidate);
      }
      if (disposed || ticket !== epoch || candidate.released) return;
      release(cached);
      cached = candidate;
      pending = null;
      overlay.prepend(image);
      update();
    } catch {
      // Optional capture failure never replaces a Python error or breaks Run.
      // Keep the older labelled image, or the existing no-preview behavior.
    } finally {
      captureInFlight = false;
      if (candidate !== cached) release(candidate);
      if (pending === candidate) pending = null;
      if (copy !== null) { copy.width = 1; copy.height = 1; }
    }
  };
  return Object.freeze({
    accept(canvas, identity) {
      if (disposed) return;
      epoch += 1;
      release(pending); pending = null;
      successful = { canvas, identity: { ...identity } };
      stale = false; update();
    },
    hold(canvas, message = "Edits not applied") {
      if (disposed) return;
      stale = true; reason = message;
      if (successful !== null) {
        const prior = successful;
        successful = null; // Repeated keystrokes never recapture or queue work.
        if (prior.canvas === canvas && !captureInFlight) {
          const ticket = ++epoch;
          release(pending); pending = null;
          void remember(prior.identity, canvas, ticket);
        }
      }
      update();
    },
    showLive() { if (!disposed) { stale = false; update(); } },
    clear,
    dispose() { clear(); disposed = true; overlay.remove(); style.remove(); },
  });
}
