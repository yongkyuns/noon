# ManimCE v0.21 parity corpus

This directory contains the canonical source used by the Manim raster differential lane.

The `.py` source is written for real ManimCE. The Noon runner is allowed to change only the import from `manim` to `noon` and append the small scene-selection wrapper required by the browser authoring worker. Scene geometry, style, timing, waits, ordering, and animation calls must remain unchanged.

The initial scenes correspond to the Manim Community v0.21.0 quickstart tutorial. Manim Community is MIT licensed; upstream source and documentation are at https://docs.manim.community/en/v0.21.0/.

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

`triangle-options` covers omitted defaults, radius/start-angle overrides and
inherited regular-polygon vertex queries. The Rust Triangle constructor delegates
to the same closed regular-polygon path as the Python wrapper; the separate fixture
preserves the polygon compound-fill/stroke regression unchanged. Wider inherited
methods/options remain owned by #2012.

`mobject-table-top-left-entry` exercises ManimCE's corner entry with shared
retained objects and Rust-owned layout. Rust/WASM and Python-worker pixels match
on WebGPU and WebGL2; strict pinned ManimCE Cairo raster qualification passes the
unchanged thresholds on both backends with exact duration and 0.12% worst pixel
difference.

`table-inner-lines-omitted` covers ManimCE's `include_inner_lines=False` with
outer rules retained. The option defaults to `True`, travels through the Rust,
WASM, and Python constructors, and survives table-family rehydration. Its direct
Rust/WASM and Python-worker captures are pixel-identical on WebGPU and WebGL2;
the strict pinned Cairo differential passes unchanged limits on both backends
with exact duration and 0.18% worst differing pixels.

`table-line-config-stroke` covers the supported `line_config` subset for grid
stroke color, width, and opacity. The paired retained Rust/WASM and Python
example exercises that configuration, and the strict pinned Cairo fixture
passes the unchanged thresholds on WebGPU and WebGL2 with exact duration and
0.11% worst differing pixels. Paired Rust/WASM and Python-worker captures are
pixel-identical on both backends. Other line configuration keys, including
background stroke styling, and remaining table variants stay open under #2014.
