"""Load unchanged selected utility definitions without unrelated engine imports."""
import ast


def load_helpers(source, names, dependencies):
    tree = ast.parse(source.read_text(), filename=str(source))
    nodes = [node for node in tree.body if isinstance(node, (ast.ClassDef, ast.FunctionDef))
             and node.name in names]
    if {node.name for node in nodes} != set(names):
        raise ValueError("pinned helper definitions are incomplete")
    future = ast.parse("from __future__ import annotations").body[0]
    namespace = {"__name__": "_pinned_python_helpers", **dependencies}
    exec(compile(ast.Module(body=[future, *nodes], type_ignores=[]), str(source), "exec"), namespace)
    return namespace
