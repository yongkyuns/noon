#!/usr/bin/env python3
"""Compare lightweight namespace values with untouched pinned Manim source."""
import argparse
import ast
import hashlib
import json
import runpy
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


def expected_enums(source_root):
    result = {}
    for filename in ("manim/constants.py", "manim/scene/section.py"):
        for node in ast.parse((source_root / filename).read_text()).body:
            if not isinstance(node, ast.ClassDef) or node.name not in {
                "RendererType", "LineJointType", "CapStyleType", "DefaultSectionType"
            }:
                continue
            members = {}
            for statement in node.body:
                if isinstance(statement, ast.Assign):
                    for target in statement.targets:
                        if isinstance(target, ast.Name):
                            members[target.id] = literal(statement.value)
            result[node.name] = (ast.unparse(node.bases[0]), members)
    if len(result) != 4 or any(not members for _, members in result.values()):
        raise ValueError("pinned enum source contracts are incomplete")
    return result


def qualify_standard_library(source_root, *, installed_manim=False):
    """Preserve the exact host aliases exported by the pinned package entrypoint."""
    import __future__
    import importlib.metadata as metadata
    import inspect
    import tempfile
    from unittest.mock import patch
    import noon

    source = source_root / "manim/__init__.py"
    imports = {(node.module, alias.name, alias.asname)
               for node in ast.parse(source.read_text()).body
               if isinstance(node, ast.ImportFrom) for alias in node.names}
    aliases = {"annotations": __future__.annotations,
               "PackageNotFoundError": metadata.PackageNotFoundError,
               "version": metadata.version}
    expected_imports = {("__future__", "annotations", None),
                        ("importlib.metadata", "PackageNotFoundError", None),
                        ("importlib.metadata", "version", None)}
    if not expected_imports <= imports:
        raise ValueError("pinned standard-library namespace imports changed")

    def observe(namespace):
        result = [[name, namespace[name] is expected]
                  for name, expected in aliases.items()]
        result.append(["version_signature", str(inspect.signature(namespace["version"]))])
        feature = namespace["annotations"]
        result.append(["future_feature", feature.getOptionalRelease(),
                       feature.getMandatoryRelease(), feature.compiler_flag])
        with tempfile.TemporaryDirectory() as directory:
            info = Path(directory) / "noon_namespace_fixture-1.2.3.dist-info"
            info.mkdir()
            (info / "METADATA").write_text(
                "Metadata-Version: 2.1\nName: noon-namespace-fixture\nVersion: 1.2.3\n")
            with patch.object(sys, "path", [directory, *sys.path]):
                for name in ["noon-namespace-fixture", "noon_namespace_fixture"]:
                    result.append(["distribution", name, namespace["version"](name)])
                try:
                    namespace["version"]("noon-namespace-absent")
                except namespace["PackageNotFoundError"] as error:
                    result.append(["missing_distribution", error.name, str(error)])
                else:
                    result.append(["missing_distribution", "did not raise"])
        return result

    expected = observe(aliases)
    actual = observe({name: getattr(noon, name) for name in aliases})
    errors = [] if actual == expected else ["standard-library namespace observations differ"]
    errors.extend(f"{name}: missing public export" for name in aliases if name not in noon.__all__)
    if installed_manim:
        import manim
        if Path(manim.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed Manim package entrypoint differs from pin")
        if observe(vars(manim)) != expected:
            errors.append("installed Manim namespace aliases differ")
    return {"symbols": len(aliases), "observations": len(expected),
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "installed_module_checked": installed_manim, "errors": errors}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
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
    from enum import Enum, StrEnum
    enums = expected_enums(args.manim_source)
    for name, (base, members) in enums.items():
        if name not in noon.__all__:
            errors.append(f"{name}: missing public export")
            continue
        enum = getattr(noon, name)
        if enum.__name__ != name or enum.__bases__ != ((StrEnum if base == "StrEnum" else Enum),):
            errors.append(f"{name}: incorrect Python enum type")
            continue
        actual = {key: value.value for key, value in enum.__members__.items()}
        if actual != members:
            errors.append(f"{name}: {actual!r} differs from pinned {members!r}")
    class_queries = runpy.run_path(
        str(Path(__file__).with_name("manim-class-queries.py")),
        run_name="_class_query_qualification",
    )["qualify"](args.manim_source)
    errors.extend(class_queries["errors"])
    standard_library = qualify_standard_library(args.manim_source, installed_manim=args.installed_manim)
    errors.extend(standard_library["errors"])
    color_palettes = runpy.run_path(
        str(Path(__file__).with_name("manim-color-palettes.py")),
        run_name="_color_palette_qualification",
    )["qualify"](args.manim_source)
    errors.extend(color_palettes["errors"])
    print(json.dumps({"colors": len(colors), "scalar_constants": len(constants), "axis_aliases": len(axes),
                      "enum_types": len(enums), "enum_members": sum(len(members) for _, members in enums.values()),
                      "class_queries": class_queries, "standard_library": standard_library,
                      "color_palettes": color_palettes,
                      "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
