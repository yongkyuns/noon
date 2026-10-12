"""Compare dictionary helper observations with unchanged pinned Manim bodies."""
import argparse
import ast
import hashlib
import itertools
import json
from pathlib import Path
import random
import sys


NAMES = {"DictAsObject", "merge_dicts_recursively", "update_dict_recursively"}


def pinned_helpers(source):
    tree = ast.parse(source.read_text(), filename=str(source))
    nodes = [node for node in tree.body if isinstance(node, (ast.ClassDef, ast.FunctionDef))
             and node.name in NAMES]
    if {node.name for node in nodes} != NAMES:
        raise ValueError("pinned dictionary helper definitions are incomplete")
    # Keep the original definitions and source locations. Only their unrelated
    # module dependencies are omitted; annotations are postponed as upstream.
    future = ast.parse("from __future__ import annotations").body[0]
    module = ast.Module(body=[future, *nodes], type_ignores=[])
    namespace = {"__name__": "_pinned_config_helpers", "it": itertools}
    exec(compile(module, str(source), "exec"), namespace)
    return namespace


def observations(namespace):
    merge, update, wrapper = (namespace[name] for name in
                              ("merge_dicts_recursively", "update_dict_recursively", "DictAsObject"))
    result = [merge()]
    rng = random.Random(20261012)
    def mapping(depth):
        return {key: mapping(depth - 1) if depth and rng.randrange(2) else rng.randrange(100)
                for key in rng.sample(["a", "b", "c", "d"], rng.randrange(5))}
    for _ in range(64):
        inputs = [mapping(3) for _ in range(3)]
        before = json.dumps(inputs, sort_keys=True)
        result.append([merge(*inputs), json.dumps(inputs, sort_keys=True) == before])
    child, untouched = {"left": 1}, [2]
    first = {"nested": child, "untouched": untouched}
    merged = merge(first, {"nested": {"right": 3}})
    result.append([merged, first, merged["nested"] is child, merged["untouched"] is untouched])
    result.append([update(first, {"nested": {"right": 4}}), first, child,
                   first["nested"] is child, first["untouched"] is untouched])
    calls = []
    class Recorded(dict):
        def items(self):
            calls.append([self["key"], "items"])
            def entries():
                calls.append([self["key"], "iterate"])
                yield from super(Recorded, self).items()
            return entries()
    result.append([merge(Recorded(key=1), Recorded(key=2)), calls])
    values = {"x": 1}
    obj = wrapper(dictin=values)
    identical = obj.__dict__ is values
    obj.x = 3
    values["y"] = 4
    result.append([identical, obj.x, obj.y, values.copy()])
    del obj.x
    result.append(values.copy())
    for operation in [lambda: obj.x, lambda: wrapper([]), lambda: merge([])]:
        try:
            operation()
            result.append("no exception")
        except (AttributeError, TypeError) as error:
            result.append(type(error).__name__)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true",
                        help="also compare the installed full Manim module, requiring identical source")
    args = parser.parse_args()
    source = args.manim_source / "manim/utils/config_ops.py"
    expected = observations(pinned_helpers(source))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    actual = observations({name: getattr(noon, name) for name in NAMES})
    errors = [] if actual == expected else ["dictionary helper observations differ"]
    for name in NAMES:
        if name not in noon.__all__:
            errors.append(f"{name}: missing public export")
    if args.installed_manim:
        from manim.utils import config_ops
        if Path(config_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed Manim utility source differs from the pinned checkout")
        if observations(vars(config_ops)) != expected:
            errors.append("installed full Manim module differs from pinned-body observations")
    print(json.dumps({"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "symbols": len(NAMES), "observations": len(expected),
                      "installed_module_checked": args.installed_manim, "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
