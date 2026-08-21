# Vector geometry milestone status

This note records the implementation state of Milestone 4 after the generic Transform and lifecycle work. It supplements the older checklist in `implementation-plan.md`, whose remaining-work section predates several completed slices.

## Implemented

- Renderer-independent `VectorPath` commands and versioned authoring/serialization.
- Deterministic Lyon fill/stroke tessellation with structural correctness tests.
- Cached path meshes and instanced transform/style rendering.
- Numerically testable path-reveal metadata and runtime reveal progress.
- Compatible, endpoint-exact morph planning with renderer reuse across morph progress.
- Generic Transform across analytic and vector geometry, including evaluated authoring snapshots.
- `ReplacementTransform`, `TransformFromCopy`, chained lifecycle composition, and deterministic `TransformMatchingShapes` lowering.
- Bounded historical path-mesh retention for long-lived authoring sessions, with incoming-frame pinning and stale LRU eviction.

## Remaining focused work

The vector-geometry foundation no longer has a missing reveal, morph-plan, or cache-lifecycle primitive. Remaining work is primarily hardening and scale validation:

- larger path and morph performance baselines;
- optional byte-weighted cache budgeting if real workloads show entry count is an insufficient memory proxy;
- broader property/fuzz coverage for malformed paths, transforms, seeks, and live patches;
- small controlled raster/golden tests only where structural and numerical tests cannot prove renderer output.

Higher-level appearance behavior such as Fade should be modeled explicitly rather than emulated through matching-shape opacity tricks. Text remains a separate architecture milestone (`GlyphRun` for normal text and `OutlineText` for path-level animation) rather than being folded into generic vector paths.
