const INSTALL_MARKER = "noonExampleBrowserInstalled";
const STYLE_ID = "noon-example-browser-style";

if (typeof document !== "undefined") {
  installExampleBrowser(document);
}

export function installExampleBrowser(documentLike = globalThis.document) {
  if (
    !documentLike?.documentElement?.dataset ||
    typeof documentLike.createElement !== "function" ||
    typeof MutationObserver !== "function"
  ) {
    return false;
  }
  if (documentLike.documentElement.dataset[INSTALL_MARKER] === "true") {
    return false;
  }
  documentLike.documentElement.dataset[INSTALL_MARKER] = "true";

  installStyles(documentLike);

  const tryInstall = () => {
    const gallery = documentLike.querySelector(".example-gallery");
    const topbar = documentLike.querySelector(".topbar");
    const workspace = documentLike.querySelector(".workspace");
    if (!(gallery instanceof HTMLElement) || !(topbar instanceof HTMLElement) || !(workspace instanceof HTMLElement)) {
      return false;
    }
    if (gallery.closest(".example-browser-layer") !== null) {
      return true;
    }
    enhanceGallery(documentLike, gallery, topbar, workspace);
    return true;
  };

  if (tryInstall()) return true;

  const observer = new MutationObserver(() => {
    if (tryInstall()) observer.disconnect();
  });
  observer.observe(documentLike.documentElement, { childList: true, subtree: true });
  return true;
}

function enhanceGallery(documentLike, gallery, topbar, workspace) {
  gallery.id = gallery.id || "example-browser-dialog";
  gallery.setAttribute("role", "dialog");
  gallery.setAttribute("aria-modal", "true");
  gallery.setAttribute("aria-label", "Examples");

  const layer = documentLike.createElement("div");
  layer.className = "example-browser-layer";
  layer.hidden = true;
  layer.setAttribute("aria-hidden", "true");

  const backdrop = documentLike.createElement("div");
  backdrop.className = "example-browser-backdrop";
  backdrop.setAttribute("aria-hidden", "true");

  const trigger = documentLike.createElement("button");
  trigger.id = "example-browser-trigger";
  trigger.type = "button";
  trigger.className = "example-browser-trigger";
  trigger.setAttribute("aria-haspopup", "dialog");
  trigger.setAttribute("aria-controls", gallery.id);
  trigger.setAttribute("aria-expanded", "false");
  trigger.innerHTML = `<span class="example-browser-trigger-label">Example</span><span class="example-browser-trigger-value">Choose example</span><span class="example-browser-trigger-chevron" aria-hidden="true">⌄</span>`;

  const status = topbar.querySelector("#status");
  topbar.insertBefore(trigger, status ?? null);

  const galleryHead = gallery.querySelector(".gallery-head");
  const galleryTitle = gallery.querySelector(".gallery-title strong");
  const gallerySubtitle = gallery.querySelector(".gallery-title span");
  const gallerySearch = gallery.querySelector('input[type="search"]');
  const galleryControls = gallery.querySelector(".gallery-controls");
  const paritySelect = gallery.querySelector('select[aria-label="Filter examples by parity status"]');

  if (galleryTitle) galleryTitle.textContent = "Examples";
  if (gallerySubtitle) gallerySubtitle.hidden = true;

  if (galleryHead instanceof HTMLElement) {
    const closeButton = documentLike.createElement("button");
    closeButton.type = "button";
    closeButton.className = "example-browser-close";
    closeButton.setAttribute("aria-label", "Close examples");
    closeButton.textContent = "×";
    galleryHead.append(closeButton);
    closeButton.addEventListener("click", () => closeBrowser({ restoreFocus: true }));
  }

  if (galleryControls instanceof HTMLElement && paritySelect instanceof HTMLSelectElement) {
    const moreFilters = documentLike.createElement("details");
    moreFilters.className = "example-browser-more-filters";
    const summary = documentLike.createElement("summary");
    summary.textContent = "More filters";
    const content = documentLike.createElement("div");
    content.className = "example-browser-more-filters-content";
    content.append(paritySelect);
    moreFilters.append(summary, content);
    galleryControls.append(moreFilters);
  }

  layer.append(backdrop, gallery);
  documentLike.body.append(layer);
  workspace.tabIndex = -1;

  const updateTriggerLabel = () => {
    const selected = gallery.querySelector('.example-card[aria-selected="true"] .example-card-title');
    const value = trigger.querySelector(".example-browser-trigger-value");
    if (value) value.textContent = selected?.textContent?.trim() || "Choose example";
  };

  const selectionObserver = new MutationObserver(updateTriggerLabel);
  selectionObserver.observe(gallery, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["aria-selected"],
  });
  updateTriggerLabel();

  let previouslyFocused = null;

  function openBrowser() {
    if (!layer.hidden) return;
    previouslyFocused = documentLike.activeElement instanceof HTMLElement ? documentLike.activeElement : trigger;
    layer.hidden = false;
    layer.setAttribute("aria-hidden", "false");
    trigger.setAttribute("aria-expanded", "true");
    documentLike.body.classList.add("example-browser-open");
    updateTriggerLabel();
    requestAnimationFrame(() => {
      if (gallerySearch instanceof HTMLElement) {
        gallerySearch.focus();
      } else {
        gallery.querySelector("button, input, select, summary")?.focus();
      }
    });
  }

  function closeBrowser({ restoreFocus = true, focusWorkspace = false } = {}) {
    if (layer.hidden) return;
    layer.hidden = true;
    layer.setAttribute("aria-hidden", "true");
    trigger.setAttribute("aria-expanded", "false");
    documentLike.body.classList.remove("example-browser-open");
    if (focusWorkspace) {
      requestAnimationFrame(() => workspace.focus());
    } else if (restoreFocus) {
      requestAnimationFrame(() => (previouslyFocused ?? trigger).focus());
    }
  }

  trigger.addEventListener("click", () => {
    if (layer.hidden) openBrowser();
    else closeBrowser({ restoreFocus: true });
  });
  backdrop.addEventListener("click", () => closeBrowser({ restoreFocus: true }));

  gallery.addEventListener("click", (event) => {
    if (event.target instanceof Element && event.target.closest(".example-card")) {
      closeBrowser({ restoreFocus: false, focusWorkspace: true });
    }
  });

  layer.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeBrowser({ restoreFocus: true });
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = [...gallery.querySelectorAll(
      'button:not([disabled]), input:not([disabled]), select:not([disabled]), summary, [href], [tabindex]:not([tabindex="-1"])',
    )].filter(
      (element) =>
        element instanceof HTMLElement &&
        !element.hidden &&
        element.getClientRects().length > 0,
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && documentLike.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && documentLike.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  });

  globalThis.__noonExampleBrowser = {
    show: openBrowser,
    hide: closeBrowser,
    get isOpen() {
      return !layer.hidden;
    },
  };
}

function installStyles(documentLike) {
  if (documentLike.getElementById(STYLE_ID)) return;
  const style = documentLike.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
    body.example-browser-open { overflow: hidden; }

    .example-browser-trigger {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr) auto;
      align-items: center;
      gap: 0.42rem;
      min-width: min(22rem, 38vw);
      max-width: 32rem;
      border: 1px solid var(--border);
      border-radius: 0.62rem;
      padding: 0.4rem 0.6rem;
      background: #101621;
      color: #dce3ef;
      cursor: pointer;
      text-align: left;
    }
    .example-browser-trigger:hover { border-color: var(--border-strong); background: #141b28; }
    .example-browser-trigger:focus-visible {
      outline: 2px solid var(--accent-strong);
      outline-offset: 2px;
    }
    .example-browser-trigger-label {
      color: var(--muted-2);
      font-size: 0.64rem;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }
    .example-browser-trigger-value {
      overflow: hidden;
      font-size: 0.74rem;
      font-weight: 700;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .example-browser-trigger-chevron { color: var(--muted); }

    .example-browser-layer {
      position: fixed;
      inset: 0;
      z-index: 1000;
      display: grid;
      place-items: center;
      padding: clamp(0.75rem, 3vw, 2rem);
    }
    .example-browser-backdrop {
      position: absolute;
      inset: 0;
      background: rgb(3 5 9 / 76%);
      backdrop-filter: blur(10px);
    }
    .example-browser-layer .example-gallery {
      position: relative;
      z-index: 1;
      display: flex;
      width: min(72rem, 100%);
      max-height: min(86vh, 54rem);
      margin: 0;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid var(--border-strong);
      border-radius: 1rem;
      background: #0b1019;
      box-shadow: 0 2rem 7rem rgb(0 0 0 / 55%);
    }
    .example-browser-layer .gallery-head {
      flex: none;
      padding: 0.8rem 0.9rem;
      background: #0d121c;
    }
    .example-browser-close {
      appearance: none;
      width: 2rem;
      height: 2rem;
      flex: none;
      border: 1px solid var(--border);
      border-radius: 0.55rem;
      background: #141a26;
      color: #cbd3e1;
      cursor: pointer;
      font-size: 1.1rem;
      line-height: 1;
    }
    .example-browser-layer .gallery-controls { flex-wrap: wrap; }
    .example-browser-layer .gallery-grid {
      min-height: 0;
      flex: 1;
      overflow: auto;
      align-content: start;
      grid-auto-rows: max-content;
      grid-template-columns: repeat(auto-fill, minmax(11.5rem, 1fr));
      overscroll-behavior: contain;
    }
    .example-browser-layer .example-card {
      content-visibility: visible;
      contain-intrinsic-size: none;
    }
    .example-browser-layer .example-parity { display: none; }
    .example-browser-layer .example-card-meta { justify-content: flex-start; }
    .example-browser-layer .gallery-pager { flex: none; }
    .example-browser-more-filters { position: relative; }
    .example-browser-more-filters > summary {
      list-style: none;
      border: 1px solid #303d58;
      border-radius: 0.58rem;
      padding: 0.48rem 0.62rem;
      background: #111722;
      color: #aeb9cc;
      cursor: pointer;
      font: 0.72rem ui-monospace, SFMono-Regular, Menlo, monospace;
    }
    .example-browser-more-filters > summary::-webkit-details-marker { display: none; }
    .example-browser-more-filters-content {
      position: absolute;
      top: calc(100% + 0.4rem);
      right: 0;
      z-index: 3;
      min-width: 12rem;
      padding: 0.45rem;
      border: 1px solid var(--border);
      border-radius: 0.65rem;
      background: #0d131e;
      box-shadow: 0 1rem 2.5rem rgb(0 0 0 / 42%);
    }
    .example-browser-more-filters-content select { width: 100%; }

    @media (max-width: 68rem) {
      .example-browser-trigger { min-width: 0; max-width: 42vw; }
      .example-browser-trigger-label { display: none; }
      .example-browser-layer .gallery-controls { display: flex; }
      .example-browser-layer .gallery-controls input { flex: 1 1 13rem; width: auto; }
    }

    @media (max-width: 44rem) {
      .topbar { gap: 0.45rem; }
      .topbar .brand h1 { display: none; }
      .example-browser-trigger {
        min-width: 0;
        max-width: none;
        flex: 1;
      }
      .example-browser-layer { padding: 0; }
      .example-browser-layer .example-gallery {
        width: 100%;
        height: 100dvh;
        max-height: none;
        border: 0;
        border-radius: 0;
      }
      .example-browser-layer .gallery-head { align-items: stretch; }
      .example-browser-layer .gallery-controls {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
      }
      .example-browser-layer .gallery-controls input { grid-column: 1 / -1; width: 100%; }
      .example-browser-layer .gallery-grid {
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 0.55rem;
        padding: 0.6rem;
      }
      .example-browser-more-filters-content {
        position: fixed;
        right: 0.6rem;
        left: 0.6rem;
      }
    }

    @media (max-width: 28rem) {
      .example-browser-layer .gallery-grid { grid-template-columns: 1fr; }
    }
  `;
  documentLike.head?.append(style);
}
