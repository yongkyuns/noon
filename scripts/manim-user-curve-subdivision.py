"""Qualify explicit user-array subdivision and curve-count remapping."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import runpy
import sys
import warnings

import numpy as np
from pinned_python_helpers import load_helpers

normalize = runpy.run_path(str(Path(__file__).with_name("manim-simple-functions.py")),
                          run_name="_subdivision_normalization")["normalize"]
NAMES = {"subdivide_bezier", "bezier_remap"}


def observations(namespace):
    result = [[name, str(inspect.signature(namespace[name]))] for name in sorted(NAMES)]

    def record(name, *args):
        before = normalize(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                value = namespace[name](*args)
                outcome = ["value", type(value).__name__, normalize(value),
                           [value is arg for arg in args],
                           [isinstance(value, np.ndarray) and isinstance(arg, np.ndarray)
                            and np.shares_memory(value, arg) for arg in args]]
            except Exception as error:
                outcome = ["error", type(error).__name__]
        result.append([name, outcome, [type(w.message).__name__ for w in caught],
                       normalize(args) == before])

    divisions = (0, 1, 2, 3, 5, -1, 1.5, np.int64(2), True, None, "2")
    for dtype in (np.int64, np.float32, np.float64, np.complex128):
        for count in (0, 1, 2, 3, 4, 5, 8):
            points = np.arange(count * 3).reshape(count, 3).astype(dtype)
            for divisions_count in divisions:
                record("subdivide_bezier", points, divisions_count)
            record("subdivide_bezier", points.tolist(), 2)
            if count:
                points.flags.writeable = False
                record("subdivide_bezier", points, 2)
        for curves in (0, 1, 2, 3):
            for count in (1, 2, 3, 4, 5):
                points = np.arange(curves * count * 3).reshape(curves, count, 3).astype(dtype)
                for new_count in (0, 1, 2, 4, 7, -1, 1.5, np.int64(3), None):
                    record("bezier_remap", points, new_count)
    for points in ([], None, [1, 2], [[1, 2], [3]], np.ones((4, 2, 3))):
        for divisions_count in (1, 2):
            record("subdivide_bezier", points, divisions_count)
        record("bezier_remap", points, 2)
    # Repeated low-degree calls exercise the original coefficient memoization.
    for _ in range(2):
        record("subdivide_bezier", np.arange(12).reshape(4, 3), 3)
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/bezier.py"
    original = load_helpers(source, NAMES | {"_get_subdivision_matrix"},
                            {"np": np, "SUBDIVISION_MATRICES": [{} for _ in range(4)]})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors as helpers
    import noon
    errors = [] if observations(vars(helpers)) == expected else ["subdivision observations differ"]
    for name in NAMES:
        if getattr(noon, name) is not getattr(helpers, name):
            errors.append(f"public identity {name} differs")
    if installed_manim:
        from manim.utils import bezier
        if Path(bezier.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed complete Bézier source differs from pin")
        if observations(vars(bezier)) != expected:
            errors.append("installed subdivision observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "functions": len(NAMES), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
