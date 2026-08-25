# Retained execution spatial index

Issue #66 introduces a renderer-independent retained spatial index keyed by stable
`ExecutionSlotId`. The first slice lives in `noon-runtime`; it does not make frame
indices or Python objects authoritative again.

## Contract

- The index stores conservative world-space AABBs for live frame objects.
- Stable execution slots are the indexed identity. Compatibility frame rows are
  used only to derive the current painter order and frame state.
- A deterministic uniform grid provides `O(cells + candidates)` viewport and point
  queries. Objects spanning too many cells are retained in a global oversized set;
  exceptionally large queries fall back explicitly and report that in query stats.
- Runtime mutation owns a second dirty stream independent from renderer
  `FrameChanges`, so renderer consumption cannot cause spatial updates to miss work
  or force repeated scene scans.
- Transform/style/timeline/structural edits refit or retire only affected leaves.
  Direct seek and explicit frame-slot compaction are deliberate full-index rebuild
  boundaries.
- Point queries return topmost painter candidates first. Rectangle/viewport queries
  return painter order bottom-to-top. Until #62 presentation metadata is carried in
  `FrameObjectState`, the runtime uses current semantic/frame insertion order, exactly
  matching the renderer's default painter-order adapter.

## Bounds

Analytic primitives use exact local boxes. Vector paths reuse their conservative
control-hull bounds. The box is transformed through the object's scale/rotation/
translation and conservatively expanded for stroke width. `External` geometry is
left unindexed until its resource payload is available at the execution boundary.

## Locality instrumentation

`SpatialIndexUpdateStats` reports full rebuilds, leaf upserts/removals, and cell
membership churn. `SpatialQueryStats` reports visited cells, exact candidates tested,
result count, and explicit full-scan fallbacks.

Scale regressions cover a 100,000-object point query without a scene scan, bounded
single-leaf motion, painter-order hits, and localized viewport queries.

## Renderer and browser integration

The renderer consumes retained viewport query results as stable execution slots and
builds a reusable `RenderVisibility` draw indirection in `O(visible)` work. Camera
changes do not repack instance buffers or path geometry, and offscreen vector paths
no longer force multisampling or draw submission. The execution render worker keeps
its own mirror-side `ExecutionSpatialIndex`, incrementally synchronized from transport
`FrameChanges`, so culling never round-trips through the Python/engine worker.

`ScenePlayer` remains the authoritative hit-test API. WASM `ScenePlayer` exposes
world-coordinate hit results, while direct `NoonCanvasPlayer` also converts backing-
store pixel coordinates through `Camera2D::screen_to_world`. Results keep topmost-first
painter ordering and include candidate/cell/fallback counters for editor observability.

Correctness now includes a deterministic indexed-vs-brute-force point/viewport corpus.
Combined with the 100,000-object locality regressions, this completes #66's retained
hit-testing and viewport-culling contract.
