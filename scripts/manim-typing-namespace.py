"""Qualify the lazy type namespace against the complete pinned Manim module."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import runpy
import sys
import typing as python_typing


def observations(namespace, names):
    def describe(value):
        origin = python_typing.get_origin(value)
        arguments = python_typing.get_args(value)
        return [repr(value), type(value).__name__, repr(origin), [repr(x) for x in arguments]]
    result = [[name, describe(namespace[name])] for name in names]
    result.extend(["identity", first, second, namespace[first] is namespace[second]]
                  for index, first in enumerate(names) for second in names[index:])
    return result


def qualify(manim_source, *, installed_manim=False):
    source = manim_source / "manim/typing.py"
    expected_module = runpy.run_path(str(source), run_name="_pinned_manim_typing")
    names = expected_module["__all__"]
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_typing
    import noon
    actual = {name: getattr(_manim_typing, name) for name in names}
    expected = observations(expected_module, names)
    errors = [] if observations(actual, names) == expected else ["type alias observations differ"]
    if _manim_typing.__all__ != names or noon.typing is not _manim_typing:
        errors.append("public type namespace differs")
    if installed_manim:
        import manim.typing as original
        if Path(original.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed full typing module differs from pinned source")
        if observations(vars(original), names) != expected:
            errors.append("installed complete module observations differ")
    original_tree = ast.parse(source.read_text())
    actual_tree = ast.parse((Path(__file__).resolve().parents[1] / "web/python/_manim_typing_values.py").read_text())
    original_values = {node.target.id: ast.dump(node.value) for node in original_tree.body
                       if isinstance(node, ast.AnnAssign)}
    actual_values = {node.targets[0].id: ast.dump(node.value) for node in actual_tree.body
                     if isinstance(node, ast.Assign)}
    if actual_values != original_values:
        errors.append("pinned alias expressions differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "aliases": len(names), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.manim_source, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
