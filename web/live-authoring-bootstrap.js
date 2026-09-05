import { LatestSourceRunner } from "./latest-source-runner.js";

const API_POLL_MS = 16;
const API_WAIT_TIMEOUT_MS = 15_000;

const editor = document.querySelector("#python-scene-source");
const status = document.querySelector("#status");

if (!(editor instanceof HTMLTextAreaElement) || !(status instanceof HTMLOutputElement)) {
  throw new Error("Noon live authoring requires the playground editor and status surfaces");
}

void startLiveAuthoring();

async function startLiveAuthoring() {
  let runner = null;
  let disposed = false;

  try {
    const gallery = await waitForGalleryApi();
    runner = new LatestSourceRunner({
      run: () => gallery.run(),
      runInFlight: () => gallery.runInFlight,
      currentExampleId: () => gallery.selectedExampleId,
    });

    const requestLatestSource = ({ immediate = false } = {}) => {
      const exampleId = gallery.selectedExampleId;
      if (typeof exampleId !== "string" || exampleId === "") return null;
      return runner.request(exampleId, { immediate });
    };

    const onInput = () => {
      requestLatestSource();
    };
    editor.addEventListener("input", onInput);

    window.addEventListener(
      "pagehide",
      () => {
        disposed = true;
        editor.removeEventListener("input", onInput);
        runner?.dispose();
        runner = null;
      },
      { once: true },
    );

    // Cross a real paint boundary after the loaded source/gallery is ready, then immediately
    // warm the persistent Pyodide + execution session. One rAF is not enough here because its
    // promise continuation can still run before that frame is presented; the second rAF proves
    // the browser had a rendering opportunity between callbacks.
    status.dataset.liveAuthoring = "preloading";
    await afterInitialPaint();
    if (disposed) return;
    await requestLatestSource({ immediate: true });
    if (!disposed) {
      status.dataset.liveAuthoring =
        status.dataset.authoringWarmup === "failed" ? "error" : "ready";
    }
  } catch (error) {
    if (!disposed) {
      status.dataset.liveAuthoring = "error";
      console.warn("Noon live authoring preload failed", error);
    }
    runner?.dispose();
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
