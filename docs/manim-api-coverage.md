# ManimCE v0.21.0 API coverage

Noon pins user-facing compatibility work to **Manim Community v0.21.0**.

API presence and behavioral parity are intentionally separate gates:

- `scripts/manim-api-coverage.py` inventories the complete pinned Manim public namespace and classifies every symbol as `supported`, `partial`, `blocked`, `deferred`, `intentional-divergence`, or `missing`.
- `scripts/manim-differential.py` compares renderer-independent behavior for features Noon actually claims to support.

The classification policy lives in `compat/manim-v0.21.0.json`. Module-level rules make large known gaps such as text/math, plotting, graph networks, and 3D explicit, while individual overrides record supported or partially supported public symbols.

## Running the report

Install the pinned reference and run:

```bash
python -m pip install "manim==0.21.0"
python scripts/manim-api-coverage.py --check
```

CI also writes `manim-v0.21-api-coverage.md` as a workflow artifact. The report includes status totals, category totals, and the complete non-supported symbol list with issue dependencies where known.

## Updating compatibility

A parity PR that exposes a new Manim-compatible public symbol should update `compat/manim-v0.21.0.json` and add behavioral evidence when applicable. Exporting a name alone does not prove compatibility: unreviewed Noon exports resolve to `partial` rather than being automatically promoted to `supported`.

Features blocked by architectural work should remain visibly `blocked` and name the relevant issue. Full 3D/vector-space behavior is deliberately `deferred` until #90 defines the intended compatibility target.

This coverage report is an inventory, not a promise that every Manim symbol will be implemented. Intentional browser/runtime divergences should be recorded explicitly rather than silently omitted.
