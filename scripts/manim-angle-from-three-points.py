"""Guard the pinned Angle.from_three_points delegation contract."""
import argparse
import ast
import hashlib
import importlib
from pathlib import Path


def _method(tree):
    angle = next(
        node for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == "Angle"
    )
    return next(
        node for node in angle.body
        if isinstance(node, ast.FunctionDef) and node.name == "from_three_points"
    )


def _expected_return():
    return ast.Return(
        value=ast.Call(
            func=ast.Name(id="Angle", ctx=ast.Load()),
            args=[
                ast.Call(
                    func=ast.Name(id="Line", ctx=ast.Load()),
                    args=[ast.Name(id="B", ctx=ast.Load()), ast.Name(id="A", ctx=ast.Load())],
                    keywords=[],
                ),
                ast.Call(
                    func=ast.Name(id="Line", ctx=ast.Load()),
                    args=[ast.Name(id="B", ctx=ast.Load()), ast.Name(id="C", ctx=ast.Load())],
                    keywords=[],
                ),
            ],
            keywords=[ast.keyword(arg=None, value=ast.Name(id="kwargs", ctx=ast.Load()))],
        )
    )


def qualify(manim_source, *, installed_manim=False):
    source = Path(manim_source) / "manim/mobject/geometry/line.py"
    data = source.read_bytes()
    tree = ast.parse(data, filename=str(source))
    method = _method(tree)
    decorators = [
        node.id for node in method.decorator_list if isinstance(node, ast.Name)
    ]
    positional = method.args.posonlyargs + method.args.args
    errors = []
    if decorators != ["staticmethod"]:
        errors.append("pinned method is not a staticmethod")
    if [argument.arg for argument in positional] != ["A", "B", "C"]:
        errors.append("pinned positional parameters differ")
    if method.args.kwarg is None or method.args.kwarg.arg != "kwargs":
        errors.append("pinned keyword forwarding boundary differs")
    returns = [node for node in method.body if isinstance(node, ast.Return)]
    if len(returns) != 1 or ast.dump(returns[0]) != ast.dump(_expected_return()):
        errors.append("pinned Line/Angle delegation differs")

    if installed_manim:
        module = importlib.import_module("manim.mobject.geometry.line")
        if Path(module.__file__).read_bytes() != data:
            raise ValueError("installed complete Manim line module differs from pin")

    return {
        "source_sha256": hashlib.sha256(data).hexdigest(),
        "installed_module_checked": installed_manim,
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
