const status = document.querySelector("#status");

if (!(status instanceof HTMLOutputElement)) {
  throw new Error("Noon live authoring requires the playground status surface");
}

void startLiveAuthoring();

async function startLiveAuthoring() {
  let disposed = false;
  const onPageHide = () => { disposed = true; };
  // Navigation may happen while main.js is still loading its initial source.
  // A later module completion must not start work in a retired page.
  window.addEventListener("pagehide", onPageHide, { once: true });

  try {
    const gallery = await waitForGalleryApi();
    if (disposed) return;

    // Cross a real paint boundary after the loaded source/gallery is ready, then immediately
    // warm the persistent Pyodide + execution session. Subsequent edits are handled by main.js: cancel the old run
    // immediately, then debounce only the next source submission.
    status.dataset.liveAuthoring = "preloading";
    await afterInitialPaint();
    if (disposed) return;
    performance.mark("noon-live-authoring-preload-start");
    await gallery.run();
    if (!disposed) {
      status.dataset.liveAuthoring =
        status.dataset.authoringWarmup === "failed" ? "error" : "ready";
    }
  } catch (error) {
    if (!disposed) {
      status.dataset.liveAuthoring = "error";
      console.warn("Noon live authoring preload failed", error);
    }
  } finally {
    window.removeEventListener("pagehide", onPageHide);
  }
}

async function waitForGalleryApi() {
  // Join the same module evaluation as index.html, including main.js's awaited
  // manifest/source loading. A slow but valid startup is not a failed startup.
  // Module caching prevents a second execution owner or another gallery boot.
  await import("./main.js");
  const gallery = window.__noonExampleGallery;
  if (
    gallery &&
    typeof gallery.run === "function" &&
    typeof gallery.selectedExampleId === "string"
  ) {
    return gallery;
  }
  throw new Error("Noon playground API unavailable after initialization");
}

function afterInitialPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}
