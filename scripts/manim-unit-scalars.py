"""Check module-only scalar unit values against complete pinned ManimCE."""
import argparse
import hashlib
import json
from pathlib import Path
import sys


def observations(module):
    values = []
    for name in ("Degrees", "Munits"):
        scale = getattr(module, name)
        rows = []
        for number in (False, True, -720, -180, -1, 0, 1, 30, 90, 180, 360, 1e-12, 1e12):
            rows.append([float(number * scale).hex(), float(scale * number).hex()])
        import numpy as np
        arrays = []
        for dtype in (np.int64, np.float32, np.float64):
            data = np.array([-3, 0, 5], dtype=dtype)
            for result in (data * scale, scale * data):
                arrays.append([str(result.dtype), [float(x).hex() for x in result]])
        values.append([name, type(scale).__name__, float(scale).hex(), rows, arrays])
    return values


def qualify(source_root):
    import manim
    from manim.utils import unit as reference
    pinned = source_root / "manim/utils/unit.py"
    if Path(reference.__file__).read_bytes() != pinned.read_bytes():
        raise ValueError("installed complete unit module differs from pinned source")
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    actual = observations(noon.unit)
    expected = observations(reference)
    errors = []
    if actual != expected:
        errors.append("scalar unit observations differ")
    for module in (manim, noon):
        if any(hasattr(module, name) for name in ("Degrees", "Munits")):
            errors.append(f"{module.__name__}: scalar units leaked into top-level namespace")
    if "unit" not in noon.__all__:
        errors.append("unit module missing from public namespace")
    return {"source_sha256": hashlib.sha256(pinned.read_bytes()).hexdigest(),
            "observations": expected, "errors": errors,
            "scope": "Degrees/Munits only; Pixels/Percent and frame configuration remain open"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    args = parser.parse_args()
    result = qualify(args.source_root)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
