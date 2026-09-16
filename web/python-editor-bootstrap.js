// Start presentation-only editor enhancement as soon as the playground DOM is ready.
// Ruff's WASM linter remains lazy until the first real editor input.
void import("./python-editor.js").catch((error) => {
  console.warn("Optional Python editor unavailable; using textarea fallback", error);
});

// Warm one persistent full-source authoring/execution session after the initial source/gallery
// paint, then coalesce editor changes onto the existing Run path. This is browser UX plumbing;
// semantic identity, scheduling and reconciliation remain owned by the existing engine path.
void import("./live-authoring-bootstrap.js").catch((error) => {
  console.warn("Live Python authoring unavailable; explicit Run remains available", error);
});
