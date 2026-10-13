"""Compare explicit user-vector helpers with unchanged pinned Manim definitions."""
import argparse
from collections.abc import Callable, Sequence
from decimal import Decimal
from fractions import Fraction
import hashlib
import inspect
import json
from pathlib import Path
import sys
import warnings

import numpy as np

import runpy

normalize = runpy.run_path(str(Path(__file__).with_name("manim-simple-functions.py")),
                          run_name="_vector_normalization")["normalize"]
from pinned_python_helpers import load_helpers

NAMES = {"complex_to_R3", "R3_to_complex", "complex_func_to_R3_func", "center_of_mass",
         "midpoint", "normalize", "angle_of_vector", "angle_between_vectors",
         "shoelace", "shoelace_direction", "cross2d"}


def observations(namespace):
    result = [["signature", name, str(inspect.signature(namespace[name]))] for name in sorted(NAMES)]

    def record(name, *args):
        before = normalize(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                value = namespace[name](*args)
                outcome = ["value", type(value).__name__, normalize(value), [value is arg for arg in args]]
                if isinstance(value, np.ndarray):
                    outcome.append([np.shares_memory(value, arg) if isinstance(arg, np.ndarray)
                                    else False for arg in args])
            except Exception as error:
                outcome = ["error", type(error).__name__]
        result.append([name, outcome, [type(item.message).__name__ for item in caught], normalize(args) == before])

    vectors = [[], [0], [0, 0], [3, 4, 0], [-1, 2, 3], [float("nan"), 1],
               [float("inf"), 0], [1j, 2], ["a", "b"], [Fraction(1, 3), Fraction(2, 3)],
               [Decimal("0.1"), Decimal("0.2")], np.array([1, 2], dtype=np.float32),
               np.array([[1, 2], [3, 4]], dtype=np.int64), np.empty((0, 2)),
               np.array([[1, 2, 3]]), np.arange(12).reshape(4, 3)[::2]]
    for vector in vectors:
        for name in ["R3_to_complex", "normalize", "angle_of_vector", "center_of_mass",
                     "shoelace", "shoelace_direction"]:
            record(name, vector)
        for fallback in [None, [], [7, 8], np.array([7, 8]), np.array([7])]:
            record("normalize", vector, fallback)
        for other in vectors:
            for name in ["midpoint", "angle_between_vectors", "cross2d"]:
                record(name, vector, other)
    for value in [0, 1j, -2 + 3j, float("nan"), "x", Decimal("0.1"), np.complex64(1 + 2j)]:
        record("complex_to_R3", value)
    for fails in [False, True]:
        events = []
        def callback(value):
            events.append(normalize(value))
            if fails:
                raise LookupError("user callback")
            return value * value + 1j
        converted = namespace["complex_func_to_R3_func"](callback)
        result.append(["callback_initial", list(events)])
        for vector in [[1, 2, 3], [3], [1, 2, 3, 4], ["bad", 2]]:
            try:
                value = converted(vector)
                outcome = ["value", type(value).__name__, normalize(value)]
            except Exception as error:
                outcome = ["error", type(error).__name__]
            result.append(["callback_result", outcome, list(events)])
    return result


def qualify(manim_source, *, installed_manim=False):
    source = manim_source / "manim/utils/space_ops.py"
    expected = observations(load_helpers(source, NAMES, {"np": np, "Callable": Callable, "Sequence": Sequence}))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors
    import noon
    actual = observations(vars(_manim_user_vectors))
    errors = [] if actual == expected else ["user-vector observations differ"]
    if installed_manim:
        from manim.utils import space_ops
        if Path(space_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed space-ops source differs from pin")
        if observations(vars(space_ops)) != expected:
            errors.append("installed space-ops observations differ")
    for name in NAMES:
        if getattr(noon, name) is not getattr(_manim_user_vectors, name) or name not in noon.__all__:
            errors.append(f"public vector export {name} differs")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "symbols": len(NAMES),
            "observations": len(expected), "numpy_version": np.__version__,
            "installed_module_checked": installed_manim, "errors": errors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.manim_source, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
