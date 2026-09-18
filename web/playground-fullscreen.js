// Fullscreen changes only presentation. The existing canvas ResizeObserver owns
// surface resizing; no scene, player or authored state is recreated here.
export function installPreviewFullscreen(preview, button, document = globalThis.document) {
  let pending = false;
  let disposed = false;
  const supported = typeof preview.requestFullscreen === "function" &&
    typeof document.exitFullscreen === "function" && document.fullscreenEnabled !== false;

  function sync() {
    const active = document.fullscreenElement === preview;
    button.textContent = active ? "Exit fullscreen" : "Fullscreen";
    button.setAttribute("aria-pressed", String(active));
    button.setAttribute("aria-label", active ? "Exit preview fullscreen" : "Show preview fullscreen");
    button.disabled = !supported || pending;
    button.title = supported ? "Fullscreen preview · Esc to exit" : "Fullscreen is unavailable in this browser";
  }

  async function toggle() {
    if (disposed || pending || !supported) return;
    pending = true;
    sync();
    try {
      // Invoke within the click's user activation, before any await.
      if (document.fullscreenElement === preview) await document.exitFullscreen();
      else await preview.requestFullscreen();
    } catch (error) {
      button.title = `Fullscreen unavailable: ${error.message ?? error}`;
    } finally {
      pending = false;
      if (!disposed) {
        // Keep a rejected request's explanation until the next real change.
        const errorTitle = button.title;
        sync();
        if (errorTitle.startsWith("Fullscreen unavailable:")) button.title = errorTitle;
      }
    }
  }

  button.addEventListener("click", toggle);
  document.addEventListener("fullscreenchange", sync);
  sync();
  return () => {
    disposed = true;
    button.removeEventListener("click", toggle);
    document.removeEventListener("fullscreenchange", sync);
  };
}
