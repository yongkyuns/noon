"""Qualify explicit NumPy array operations against unchanged Manim definitions."""
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
                          run_name="_array_normalization")["normalize"]
NAMES = ("normalize_along_axis", "rotation_matrix_from_quaternion",
         "rotation_matrix_transpose_from_quaternion")


def observations(namespace):
    result = [[name, str(inspect.signature(namespace[name]))] for name in NAMES]

    def record(name, *args):
        before = normalize(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                value = namespace[name](*args)
                outcome = ["value", type(value).__name__, normalize(value),
                           [value is arg for arg in args]]
            except Exception as error:
                outcome = ["error", type(error).__name__]
        result.append([name, outcome, [type(x.message).__name__ for x in caught],
                       before, normalize(args)])

    for dtype in (np.float32, np.float64, np.int64, np.complex128):
        for shape in ((0,), (3,), (0, 3), (2, 3), (2, 2, 3)):
            for axis in (-3, -1, 0, 1, 2, 3, None):
                array = np.arange(np.prod(shape)).reshape(shape).astype(dtype)
                record("normalize_along_axis", array, axis)
        record("normalize_along_axis", np.zeros((2, 3), dtype=dtype), 1)
        record("normalize_along_axis", np.arange(24).reshape(4, 6).astype(dtype)[::2, ::2], 1)
        array = np.ones((2, 3), dtype=dtype)
        array.flags.writeable = False
        record("normalize_along_axis", array, 1)
    for array in ([[3., 4.]], np.full((2, 3), np.nan), np.full((2, 3), np.inf)):
        record("normalize_along_axis", array, 1)
    quaternions = ([], [1, 0, 0, 0], [0, 1, 0, 0], [1, 2, 3, 4], [1, 2],
                   (0.5, 0.5, 0.5, 0.5), [float("nan"), 0, 0, 0], ["x", 0, 0, 0],
                   np.array([1, 2, 3, 4], dtype=np.float32), np.ones((4, 2)))
    for quat in quaternions:
        for name in NAMES[1:]:
            record(name, quat)
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/space_ops.py"
    original = load_helpers(source, set(NAMES) | {"quaternion_mult", "quaternion_conjugate"},
                            {"np": np})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors as helpers
    import noon
    errors = [] if observations(vars(helpers)) == expected else ["array operation observations differ"]
    for name in NAMES:
        if getattr(noon, name) is not getattr(helpers, name):
            errors.append(f"public identity {name} differs")
    if installed_manim:
        from manim.utils import space_ops
        if Path(space_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed complete space-ops module differs from pin")
        if observations(vars(space_ops)) != expected:
            errors.append("installed array operation observations differ")
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
