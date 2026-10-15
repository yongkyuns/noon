"""Guard pinned Angle constructor metadata and from_three_points contracts."""
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


def _constructor(tree):
    angle = next(
        node for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == "Angle"
    )
    return next(
        node for node in angle.body
        if isinstance(node, ast.FunctionDef) and node.name == "__init__"
    )


def _radius_none_check(node):
    if not isinstance(node, ast.If):
        return False
    test = node.test
    return (
        isinstance(test, ast.Compare)
        and isinstance(test.left, ast.Name)
        and test.left.id == "radius"
        and len(test.ops) == 1
        and isinstance(test.ops[0], ast.Is)
        and len(test.comparators) == 1
        and isinstance(test.comparators[0], ast.Constant)
        and test.comparators[0].value is None
    )


def _stores_explicit_radius(node):
    targets = (
        node.targets if isinstance(node, ast.Assign)
        else [node.target] if isinstance(node, ast.AnnAssign)
        else []
    )
    return (
        any(
            isinstance(target, ast.Attribute)
            and isinstance(target.value, ast.Name)
            and target.value.id == "self"
            and target.attr == "radius"
            for target in targets
        )
        and isinstance(node.value, ast.Name)
        and node.value.id == "radius"
    )


def _parallel_empty_return(node):
    if not isinstance(node, ast.Try):
        return False
    for handler in node.handlers:
        if not (
            isinstance(handler.type, ast.Name)
            and handler.type.id == "ValueError"
        ):
            continue
        stores_zero_value = any(
            isinstance(statement, ast.Assign)
            and isinstance(statement.value, ast.Constant)
            and statement.value.value == 0.0
            and any(
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id == "self"
                and target.attr == "angle_value"
                for target in statement.targets
            )
            for statement in handler.body
        )
        returns_empty = any(isinstance(statement, ast.Return) for statement in handler.body)
        if stores_zero_value and returns_empty:
            return True
    return False


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
    constructor = _constructor(tree)
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

    radius_checks = [node for node in constructor.body if _radius_none_check(node)]
    parallel_return_indices = [
        index for index, node in enumerate(constructor.body)
        if _parallel_empty_return(node)
    ]
    explicit_radius_assignments = [
        node for node in ast.walk(constructor) if _stores_explicit_radius(node)
    ]
    conditional_assignments = (
        [
            node
            for statement in radius_checks[0].orelse
            for node in ast.walk(statement)
            if _stores_explicit_radius(node)
        ]
        if len(radius_checks) == 1
        else []
    )
    radius_attribute_checked = (
        len(radius_checks) == 1
        and len(explicit_radius_assignments) == 1
        and len(conditional_assignments) == 1
    )
    if not radius_attribute_checked:
        errors.append("pinned explicit-radius attribute assignment differs")
    parallel_empty_return_checked = (
        len(parallel_return_indices) == 1
        and len(radius_checks) == 1
        and parallel_return_indices[0] < constructor.body.index(radius_checks[0])
    )
    if not parallel_empty_return_checked:
        errors.append("pinned parallel-line empty return no longer precedes radius storage")

    if installed_manim:
        module = importlib.import_module("manim.mobject.geometry.line")
        if Path(module.__file__).read_bytes() != data:
            raise ValueError("installed complete Manim line module differs from pin")

    return {
        "source_sha256": hashlib.sha256(data).hexdigest(),
        "installed_module_checked": installed_manim,
        "explicit_radius_attribute_checked": radius_attribute_checked,
        "parallel_empty_return_checked": parallel_empty_return_checked,
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
