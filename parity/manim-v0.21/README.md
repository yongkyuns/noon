# ManimCE v0.21 parity corpus

This directory contains the canonical source used by the Manim raster differential lane.

The `.py` source is written for real ManimCE. The Noon runner is allowed to change only the import from `manim` to `noon` and append the small scene-selection wrapper required by the browser authoring worker. Scene geometry, style, timing, waits, ordering, and animation calls must remain unchanged.

The initial scenes correspond to the Manim Community v0.21.0 quickstart tutorial. Manim Community is MIT licensed; upstream source and documentation are at https://docs.manim.community/en/v0.21.0/.

Do not use this directory for pedagogical Noon-specific adaptations. Those belong in the demo/tutorial corpus. A parity fixture is a compatibility oracle, not a showcase scene.

The full pinned directive inventory is extracted by `scripts/manim-reference-inventory.mjs` and locked under `compat/`. Public tutorial/reference enrollment remains in `web/python/examples/manim_tutorial_manifest.json`; the remaining directives have internal provenance/ownership records in `compat/manim-v0.21.0-reference-dispositions.json`. The coverage join rejects duplicate or stale identities and requires every directive to be represented. Pending `unqualified`, `blocked` and `deferred` records are not executable or parity-qualified claims. New exports require stale namespace blockers to be reconciled without automatically promoting behavior. Existing exact-camera qualification is retained separately in recorded case evidence until its canonical enrollment is reconciled.
