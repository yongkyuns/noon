"""Qualify Python iterable helpers against untouched ManimCE definitions."""
import argparse
from collections.abc import Iterable
import hashlib
import itertools
import inspect
import json
from pathlib import Path
import sys
from typing import overload
from types import GeneratorType

from pinned_python_helpers import load_helpers

NAMES = {"adjacent_n_tuples", "adjacent_pairs", "all_elements_are_instances", "concatenate_lists",
         "list_difference_update", "list_update", "listify", "tuplify", "make_even",
         "make_even_by_cycling", "remove_list_redundancies", "remove_nones",
         "batch_by_property", "uniq_chain", "hash_obj"}


def observations(namespace):
    output = []
    def record(name, *args):
        try:
            value = namespace[name](*args)
            if isinstance(value, (zip, GeneratorType)): value = list(value)
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
    for name in ("batch_by_property", "uniq_chain", "hash_obj"):
        output.append(["signature", name, str(inspect.signature(namespace[name]))])
    for values in ([], [None], [1, 1, 2, 1], ["a", "ab", "cd", "e"]):
        calls = []
        def property_func(value):
            calls.append(value)
            return len(value) if isinstance(value, str) else value
        record("batch_by_property", iter(values), property_func)
        output.append(["batch_callback_order", calls])
    record("uniq_chain", [], [1, 2, 1], iter([2, 3, 3]), (4,))
    events = []
    def stream():
        for value in [1, 1, 2]:
            events.append(value)
            yield value
    unique = namespace["uniq_chain"](stream())
    output.append(["unique_lazy_creation", events.copy()])
    output.append(["unique_first", next(unique), events.copy()])
    output.append(["unique_second", next(unique), events.copy()])
    output.append(["unique_exhausted", list(unique), events.copy()])
    events.clear()
    class Collision:
        def __init__(self, name): self.name = name
        def __hash__(self): events.append(["hash", self.name]); return 19
        def __eq__(self, other):
            events.append(["equal", self.name, other.name]); return self.name == other.name
    first, equal, other = Collision("a"), Collision("a"), Collision("b")
    result = list(namespace["uniq_chain"]([first, equal, other]))
    output.append(["unique_collisions", [v.name for v in result],
                   result[0] is first, events.copy()])
    def fail(value): raise ValueError("user callback")
    record("batch_by_property", [1], fail)
    for value in (None, True, 7, -7, 1.25, "text", (), [], {}, set(),
                  [1, [2, 3]], (1, (2, 3)), {"b": [2], "a": {3, 4}},
                  {1, 2, 3}, frozenset({1, 2})):
        record("hash_obj", value)
    output.append(["hash_order_relations",
                   namespace["hash_obj"]({"a": 1, "b": 2}) == namespace["hash_obj"]({"b": 2, "a": 1}),
                   namespace["hash_obj"]([1, 2]) == namespace["hash_obj"]((1, 2)),
                   namespace["hash_obj"]([1, 2]) != namespace["hash_obj"]([2, 1])])
    events.clear()
    class UserHash:
        def __hash__(self): events.append("hash"); return 17
    value = UserHash()
    record("hash_obj", [value, {value: value}])
    output.append(["custom_hash_dispatch", events.copy()])
    class BrokenHash:
        def __hash__(self): raise ValueError("user hash")
    record("hash_obj", [BrokenHash()])
    record("hash_obj", bytearray(b"x"))
    record("uniq_chain", [[1]])
    cyclic = []; cyclic.append(cyclic)
    try: namespace["hash_obj"](cyclic)
    except RecursionError: output.append(["cyclic_hash", "RecursionError"])
    else: output.append(["cyclic_hash", "missing error"])
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
