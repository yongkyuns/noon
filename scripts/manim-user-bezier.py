"""Qualify Bézier callables over user arrays against unchanged Manim source."""
import argparse
from decimal import Decimal
from fractions import Fraction
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
                          run_name="_bezier_normalization")["normalize"]


def observations(namespace):
    factory = namespace["bezier"]
    result = [["signature", str(inspect.signature(factory))]]
    parameters = (-0.25, 0, 0.125, 0.5, 1, 1.5, np.float32(0.25),
                  np.array([0., .5, 1.]), np.array([[0.], [.5], [1.]]),
                  np.array(.25), [], [.25], False, None, .5 + .1j,
                  Decimal("0.5"), Fraction(1, 2), float("nan"), float("inf"))
    points_cases = [None, "x", [], [1, 2], [[1, 2], [3]], [["x", "y"]],
                    np.array([[Fraction(1, 3), Fraction(2, 3)]], dtype=object)]
    for dtype in (np.int64, np.float32, np.float64, np.complex128):
        for count in (0, 1, 2, 3, 4, 5, 8):
            for shape in ((count,), (count, 3), (count, 2, 3)):
                points_cases.append(np.arange(np.prod(shape)).reshape(shape).astype(dtype))
    for points in points_cases:
        before = normalize(points)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                curve = factory(points)
                outcome = ["callable", curve.__name__, str(inspect.signature(curve))]
            except Exception as error:
                outcome = ["error", type(error).__name__]
                curve = None
        result.append(["factory", outcome, [type(w.message).__name__ for w in caught],
                       normalize(points) == before])
        if curve is None:
            continue
        for parameter in parameters:
            input_before = normalize(parameter)
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter("always")
                try:
                    value = curve(parameter)
                    outcome = ["value", type(value).__name__, normalize(value),
                               isinstance(value, np.ndarray) and isinstance(points, np.ndarray)
                               and np.shares_memory(value, points)]
                except Exception as error:
                    outcome = ["error", type(error).__name__]
            result.append([outcome, [type(w.message).__name__ for w in caught],
                           normalize(points) == before, normalize(parameter) == input_before])
    for points in (np.array([[1., 2.], [3., 4.]]), [[1., 2.], [3., 4.]]):
        curve = factory(points)
        before = normalize(curve(.5))
        points[0][0] = 9
        result.append(["capture-after-mutation", type(points).__name__, before,
                       normalize(curve(.5))])
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/bezier.py"
    original = load_helpers(source, {"bezier"}, {"np": np, "overload": overload})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors as helpers
    import noon
    actual = observations(vars(helpers))
    errors = [] if actual == expected else ["user Bézier observations differ"]
    if noon.bezier is not helpers.bezier:
        errors.append("public callable identity differs")
    if installed_manim:
        from manim.utils import bezier as module
        if Path(module.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed complete Bézier module differs from pin")
        if observations(vars(module)) != expected:
            errors.append("installed user Bézier observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
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
