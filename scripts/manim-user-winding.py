"""Compare winding on explicit user arrays with unchanged pinned definitions."""
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
                          run_name="_winding_normalization")["normalize"]


def observations(namespace):
    function = namespace["get_winding_number"]
    result = [["signature", str(inspect.signature(function))]]
    square = np.array([[1, 1], [-1, 1], [-1, -1], [1, -1]])
    values = [[], (), [[1, 0]], [[0, 0]], [[1, 0], [-1, 0]],
              [[1, 0], [0, 1], [-1, 0], [0, -1]], "xy", None,
              [[float("nan"), 1], [1, 0]], [[float("inf"), 0], [0, 1]],
              [[1j, 0], [0, 1]], [["x", 0], [0, 1]], [[1], [0]],
              np.empty((0, 2)), np.empty((2, 0))]
    for dtype in (np.int64, np.float32, np.float64):
        for offset in ((0, 0), (3, 0), (0, -3)):
            points = (square + offset).astype(dtype)
            values.extend([points, points[::-1], points.tolist(),
                           np.vstack([points, points]), np.column_stack([points, np.zeros(4)])])
    for value in values:
        before = normalize(value)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                answer = function(value)
                outcome = ["value", type(answer).__name__, normalize(answer)]
            except Exception as error:
                outcome = ["error", type(error).__name__]
        result.append([outcome, [type(w.message).__name__ for w in caught],
                       normalize(value) == before])
    iterator = iter(square.tolist())
    try:
        function(iterator)
        outcome = ["value"]
    except Exception as error:
        outcome = ["error", type(error).__name__]
    result.append(["iterator", outcome, list(iterator)])
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/space_ops.py"
    iterable_source = source_root / "manim/utils/iterables.py"
    iterables = load_helpers(iterable_source, {"adjacent_pairs", "adjacent_n_tuples"}, {})
    original = load_helpers(source, {"get_winding_number", "angle_of_vector"},
                            {"np": np, "PI": np.pi, "TAU": 2 * np.pi,
                             "adjacent_pairs": iterables["adjacent_pairs"]})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors as helpers
    import noon
    errors = [] if observations(vars(helpers)) == expected else ["winding observations differ"]
    if noon.get_winding_number is not helpers.get_winding_number:
        errors.append("public callable identity differs")
    if installed_manim:
        from manim.utils import space_ops, iterables
        for module, path in ((space_ops, source), (iterables, iterable_source)):
            if Path(module.__file__).read_bytes() != path.read_bytes():
                raise ValueError("installed complete source differs from pin")
        if observations(vars(space_ops)) != expected:
            errors.append("installed winding observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "iterable_source_sha256": hashlib.sha256(iterable_source.read_bytes()).hexdigest(),
            "observations": len(expected), "installed_module_checked": installed_manim,
            "errors": errors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
