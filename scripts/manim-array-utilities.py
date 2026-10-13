"""Check user-data utilities against unchanged pinned Manim definitions."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import sys
import warnings

import numpy as np

from pinned_python_helpers import load_helpers

NAMES = {"batch_by_property", "uniq_chain", "hash_obj", "resize_array",
         "resize_preserving_order", "resize_with_interpolation", "stretch_array_to_length"}


def observations(namespace):
    result = []
    def record(name, *args):
        try:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", RuntimeWarning)
                value = namespace[name](*args)
            if isinstance(value, np.ndarray):
                value = {"values": value.tolist(), "dtype": str(value.dtype), "shape": value.shape,
                         "same_object": value is args[0], "shared_memory": np.shares_memory(value, args[0])}
            elif name == "uniq_chain":
                value = list(value)
            result.append([name, value])
        except (TypeError, ValueError, IndexError, Warning, ZeroDivisionError) as error:
            result.append([name, type(error).__name__])
    for shape in [(0,), (0, 2), (1,), (2,), (3, 2), (2, 2, 2)]:
        for dtype in [np.int64, np.float32, np.float64, np.complex128]:
            array = np.arange(np.prod(shape), dtype=np.float64).reshape(shape).astype(dtype)
            for length in [-1, 0, 1, 2, 5]:
                for name in ["resize_array", "resize_preserving_order", "resize_with_interpolation", "stretch_array_to_length"]:
                    record(name, array, length)
    for array in [np.arange(12).reshape(6, 2)[::2], np.array(["a", "b"]), np.array([True, False])]:
        for name in ["resize_array", "resize_preserving_order", "resize_with_interpolation", "stretch_array_to_length"]:
            record(name, array, 5)
    for values in [[], [0, 0, 1, 0], [None, None, 1, None]]:
        calls = []
        def property_func(value):
            calls.append(value)
            return value
        record("batch_by_property", iter(values), property_func)
        result.append(["callback_order", calls])
    record("uniq_chain", [1, 2], iter([2, 3, 1, 4]))
    record("uniq_chain", [[], []])
    events = []
    def values():
        for value in [1, 1, 2]:
            events.append(value)
            yield value
    generator = namespace["uniq_chain"](values())
    result.append(["lazy_initial", list(events)])
    result.append(["lazy_first", next(generator), list(events)])
    result.append(["lazy_rest", list(generator), list(events)])
    for obj in [None, True, 42, "text", [1, [2, 3]], {"a": [1, 2], "b": {3, 4}}, {1, 2}, bytearray(b"x")]:
        record("hash_obj", obj)
    result.append(["dict_order", namespace["hash_obj"]({"a": 1, "b": 2}) == namespace["hash_obj"]({"b": 2, "a": 1})])
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    source = args.manim_source / "manim/utils/iterables.py"
    expected = observations(load_helpers(source, NAMES, {"it": itertools, "np": np}))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_iterables
    import noon
    actual = observations(vars(_manim_iterables))
    errors = [] if actual == expected else ["user-data utility observations differ"]
    if args.installed_manim:
        from manim.utils import iterables
        if Path(iterables.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed iterable source differs from pin")
        if observations(vars(iterables)) != expected: errors.append("full installed module differs")
    if noon.stretch_array_to_length is not _manim_iterables.stretch_array_to_length:
        errors.append("public export identity differs")
    if "stretch_array_to_length" not in noon.__all__: errors.append("public export missing")
    print(json.dumps({"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "symbols": len(NAMES), "observations": len(expected), "numpy_version": np.__version__,
                      "installed_module_checked": args.installed_manim, "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__": raise SystemExit(main())
