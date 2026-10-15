"""Compare explicit vector rotation values with complete pinned Manim.

Mutable default direction identity and exact default-object signatures remain
unqualified; this profile records numeric arrays, call boundaries and errors.
"""
import argparse
from decimal import Decimal
from fractions import Fraction
import hashlib
import inspect
import json
import math
from pathlib import Path
import runpy
import sys
import warnings

import numpy as np
from scipy.spatial.transform import Rotation
from pinned_python_helpers import load_helpers

NAMES = ("rotate_vector", "compass_directions")
normalize_value = runpy.run_path(str(Path(__file__).with_name("manim-simple-functions.py")),
                               run_name="_vector_rotation_normalization")["normalize"]


def observations(namespace):
    result = []

    def record(name, *args):
        before = normalize_value(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                value = namespace[name](*args)
                outcome = ["value", type(value).__name__, normalize_value(value)]
                if isinstance(value, np.ndarray):
                    outcome.append([np.shares_memory(value, arg) if isinstance(arg, np.ndarray)
                                    else False for arg in args])
            except Exception as error:
                outcome = ["error", type(error).__name__]
        result.append([name, normalize_value(args), outcome,
                       [type(item.message).__name__ for item in caught],
                       normalize_value(args) == before])

    readonly = np.array([1., 2., 3.])
    readonly.flags.writeable = False
    vectors = [[], [0], [1, 2], [1, 2, 3], [1, 2, 3, 4],
               [float("inf"), 0, 1], [float("nan"), 1, 2], ["x", 1, 2], [1j, 2, 3],
               np.array([1, 2, 3], dtype=np.float32), np.arange(6, dtype=float)[::2],
               np.array([[1, 0, 0], [0, 1, 0]]), readonly,
               [Fraction(1, 3), Fraction(2, 3), 1], [Decimal("0.1"), Decimal("0.2"), 1]]
    axes = [[0, 0, 1], [0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 2], readonly]
    angles = [0, .2, math.pi, -math.pi / 3, np.float32(.2), Fraction(1, 3),
              Decimal(".2"), True, float("inf"), float("nan"), "bad"]
    for vector in vectors:
        for angle in angles:
            record("rotate_vector", vector, angle)
            for axis in axes:
                record("rotate_vector", vector, angle, axis)
    for count in [-2, 0, 1, 3, 4, np.int32(4), True, 2.0, np.float64(3), None, "4"]:
        record("compass_directions", count)
        for vector in vectors:
            record("compass_directions", count, vector)
    record("compass_directions")
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/space_ops.py"
    original = load_helpers(source, set(NAMES) | {"rotation_matrix", "normalize"},
                            {"np": np, "Rotation": Rotation, "TAU": math.tau,
                             "RIGHT": np.array([1., 0., 0.]), "OUT": np.array([0., 0., 1.])})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors as helpers
    import noon
    actual = observations(vars(helpers))
    errors = [] if actual == expected else ["explicit vector rotation observations differ"]
    differences = [[i, a, b] for i, (a, b) in enumerate(zip(expected, actual)) if a != b]
    for name in NAMES:
        if name not in noon.__all__ or getattr(noon, name) is not getattr(helpers, name):
            errors.append(f"public helper identity differs: {name}")
    if installed_manim:
        from manim.utils import space_ops
        if Path(space_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed complete space-ops source differs from pin")
        if observations(vars(space_ops)) != expected:
            errors.append("installed vector rotation observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "functions": len(NAMES), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors,
            "differences": differences,
            "unqualified_signature_differences": {
                name: {"original": str(inspect.signature(original[name])),
                       "noon": str(inspect.signature(getattr(helpers, name)))}
                for name in NAMES
                if str(inspect.signature(original[name])) != str(inspect.signature(getattr(helpers, name)))
            },
            "unqualified": ["mutable default direction identities", "exact default-object signatures",
                            "full annotations", "engine geometry and scene transforms"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
