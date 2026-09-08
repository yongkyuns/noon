"""Compile portable host continuations, without changing scene semantics.

The module namespace and all definition effects execute exactly once. Direct
module-level play/wait statements use top-level await without an async-function
wrapper that would turn globals into locals. The deferred callbacks themselves
stay synchronous and are never rewritten.

The original construct definitions are retained. A second, never-executed module compilation
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
from contextlib import ExitStack, contextmanager
from contextvars import ContextVar
from dataclasses import dataclass
from types import CodeType, FunctionType, MethodType

BARRIER_GLOBAL = "_noon_await_source_barrier"
MODULE_BARRIER_GLOBAL = "_noon_await_module_barrier"


@dataclass
class _SourceInvocation:
    export_document: bool
    cleanup: ExitStack


_SOURCE_INVOCATION: ContextVar[_SourceInvocation | None] = ContextVar(
    "noon_source_invocation", default=None
)


def current_source_invocation():
    return _SOURCE_INVOCATION.get()


@contextmanager
def authoring_source_scope(*, export_document: bool = False):
    """Scope top-level host execution and restore continuation flags on exit.

    Scene state and timing stay in Rust. The scope carries invocation mode and
    host cleanup only; construct dispatch retains its portable/async handling.
    """
    with ExitStack() as cleanup:
        token = _SOURCE_INVOCATION.set(_SourceInvocation(export_document, cleanup))
        cleanup.callback(_SOURCE_INVOCATION.reset, token)
        yield

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

    def _visit_callback(self, node):
        # Deferred callback code stays synchronous, with its original closure,
        # defaults and identity. Never insert awaits into the callback itself.
        # Reject access to the scene or hidden barriers/introspection rather
        # than pretending an arbitrary synchronous helper can suspend.
        for child in ast.walk(node):
            if (isinstance(child, ast.Name) and child.id == self.receiver
                    or isinstance(child, ast.Attribute) and child.attr in {"play", "wait"}
                    or isinstance(child, (ast.Await, ast.Yield, ast.YieldFrom, ast.AsyncFunctionDef, ast.ClassDef))
                    or isinstance(child, ast.Call) and isinstance(child.func, ast.Name)
                    and child.func.id in {"super", "locals", "globals", "vars", "eval", "exec"}
                    or isinstance(child, ast.FunctionDef) and child.decorator_list):
                self.unsupported = True
                break
        return node

    visit_Lambda = _visit_callback
    visit_FunctionDef = _visit_callback

    def visit_AsyncFunctionDef(self, node):
        self.unsupported = True
        return node

    visit_ClassDef = visit_AsyncFunctionDef

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


class _ModuleBarriers(ast.NodeTransformer):
    """Yield at direct module statements, never inside deferred function code.

    Runtime dispatch invokes noncanonical methods normally and only consumes
    the engine's own continuation awaitable. An arbitrary awaitable returned by
    user code must not acquire new await semantics.
    """
    def __init__(self):
        self.barriers = 0

    def visit_Expr(self, node):
        call = node.value
        if (isinstance(call, ast.Call) and isinstance(call.func, ast.Attribute)
                and isinstance(call.func.value, ast.Name)
                and call.func.attr in {"play", "wait"}):
            self.barriers += 1
            wrapped = ast.Call(
                func=ast.Name(id=MODULE_BARRIER_GLOBAL, ctx=ast.Load()),
                args=[call.func, *call.args], keywords=call.keywords,
            )
            return ast.copy_location(
                ast.Expr(value=ast.copy_location(ast.Await(value=wrapped), call)), node
            )
        return node

    def visit_FunctionDef(self, node):
        return node

    visit_AsyncFunctionDef = visit_FunctionDef
    visit_ClassDef = visit_FunctionDef
    visit_Lambda = visit_FunctionDef


def compile_authoring_source(
    source: str, filename: str = "<string>", *, portable: bool = True
) -> tuple[CodeType, dict[CodeType, CodeType]]:
    """Return executable module code and optional portable construct code pairs."""
    original = compile(source, filename, "exec", dont_inherit=True)
    if not portable or any(name in source for name in (BARRIER_GLOBAL, MODULE_BARRIER_GLOBAL)):
        return original, {}
    originals = list(_function_codes(original))
    has_construct = any(
        code.co_name == "construct"
        and not code.co_flags & (inspect.CO_COROUTINE | inspect.CO_GENERATOR)
        and {"play", "wait"}.intersection(code.co_names)
        for code in originals
    )
    has_module_barrier = bool({"play", "wait"}.intersection(original.co_names))
    # Static, explicit-async and export-only source retains the fast path.
    if not has_construct and not has_module_barrier:
        return original, {}
    tree = ast.parse(source, filename=filename, mode="exec")
    module_code = original
    if has_module_barrier:
        module_compiler = _ModuleBarriers()
        module_tree = module_compiler.visit(tree)
        if module_compiler.barriers:
            module_code = compile(
                ast.fix_missing_locations(module_tree), filename, "exec",
                flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT, dont_inherit=True,
            )
    if not has_construct:
        return module_code, {}
    compiler = _ConstructCompiler()
    candidate = compiler.visit(tree)
    if not compiler.locations:
        return module_code, {}
    portable_code = compile(
        ast.fix_missing_locations(candidate), filename, "exec",
        flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT, dont_inherit=True,
    )
    portable_codes = {
        (code.co_qualname, code.co_firstlineno): code
        for code in _function_codes(portable_code)
        if code.co_flags & inspect.CO_COROUTINE
    }
    pairs = {}
    for code in originals:
        if (code.co_name, code.co_firstlineno) not in compiler.locations:
            continue
        replacement = portable_codes.get((code.co_qualname, code.co_firstlineno))
        if replacement is not None and replacement.co_freevars == code.co_freevars:
            pairs[code] = replacement
    return module_code, pairs


async def execute_authoring_module(code: CodeType, namespace: dict) -> None:
    """Execute the module once in its original namespace, awaiting only module code."""
    if code.co_flags & inspect.CO_COROUTINE:
        await eval(code, namespace)
    else:
        exec(code, namespace)


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
