const API_POLL_MS = 16;
const API_WAIT_TIMEOUT_MS = 15_000;

const status = document.querySelector("#status");

if (!(status instanceof HTMLOutputElement)) {
  throw new Error("Noon live authoring requires the playground status surface");
}

void startLiveAuthoring();

async function startLiveAuthoring() {
  let disposed = false;

  try {
    const gallery = await waitForGalleryApi();
    window.addEventListener(
      "pagehide",
      () => {
        disposed = true;
      },
      { once: true },
    );

    // Cross a real paint boundary after the loaded source/gallery is ready, then immediately
    // warm the persistent Pyodide + execution session. Editing after this point is deliberately
    // inert with respect to execution: the preview keeps running until explicit Run.
    status.dataset.liveAuthoring = "preloading";
    await afterInitialPaint();
    if (disposed) return;
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
  }
}

async function waitForGalleryApi() {
  const startedAt = performance.now();
  while (performance.now() - startedAt < API_WAIT_TIMEOUT_MS) {
    const gallery = window.__noonExampleGallery;
    if (
      gallery &&
      typeof gallery.run === "function" &&
      typeof gallery.selectedExampleId === "string"
    ) {
      return gallery;
    }
    await delay(API_POLL_MS);
  }
  throw new Error("Noon playground API did not become ready for live authoring");
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function afterInitialPaint() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}
