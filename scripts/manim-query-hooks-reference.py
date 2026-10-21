"""Capture independent length/split/predicate dispatch from pinned ManimCE."""
import argparse
import ast
import hashlib
import inspect
import json
from pathlib import Path
import manim as m

KINDS = ("Mobject", "VMobject", "Circle", "Square", "VectorizedPoint", "Group", "VGroup")
HOOKS = ("split_returns", "split_raises", "has_points_false", "has_points_true", "has_points_raises")
OPERATIONS = ("length", "split", "iteration", "index")


def main():
    assert m.__version__ == "0.21.0"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--probe", type=Path, default=Path(__file__).resolve().parents[1] / "web/python/examples/manim_parity_query_hooks.py")
    args = parser.parse_args()
    source = args.probe.read_text()
    nodes = [node for node in ast.parse(source).body
             if isinstance(node, ast.FunctionDef) and node.name == "observe_query_hook"]
    assert len(nodes) == 1
    namespace = {}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(args.probe), "exec"), namespace)
    observe = namespace["observe_query_hook"]
    cases = [observe(m, {"class": kind, "ownPoints": own, "hook": hook, "operation": operation})
             for kind in KINDS for own in (0, 4) for hook in HOOKS for operation in OPERATIONS]
    assert len(cases) == 280
    for row in cases:
        spec = row["spec"]
        if spec["hook"].startswith("split_") and spec["operation"] != "split":
            assert row["events"] == [], row
        if spec["hook"].startswith("has_points_") and spec["operation"] == "split":
            assert row["events"] == [], row
    result = {"schemaVersion": 1, "manimVersion": m.__version__,
              "scope": "Public own0/4 queries across seven classes: split override never influences inherited len/iteration/index; intrinsic split bypasses has_points overrides; predicate events and errors remain visible in inherited len/iteration/index.",
              "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "probeSha256": hashlib.sha256(source.encode()).hexdigest(),
              "methodSourceSha256": {name: hashlib.sha256(inspect.getsource(getattr(m.Mobject, name)).encode()).hexdigest()
                                     for name in ("__len__", "__iter__", "__getitem__", "split", "has_points", "has_no_points")},
              "cases": cases}
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print("Observed 280 pinned query-hook controls")


if __name__ == "__main__":
    main()
