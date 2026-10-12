#!/usr/bin/env python3
"""Compare lightweight namespace values with untouched pinned Manim source."""
import argparse
import ast
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FONT_NAMES = set("NORMAL ITALIC OBLIQUE BOLD THIN ULTRALIGHT LIGHT SEMILIGHT BOOK MEDIUM SEMIBOLD ULTRABOLD HEAVY ULTRAHEAVY".split())


def literal(node):
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Div):
        return ast.literal_eval(node.left) / ast.literal_eval(node.right)
    return ast.literal_eval(node)


def expected_values(source_root):
    colors = {}
    constants = {}
    axes = {}
    for node in ast.parse((source_root / "manim/utils/color/manim_colors.py").read_text()).body:
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Call):
            call = node.value
            if isinstance(call.func, ast.Name) and call.func.id == "ManimColor" and len(call.args) == 1:
                for target in node.targets:
                    if isinstance(target, ast.Name):
                        colors[target.id] = int(literal(call.args[0]).removeprefix("#"), 16)
    for node in ast.parse((source_root / "manim/constants.py").read_text()).body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if not isinstance(target, ast.Name):
                    continue
                name = target.id
                if name in FONT_NAMES or name == "SCALE_FACTOR_PER_FONT_POINT" or (
                    name.startswith("DEFAULT_") and name != "DEFAULT_QUALITY" and isinstance(node.value, ast.Constant)
                ):
                    constants[name] = literal(node.value)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            if node.target.id in {"X_AXIS", "Y_AXIS", "Z_AXIS"}:
                axes[node.target.id] = tuple(literal(node.value.args[0]))
    if not colors or not constants or len(axes) != 3:
        raise ValueError("pinned namespace source contracts are incomplete")
    return colors, constants, axes


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(ROOT / "web/python"))
    import noon
    import _manim_namespace
    _manim_namespace.install()
    colors, constants, axes = expected_values(args.manim_source)
    errors = []
    for name in colors.keys() | constants.keys() | axes.keys():
        if name not in noon.__all__:
            errors.append(f"{name}: missing public export")
            continue
        value = getattr(noon, name)
        if name in colors:
            encoded = colors[name]
            expected = tuple(((encoded >> shift) & 255) / 255 for shift in (16, 8, 0)) + (1.0,)
            actual = (value.red, value.green, value.blue, value.alpha)
        elif name in axes:
            expected = axes[name]
            actual = tuple(value)
            if len(actual) == 2:
                actual += (0.0,)
        else:
            expected = constants[name]
            actual = value
        if actual != expected:
            errors.append(f"{name}: {actual!r} differs from pinned {expected!r}")
    print(json.dumps({"colors": len(colors), "scalar_constants": len(constants), "axis_aliases": len(axes), "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
