"""Qualify curve-array helpers without owning canonical engine geometry."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import runpy
import sys
from typing import overload
import warnings

import numpy as np
from pinned_python_helpers import load_helpers

normalize = runpy.run_path(str(Path(__file__).with_name("manim-simple-functions.py")),
                          run_name="_curve_normalization")["normalize"]
NAMES = {"partial_bezier_points", "split_bezier", "is_closed", "straight_path"}


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

    cases = [[], None, [[0, 0, 0]], [[-1, -1, -1], [-1, -1, -1]],
             [[1, 2, 3], [1, 2, 3]], [["x", 0, 0]], np.empty((0, 3))]
    for dtype in (np.int64, np.float32, np.float64, np.complex128):
        for count in (1, 2, 3, 4, 5, 8):
            for shape in ((count,), (count, 3), (count, 2, 3)):
                cases.append(np.arange(np.prod(shape)).reshape(shape).astype(dtype))
    for points in cases:
        record("is_closed", points)
        for a, b in ((0, 1), (.2, .8), (.9, .1), (1, .5), (0, 0), (.5, .5),
                     (-.2, 1.2), (np.float32(.2), np.float32(.8)),
                     (None, .5), ([.2], [.8]), (float("nan"), 1)):
            record("partial_bezier_points", points, a, b)
        for t in (0, .5, 1, -.25, 1.5, np.float32(.5), None, [.5], float("nan")):
            record("split_bezier", points, t)
    path = namespace["straight_path"]()
    result.append(["path-identity", path is namespace["interpolate"],
                   path.__name__, str(inspect.signature(path))])
    path_namespace = {"path": path}
    original_namespace = namespace
    namespace = path_namespace
    for start, end in ((0, 2), ([1, 2], [3, 4]),
                       (np.array([1., 2.]), np.array([3., 4.])),
                       (np.ones((2, 1)), np.arange(3))):
        for alpha in (0, .5, 1, -.2, 1.2, np.array([[0], [.5], [1]]), None):
            record("path", start, end, alpha)
    namespace = original_namespace
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/bezier.py"
    paths_source = source_root / "manim/utils/paths.py"
    original = load_helpers(source, {"partial_bezier_points", "split_bezier", "is_closed", "interpolate"}, {"np": np, "overload": overload})
    original.update(load_helpers(paths_source, {"straight_path"}, {"interpolate": original["interpolate"]}))
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors as helpers
    import _manim_interpolation
    import noon
    actual_namespace = {**vars(helpers), "interpolate": _manim_interpolation.interpolate}
    errors = [] if observations(actual_namespace) == expected else ["curve operation observations differ"]
    for name in NAMES:
        if getattr(noon, name) is not getattr(helpers, name):
            errors.append(f"public identity {name} differs")
    if installed_manim:
        from manim.utils import bezier, paths
        for module, path in ((bezier, source), (paths, paths_source)):
            if Path(module.__file__).read_bytes() != path.read_bytes():
                raise ValueError("installed complete source differs from pin")
        installed = {**vars(bezier), "straight_path": paths.straight_path}
        if observations(installed) != expected:
            errors.append("installed curve operation observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "paths_source_sha256": hashlib.sha256(paths_source.read_bytes()).hexdigest(),
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
