from __future__ import annotations

import json
import runpy
import traceback
from pathlib import Path

root = Path(__file__).resolve().parents[1]
ns = runpy.run_path(str(root / "scripts" / "manim-differential.py"))
results = []
for fixture in ns["FIXTURES"]:
    record = {"fixture": fixture.name}
    try:
        noon_value = fixture.noon_probe()
        manim_value = fixture.manim_probe()
        differences = ns["_compare"](noon_value, manim_value, fixture.tolerance)
        record.update(
            status="pass" if not differences else "mismatch",
            noon=noon_value,
            manim=manim_value,
            differences=differences,
        )
    except Exception as exc:
        record.update(
            status="error",
            error=f"{type(exc).__name__}: {exc}",
            traceback=traceback.format_exc(),
        )
    results.append(record)

(root / "parity-diagnostics.json").write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
for cache in (root / "web" / "python").glob("__pycache__"):
    for child in cache.iterdir():
        child.unlink()
    cache.rmdir()
