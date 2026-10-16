"""Guard pinned regular-polygon constructor start-angle metadata."""
import argparse
import ast
import hashlib
import importlib
from pathlib import Path


def _constructor(tree, name):
    cls = next(
        node for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == name
    )
    return next(
        node for node in cls.body
        if isinstance(node, ast.FunctionDef) and node.name == "__init__"
    )


def _call_name(node):
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return node.attr
    return None


def _stores_resolved_start_angle(constructor, function_name):
    for node in ast.walk(constructor):
        if not isinstance(node, ast.Assign) or not isinstance(node.value, ast.Call):
            continue
        stores_attribute = any(
            isinstance(target, (ast.Tuple, ast.List))
            and any(
                isinstance(item, ast.Attribute)
                and isinstance(item.value, ast.Name)
                and item.value.id == "self"
                and item.attr == "start_angle"
                for item in ast.walk(target)
            )
            or isinstance(target, ast.Attribute)
            and isinstance(target.value, ast.Name)
            and target.value.id == "self"
            and target.attr == "start_angle"
            for target in node.targets
        )
        if stores_attribute and _call_name(node.value.func) == function_name:
            return True
    return False


def qualify(manim_source, *, installed_manim=False):
    source = Path(manim_source) / "manim/mobject/geometry/polygram.py"
    data = source.read_bytes()
    tree = ast.parse(data, filename=str(source))
    regular = _stores_resolved_start_angle(
        _constructor(tree, "RegularPolygram"), "gen_polygon_vertices"
    )
    star = _stores_resolved_start_angle(
        _constructor(tree, "Star"), "regular_vertices"
    )
    errors = []
    if not regular:
        errors.append("pinned RegularPolygram resolved start-angle assignment differs")
    if not star:
        errors.append("pinned Star resolved start-angle assignment differs")

    if installed_manim:
        module = importlib.import_module("manim.mobject.geometry.polygram")
        if Path(module.__file__).read_bytes() != data:
            raise ValueError("installed complete Manim polygram module differs from pin")

    return {
        "source_sha256": hashlib.sha256(data).hexdigest(),
        "installed_module_checked": installed_manim,
        "regular_polygram_start_angle_checked": regular,
        "star_start_angle_checked": star,
        "errors": errors,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    report = qualify(args.manim_source, installed_manim=args.installed_manim)
    import json

    print(json.dumps(report, indent=2))
    raise SystemExit(bool(report["errors"]))
