"""Qualify Python iterable helpers against untouched ManimCE definitions."""
import argparse
from collections.abc import Iterable
import hashlib
import itertools
import json
from pathlib import Path
import sys
from typing import overload

from pinned_python_helpers import load_helpers

NAMES = {"adjacent_n_tuples", "adjacent_pairs", "all_elements_are_instances", "concatenate_lists",
         "list_difference_update", "list_update", "listify", "tuplify", "make_even",
         "make_even_by_cycling", "remove_list_redundancies", "remove_nones"}


def observations(namespace):
    output = []
    def record(name, *args):
        try:
            value = namespace[name](*args)
            if isinstance(value, zip): value = list(value)
            output.append([name, value])
        except (IndexError, StopIteration, TypeError, ValueError) as error:
            output.append([name, type(error).__name__])
    for length in range(6):
        values = list(range(length))
        for n in [-1, 0, 1, 2, 3, 7]: record("adjacent_n_tuples", values, n)
        record("adjacent_pairs", tuple(values))
        for other in [[], [8], [8, 9, 10, 11]]:
            record("make_even", iter(values), iter(other))
            record("make_even_by_cycling", values, other)
            record("list_difference_update", values, other)
            record("list_update", values, other)
    for obj in ["ab", "", 1, None, [1, 2], (1, 2), {"x": 1}]:
        record("listify", obj);record("tuplify", obj)
    for name in ["listify", "tuplify"]: record(name, iter([1, 2]))
    record("all_elements_are_instances", iter([1, True]), int)
    record("all_elements_are_instances", [1, "x"], int)
    record("all_elements_are_instances", [], str)
    record("concatenate_lists", iter([1, 2]), [3], ())
    record("remove_nones", [None, 0, False, [], "", 1, "a"])
    record("remove_list_redundancies", [1, 2, 1, 3, 2])
    record("remove_list_redundancies", [[], []])
    record("list_update", [1, 2, 3], iter([2, 4, 4]))
    marker = object()
    first, second = namespace["make_even"]([marker], [1, 2, 3])
    output.append(["repeated_identity", all(value is marker for value in first), second])
    return output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    source = args.manim_source / "manim/utils/iterables.py"
    expected = observations(load_helpers(source, NAMES, {"it": itertools, "Iterable": Iterable, "overload": overload}))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    actual = observations({name: getattr(noon, name) for name in NAMES})
    errors = [] if actual == expected else ["iterable helper observations differ"]
    if args.installed_manim:
        from manim.utils import iterables
        if Path(iterables.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed iterable utility source differs from pin")
        if observations(vars(iterables)) != expected: errors.append("full installed module differs")
    for name in NAMES:
        if name not in noon.__all__: errors.append(f"{name}: missing export")
    print(json.dumps({"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "symbols": len(NAMES), "observations": len(expected),
                      "installed_module_checked": args.installed_manim, "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
