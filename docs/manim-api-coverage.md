# ManimCE v0.21.0 API coverage

Noon pins user-facing compatibility work to **Manim Community v0.21.0**.

API presence, behavioral parity, executable example coverage, and exact observable-output parity are intentionally separate but connected gates:

- `scripts/manim-api-coverage.py` inventories the complete pinned Manim public namespace and classifies every symbol as `supported`, `partial`, `blocked`, `deferred`, `intentional-divergence`, or `missing`.
- `scripts/manim-differential.py` compares renderer-independent behavior for features Noon actually claims to support.
- `parity/manim-v0.21/manifest.json` defines the canonical source-equivalent Manim raster + timeline corpus and the pixel/time comparison policy.
- `web/python/examples/manim_tutorial_manifest.json`, when present, is the single source for tutorial/example coverage. The API report validates and summarizes that same manifest rather than maintaining a second example list.

The API classification policy lives in `compat/manim-v0.21.0.json`. The report audits the current `web/python` public export construction and joins it with ready entries in the executable tutorial manifest. `missing` means the symbol is not exported on the audited ref; `blocked` is reserved for an identified architecture dependency; `deferred` is used for deliberate non-goals such as the current 3D target. Individual overrides record supported or partially supported public symbols and high-value issue ownership.

## Exact-output promotion rule

A public symbol or behavior is not promoted to `supported` merely because the name exists, a semantic probe passes, or a Noon-adapted tutorial renders. For observable Manim-facing behavior, `supported` requires the exact-output gate tracked by #185/#176 for the relevant slice:

- use a source-equivalent ManimCE v0.21.0 fixture with no Noon-only geometry, style, layout, camera, or timing compensation;
- compare normalized semantic state and rendered pixels against real Manim under the pinned canonical profile;
- for animated/time-dependent behavior, match duration, child intervals, rate functions, lifecycle boundaries, and intermediate states at begin, 25%, 50%, 75%, the final rendered sample, exact end, and any additional discontinuity-sensitive samples;
- deterministic direct seek must agree with forward playback;
- WebGPU and WebGL must both satisfy the reference contract where the backend is claimed supported;
- expose at least one user-facing demo/gallery example for the same source-equivalent behavior, linked to its canonical parity fixture.

Until those gates pass, keep the item `partial`, `blocked`, `deferred`, or `intentional-divergence` as appropriate. Semantic/numeric tests remain required because they diagnose failures cheaply, but they do not replace pixel/time-level qualification.

Tutorial manifest `status: ready` means the example executes. It does **not** mean visual compatibility. Source-equivalent demo entries therefore carry separate `parity_status` metadata: `candidate` while exact-output work remains, and `parity-qualified` only after the canonical gate passes. Existing `original-noon-adaptation` examples are explicitly marked `adaptation`.

## Running the report

Install the pinned reference and run:

```bash
python -m pip install "manim==0.21.0"
python scripts/manim-api-coverage.py --check
```

CI also writes `manim-v0.21-api-coverage.md` as a workflow artifact. The report includes status totals, category totals, tutorial/example readiness when the manifest exists, and the complete non-supported symbol list with issue dependencies where known.

## Updating compatibility

A parity PR that exposes a new Manim-compatible public symbol should update `compat/manim-v0.21.0.json` and add behavioral evidence when applicable. Exporting a name alone does not prove compatibility: unreviewed Noon exports resolve to `partial` rather than being automatically promoted to `supported`. `supported` entries must name executable evidence, and the report fails if an exported symbol with a ready browser fixture is explicitly left `blocked`.

A parity PR that unlocks a tutorial or gallery example should update `web/python/examples/manim_tutorial_manifest.json` rather than adding a duplicate coverage list. Ready entries require an executable fixture plus upstream provenance/reuse metadata; blocked and deferred entries require an issue dependency. Source-equivalent parity examples must additionally link a canonical `parity_fixture` and carry their independent `parity_status`.

Features blocked by architectural work should remain visibly `blocked` and name the relevant issue. Full 3D/vector-space behavior is deliberately `deferred` until #90 defines the intended compatibility target; any 3D slice later promoted to supported is still subject to the exact-output promotion rule above.

This coverage report is an inventory, not a promise that every Manim symbol will be implemented. Intentional browser/runtime divergences should be recorded explicitly rather than silently omitted.
