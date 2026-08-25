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

## Next #66 slice

Renderer viewport culling should consume `ScenePlayer::query_viewport` / the same
execution index and filter ordered draw submission without repacking unrelated GPU
instance or path geometry. Native hover/selection can consume `hit_test` directly.
