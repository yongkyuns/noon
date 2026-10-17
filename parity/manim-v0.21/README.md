# ManimCE v0.21 parity corpus

This directory contains the canonical source used by the Manim raster differential lane.

The `.py` source is written for real ManimCE. The Noon runner is allowed to change only the import from `manim` to `noon` and append the small scene-selection wrapper required by the browser authoring worker. Scene geometry, style, timing, waits, ordering, and animation calls must remain unchanged. A fixture may name a `noon_source` only when its canonical ManimCE scene depends on private upstream state; the paired Noon scene must preserve the same final state through Noon-supported public inputs.

The initial scenes correspond to the Manim Community v0.21.0 quickstart tutorial. Manim Community is MIT licensed; upstream source and documentation are at https://docs.manim.community/en/v0.21.0/.

`graph-default-spring` adds a seeded ManimCE Graph that omits the `layout` argument, giving the default spring placement a direct pinned Cairo comparison on both browser backends. The seed makes the NetworkX initialization reproducible.

`graph-weighted-spring` and `digraph-weighted-spring` compare weighted placements against pinned ManimCE Cairo. The canonical source assigns edge attributes to ManimCE's private NetworkX graph before calling `change_layout`; the Noon-only source carries those same attributes in supported edge triples and selects the non-default `strength` field through `layout_config["weight"]`. Both strict local Cairo raster fixtures pass on WebGPU and WebGL2 (maximum pixel differences 0.01% and 0.19%); Manim refreshes its static edge updater before capture, and the hosted exact-head run remains pending.

`graph-seeded-random-layout` and `digraph-seeded-random-layout` compare NumPy-seeded NetworkX random placements against pinned ManimCE Cairo, including an asymmetric directed graph.

`graph-circular-layout`, `graph-shell-layout`, `graph-spiral-layout`, `graph-partite-layout`, `graph-spectral-layout`, and `graph-kamada-kawai-layout` enroll the supported placement families for pinned Cairo comparison; directed fixtures are included where ManimCE and Noon support them. `graph-tree-layout` covers the supported undirected tree configuration. Integer keys keep NetworkX 3.4.2's set-backed partite member order stable, and the directed spectral fixture starts with a positive first eigenvector component so its arbitrary SciPy sign matches Noon's canonical axis. All 26 current Graph/DiGraph fixtures pass the unchanged strict local Cairo raster ratchet on WebGPU and WebGL2 (52 fixture/backend samples; worst differing-pixel ratio 0.41%). On macOS, set `NOON_MANIM_REFERENCE_FONTS` to the exact bundled `DejaVuSansMono.ttf` file so Manim registers the same face used by Noon; otherwise the local system fallback selects Andale Mono. Hosted exact-PR-head qualification remains pending.

`graph-circle-late-vertex` checks the standard Circle constructor defaults through `add_vertices`, separately from constructor-wide `vertex_type`. `graph-default-labels` and `digraph-custom-labels` enroll ManimCE's default MathTex labels and caller-supplied MathTex/Text labels on every vertex through constructor layouts, incremental vertex creation, copies, and removals. `graph-change-layout-scale` isolates the upstream `change_layout(..., layout_scale=...)` call on a vertex-only graph so the placement API can be compared without edge lifecycle behavior obscuring it. `graph-style` also covers caller-supplied DejaVu Sans Mono Text labels alongside global and per-key paint; those supplied label Mobjects belong to one graph to avoid cross-parent aliasing. On macOS, the reference runner must register the bundled DejaVu Sans Mono face through `NOON_MANIM_REFERENCE_FONTS`; Manim otherwise falls back to Andale Mono. ManimCE 0.21.0 converts any nonempty `labels` mapping to `LabeledDot` for every vertex but supplies `label` only for mapped keys, so a partial mapping raises `TypeError` during construction. Noon keeps partial mappings as an extension; it is tested locally and carries no upstream raster parity claim. All Graph/DiGraph label, style, default-API and late-Circle raster fixtures pass the unchanged strict local policy on both WebGPU and WebGL2; hosted exact-PR-head workflow is still pending.

Do not use this directory for pedagogical Noon-specific adaptations. Those belong in the demo/tutorial corpus. A parity fixture is a compatibility oracle, not a showcase scene.

The full pinned directive inventory is extracted by `scripts/manim-reference-inventory.mjs` and locked under `compat/`. Public tutorial/reference enrollment remains in `web/python/examples/manim_tutorial_manifest.json`; the remaining directives have internal provenance/ownership records in `compat/manim-v0.21.0-reference-dispositions.json`. The coverage join rejects duplicate or stale identities and requires every directive to be represented. Pending `unqualified`, `blocked` and `deferred` records are not executable or parity-qualified claims. New exports require stale namespace blockers to be reconciled without automatically promoting behavior. Existing exact-camera qualification is retained separately in recorded case evidence until its canonical enrollment is reconciled.

`paint-fade` covers ordinary leaf fades and recursive nested-family fades with
shared descendants. Manim's recursive fade visits a leaf once per membership
path; shared Rust counts those paths and publishes each affected leaf once.
The paired native/direct-WASM/Python scene is `family_paint`, qualified by
`scripts/paint-fade-qualification.mjs`. Callback fades use the same Rust paint
operation over the current ordered overlay. The bounded profile accepts darkness
in `[0, 1]`; background strokes, mixed image families and non-recursive
family-container paint remain unqualified. Image fade resets original pixel
alpha in Manim and is explicitly rejected rather than inheriting vector alpha
multiplication. Unrepresentable recursive occurrence counts fail before mutation.

`Mobject.fade_to(color, alpha)` now interpolates the shared current RGB paint
toward the target and routes the result through the common color edit, preserving
fill and stroke opacity. Direct, live, recursive-family, and staged callback
paths share that implementation; repeated family aliases blend once per membership
path using bounded composition and one publication per leaf. Point-free geometry
and image objects remain unchanged. Focused
Rust and Python adapter tests cover RGB interpolation, opacity preservation,
aliases, and publication routing. The pinned-source `paint-fade` fixture blends
the current vector fill/stroke color after prior paint edits, as Manim's
`VMobject.get_color` does, and holds the result in a visible interval.
Its exact-head native/direct-WASM/Python raster comparison and ManimCE oracle are
pending. Native Text hierarchy differences, resource paint color queries and
broader object types remain unqualified.

`triangle-options` covers omitted defaults, radius/start-angle overrides and
inherited regular-polygon vertex queries. The Rust Triangle constructor delegates
to the same closed regular-polygon path as the Python wrapper; the separate fixture
preserves the polygon compound-fill/stroke regression unchanged. Wider inherited
methods/options remain owned by #2012.

Angle nominal-value overrides are semantic metadata: assigning `angle_value`
updates the Rust-owned value without rebuilding the authored arc/elbow path, and
live assignments use the normal publication transaction. Focused native/live and
Python adapter tests cover this behavior, and the paired constructor scene checks
the same assignments without changing its paths. Pending callback phases reject
nominal-value writes through the existing publication barrier; callback-local
Angle metadata overlays are not supported. This non-visual operation does not qualify
the still-open dot-child, non-planar, or wider constructor behavior in #2012.
