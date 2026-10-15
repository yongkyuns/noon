"""Qualify pinned named RGB constants using the original Manim color constructor."""
import argparse
import ast
import hashlib
import importlib
import json
from pathlib import Path
import sys

NAMES = ("AS2700", "BS381", "DVIPSNAMES", "SVGNAMES", "X11", "XKCD")


def qualify(manim_source, *, installed_manim=False):
    pin = Path(manim_source)
    core = pin / "manim/utils/color/core.py"
    if installed_manim:
        original_core = importlib.import_module("manim.utils.color.core")
        ManimColor = original_core.ManimColor
        if Path(original_core.__file__).read_bytes() != core.read_bytes():
            raise ValueError("installed original ManimColor implementation differs from pin")
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import noon
    errors, modules, observations = [], {}, 0
    for name in NAMES:
        source = pin / f"manim/utils/color/{name}.py"
        tree = ast.parse(source.read_text(), filename=str(source))
        assignments = [node for node in tree.body if isinstance(node, ast.Assign)]
        for node in assignments:
            if not (len(node.targets) == 1 and isinstance(node.targets[0], ast.Name)
                    and isinstance(node.value, ast.Call)
                    and isinstance(node.value.func, ast.Name)
                    and node.value.func.id == "ManimColor"
                    and len(node.value.args) == 1 and not node.value.keywords
                    and isinstance(node.value.args[0], ast.Constant)):
                raise ValueError(f"{name}: unsupported pinned constant declaration")
        keys = [node.targets[0].id for node in assignments]
        encoded = {}
        for node in assignments:
            literal = node.value.args[0].value
            if not isinstance(literal, str) or len(literal) != 7 or not literal.startswith("#"):
                raise ValueError(f"{name}: unsupported pinned RGB literal")
            encoded[node.targets[0].id] = int(literal[1:], 16)
        if installed_manim:
            # Execute the unchanged declarations with the byte-verified original
            # constructor. Lightweight CI only needs the original literal facts.
            reference = {"ManimColor": ManimColor}
            exec(compile(ast.Module(body=assignments, type_ignores=[]), str(source), "exec"), reference)
        palette = getattr(noon, name)
        if tuple(palette.__all__) != tuple(keys): errors.append(f"{name}: constant names/order differ")
        for key in keys:
            value = getattr(palette, key)
            actual = (value.red, value.green, value.blue, value.alpha)
            expected = tuple(((encoded[key] >> shift) & 255) / 255 for shift in (16, 8, 0)) + (1.0,)
            if installed_manim and tuple(reference[key].to_rgba()) != expected:
                errors.append(f"{name}.{key}: original constructor differs from RGB literal")
            if actual != expected: errors.append(f"{name}.{key}: RGBA differs")
            if getattr(palette, key) is not value: errors.append(f"{name}.{key}: repeated identity differs")
            observations += 1
        if installed_manim:
            installed = importlib.import_module(f"manim.utils.color.{name}")
            if Path(installed.__file__).read_bytes() != source.read_bytes():
                raise ValueError(f"installed {name} module differs from pin")
            for key in keys:
                if tuple(getattr(installed, key).to_rgba()) != tuple(reference[key].to_rgba()):
                    errors.append(f"{name}.{key}: installed original differs")
        modules[name] = {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "constants": len(keys)}
    return {"modules": modules, "observations": observations,
            "core_sha256": hashlib.sha256(core.read_bytes()).hexdigest(),
            "installed_modules_checked": installed_manim,
            "original_constructor_checked": installed_manim, "errors": errors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    report = qualify(args.manim_source, installed_manim=args.installed_manim)
    print(json.dumps(report, indent=2))
    raise SystemExit(bool(report["errors"]))
