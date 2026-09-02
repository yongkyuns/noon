// Start the presentation-only editor enhancement as soon as the playground DOM is ready.
// The Python authoring/runtime clients remain independent and still start only on Run.
// Ruff's WASM linter remains lazy until the first real editor input.
void import("./python-editor.js").catch((error) => {
  console.warn("Optional Python editor unavailable; using textarea fallback", error);
});
