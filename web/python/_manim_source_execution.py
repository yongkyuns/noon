"""Compile portable host continuations, without changing scene semantics.

The original module is executed once. A second, never-executed module compilation
supplies coroutine code for ordinary ``construct`` methods with direct, statement-
position ``self.play``/``self.wait`` calls. Binding that code reuses the original
function's globals, defaults and closure; class/decorator/module effects are not
replayed. All barriers still use the canonical Scene implementation and runtime.

This is deliberately not a general sync-to-async Python compiler. Indirect/helper
barriers, generators and custom Scene methods keep the original execution path.
"""

from __future__ import annotations

import ast
import copy
import inspect
from types import CodeType, FunctionType, MethodType

BARRIER_GLOBAL = "_noon_await_source_barrier"

# Calls on the scene that cannot suspend the authoring continuation. Unknown
# methods (including super().construct()) are not silently converted.
_SCENE_CALLS = frozenset({"play", "wait", "add", "remove", "clear"})


class _ConstructBody(ast.NodeTransformer):
    def __init__(self, receiver: str):
        self.receiver = receiver
        self.barriers = 0
        self.unsupported = False
        self.allowed_receivers: set[int] = set()

    def visit_Expr(self, node: ast.Expr):
        call = node.value
        if (
            isinstance(call, ast.Call)
            and isinstance(call.func, ast.Attribute)
            and isinstance(call.func.value, ast.Name)
            and call.func.value.id == self.receiver
            and call.func.attr in {"play", "wait"}
        ):
            # Inspect arguments as usual, but admit this particular receiver and
            # barrier. A nested call to play/wait is not a statement barrier.
            self.allowed_receivers.add(id(call.func.value))
            for arg in call.args:
                self.visit(arg)
            for keyword in call.keywords:
                self.visit(keyword.value)
            self.barriers += 1
            wrapped = ast.Call(
                func=ast.Name(id=BARRIER_GLOBAL, ctx=ast.Load()),
                args=[call.func, *call.args],
                keywords=call.keywords,
            )
            return ast.copy_location(
                ast.Expr(value=ast.copy_location(ast.Await(value=wrapped), call)), node
            )
        return self.generic_visit(node)

    def visit_Attribute(self, node: ast.Attribute):
        if isinstance(node.value, ast.Name) and node.value.id == self.receiver:
            if (node.attr.startswith("__") or
                    (node.attr in _SCENE_CALLS and isinstance(node.ctx, (ast.Store, ast.Del)))):
                self.unsupported = True
            if node.attr in {"play", "wait"}:
                # Aliasing, storing, passing, or using a barrier's return value
                # requires the original synchronous/explicit async execution.
                self.unsupported = True
            else:
                self.allowed_receivers.add(id(node.value))
        return self.generic_visit(node)

    def visit_Call(self, node: ast.Call):
        func = node.func
        if isinstance(func, ast.Name) and func.id in {"super", "locals", "vars", "eval", "exec"}:
            self.unsupported = True
        if (isinstance(func, ast.Attribute) and isinstance(func.value, ast.Name)
                and func.value.id == self.receiver and func.attr not in _SCENE_CALLS):
            self.unsupported = True
        return self.generic_visit(node)

    def visit_Name(self, node: ast.Name):
        # Passing/rebinding the scene can hide a synchronous barrier in a helper.
        if node.id == self.receiver and id(node) not in self.allowed_receivers:
            self.unsupported = True
        return node

    def visit_Lambda(self, node):
        self.unsupported = True
        return node

    def visit_FunctionDef(self, node):
        self.unsupported = True
        return node

    visit_AsyncFunctionDef = visit_FunctionDef
    visit_ClassDef = visit_FunctionDef

    def visit_Yield(self, node):
        self.unsupported = True
        return node

    visit_YieldFrom = visit_Yield


class _ConstructCompiler(ast.NodeTransformer):
    def __init__(self):
        self.locations: set[tuple[str, int]] = set()

    def visit_FunctionDef(self, node: ast.FunctionDef):
        if node.name != "construct":
            return self.generic_visit(node)
        arguments = [*node.args.posonlyargs, *node.args.args]
        if not arguments or node.decorator_list:
            return node
        body = _ConstructBody(arguments[0].arg)
        candidate = copy.deepcopy(node)
        candidate.body = [body.visit(statement) for statement in candidate.body]
        if body.unsupported or not body.barriers:
            return node
        self.locations.add((node.name, node.lineno))
        return ast.copy_location(ast.AsyncFunctionDef(**vars(candidate)), node)

    def visit_AsyncFunctionDef(self, node):
        # Explicitly async source is already correct. Never double-await it.
        return node


def _function_codes(code: CodeType):
    for value in code.co_consts:
        if isinstance(value, CodeType):
            yield value
            yield from _function_codes(value)


def compile_authoring_source(
    source: str, filename: str = "<string>", *, portable: bool = True
) -> tuple[CodeType, dict[CodeType, CodeType]]:
    """Return original module code and optional portable construct code pairs."""
    original = compile(source, filename, "exec", dont_inherit=True)
    # Static, explicitly async and export-only source takes the original compiler
    # path. Inspect immutable code metadata before allocating any Python AST.
    if not portable or BARRIER_GLOBAL in source:
        return original, {}
    originals = list(_function_codes(original))
    if not any(
        code.co_name == "construct"
        and not code.co_flags & (inspect.CO_COROUTINE | inspect.CO_GENERATOR)
        and {"play", "wait"}.intersection(code.co_names)
        for code in originals
    ):
        return original, {}
    tree = ast.parse(source, filename=filename, mode="exec")
    compiler = _ConstructCompiler()
    # The original code has already been compiled. Only candidate construct
    # bodies need copying for conservative rejection; never clone the module.
    candidate = compiler.visit(tree)
    if not compiler.locations:
        return original, {}
    portable = compile(ast.fix_missing_locations(candidate), filename, "exec", dont_inherit=True)
    portable_codes = {
        (code.co_qualname, code.co_firstlineno): code
        for code in _function_codes(portable)
        if code.co_flags & inspect.CO_COROUTINE
    }
    pairs = {}
    for code in originals:
        if (code.co_name, code.co_firstlineno) not in compiler.locations:
            continue
        replacement = portable_codes.get((code.co_qualname, code.co_firstlineno))
        if replacement is not None and replacement.co_freevars == code.co_freevars:
            pairs[code] = replacement
    return original, pairs


def bind_portable_construct(
    method: object, code_pairs: dict[CodeType, CodeType]
) -> MethodType | None:
    """Bind only the selected original function, not a decorator or another scene."""
    if not isinstance(method, MethodType) or not isinstance(method.__func__, FunctionType):
        return None
    original = method.__func__
    code = code_pairs.get(original.__code__)
    if code is None:
        return None
    portable = FunctionType(
        code, original.__globals__, original.__name__, original.__defaults__, original.__closure__
    )
    portable.__kwdefaults__ = original.__kwdefaults__
    return MethodType(portable, method.__self__)


def has_portable_scene_methods(scene: object, **methods: FunctionType) -> bool:
    """Admit only ordinary method lookup, without invoking authored descriptors.

    Class-only checks miss instance overrides installed by setup(). Dynamic
    attribute lookup and descriptors retain their original synchronous behavior.
    """
    return (
        type(scene).__getattribute__ is object.__getattribute__
        and all(inspect.getattr_static(scene, name, None) is method
                for name, method in methods.items())
    )
