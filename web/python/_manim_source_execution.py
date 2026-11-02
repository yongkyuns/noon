"""Compile portable host continuations, without changing scene semantics.

The module namespace and all definition effects execute exactly once. Direct
module-level play/wait statements use top-level await without an async-function
wrapper that would turn globals into locals. The deferred callbacks themselves
stay synchronous and are never rewritten.

The original construct definitions are retained. A second, never-executed module compilation
supplies coroutine code for ordinary ``construct`` methods with direct statement
``self.play``/``self.wait`` barriers and resource-dependent effect/membership calls,
including assigned and chained results. Binding that code reuses the original
function's globals, defaults and closure; class/decorator/module effects are not
replayed. All barriers still use the canonical Scene implementation and runtime.

This is deliberately not a general sync-to-async Python compiler. Indirect/helper
barriers, generators and custom Scene methods keep the original execution path.
"""

from __future__ import annotations

import ast
import builtins
import copy
import inspect
import sys
from contextlib import ExitStack, contextmanager
from contextvars import ContextVar
from dataclasses import dataclass
from types import CodeType, FunctionType, MethodType

BARRIER_GLOBAL = "_noon_await_source_barrier"
MODULE_BARRIER_GLOBAL = "_noon_await_module_barrier"

# A closed source may use these trusted, scene/data-only entry points. This is
# deliberately a positive capability list, not a blacklist of known I/O calls.
# Unknown dispatch, descriptors and source hooks keep observed execution.
_SOURCE_CONSTRUCTORS = frozenset("""
Scene Circle Square Rectangle RoundedRectangle Triangle Polygon RegularPolygon
Dot Line Arrow DoubleArrow DashedLine VMobject VGroup Group Text Code MathTypst
Color Vec2 Vec3 VectorPath Create Write Uncreate FadeIn FadeOut Transform
ReplacementTransform Indicate GrowFromCenter AnimationGroup Succession LaggedStart
ValueTracker DecimalNumber Integer Pixels Glow Blur BackdropBlur GlowPulse
""".split())
_SOURCE_FUNCTIONS = frozenset("""
linear smooth rush_into rush_from there_and_back color_from_hex interpolate_color
always_redraw
""".split())
_SOURCE_CONSTANTS = frozenset("""
BLACK WHITE BLUE BLUE_A BLUE_B BLUE_C BLUE_D BLUE_E GREEN GREEN_A GREEN_B GREEN_C
GREEN_D GREEN_E RED RED_A RED_B RED_C RED_D RED_E YELLOW ORANGE PINK LIGHT_PINK
PURPLE PURPLE_A PURPLE_B PURPLE_C PURPLE_D PURPLE_E GREY GRAY GOLD TEAL MAROON
UP DOWN LEFT RIGHT IN OUT ORIGIN UL UR DL DR PI TAU DEGREES
""".split())
_SOURCE_BUILTINS = frozenset("""
abs all any bool bytes bytearray dict enumerate filter float frozenset int
isinstance len list map max min next range reversed round set slice sorted str
sum tuple zip ValueError RuntimeError
""".split())
_SOURCE_MEMBERS = frozenset("""
add remove clear play wait on_click set_drag_targets next_section
add_foreground_mobject add_foreground_mobjects bring_to_front bring_to_back replace
shift move_to scale rotate stretch to_edge to_corner next_to align_to arrange
set_fill set_stroke set_color set_opacity set_z_index set_glow remove_glow
add_effect remove_effect get_center get_x get_y get_width get_height get_value
set_value increment_value copy become add_updater remove_updater clear_updaters
start_new_path add_line_to add_cubic_bezier_curve_to close_path set_points_as_corners
animate id width height radius append extend insert pop count index get keys values
items update union intersection difference join split strip lower upper replace
startswith endswith encode decode hex fromhex tobytes reverse sort
""".split())
_SOURCE_MODULES = {
    "math": frozenset("""
        pi tau e inf nan sin cos tan asin acos atan atan2 sinh cosh tanh sqrt hypot
        exp log log2 log10 floor ceil trunc degrees radians copysign fabs fmod pow
        isfinite isnan isclose dist prod fsum
    """.split()),
    "colorsys": frozenset("hsv_to_rgb rgb_to_hsv hls_to_rgb rgb_to_hls".split()),
    "struct": frozenset("pack unpack calcsize".split()),
    "zlib": frozenset("compress decompress crc32 adler32".split()),
    "array": frozenset({"array"}),
    "re": frozenset("compile escape findall finditer match fullmatch search split sub IGNORECASE MULTILINE DOTALL".split()),
}


class _OpaqueSource(Exception):
    pass


class _SourceBindings(ast.NodeVisitor):
    """Lexical bindings only; nested functions/classes/comprehensions own theirs."""
    def __init__(self):
        self.names = set()

    def visit_Name(self, node):
        if isinstance(node.ctx, ast.Store):
            self.names.add(node.id)

    def visit_FunctionDef(self, node):
        self.names.add(node.name)

    visit_AsyncFunctionDef = visit_FunctionDef
    visit_ClassDef = visit_FunctionDef

    def visit_Import(self, node):
        self.names.update(alias.asname or alias.name.split(".")[0] for alias in node.names)

    def visit_ImportFrom(self, node):
        self.names.update(alias.asname or alias.name for alias in node.names if alias.name != "*")

    def visit_ListComp(self, node):
        pass

    visit_SetComp = visit_DictComp = visit_GeneratorExp = visit_ListComp


class _SceneOnlySource(ast.NodeVisitor):
    def __init__(self, tree, star_exports):
        bindings = _SourceBindings()
        for node in tree.body:
            bindings.visit(node)
        # A later module binding must not make an earlier builtin call look safe.
        if (bindings.names.intersection(vars(builtins))
                or any(name.startswith("_") for name in bindings.names)):
            raise _OpaqueSource()
        noon_names = _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS | _SOURCE_CONSTANTS
        imported = frozenset().union(*star_exports.values())
        if ((imported - noon_names).intersection(bindings.names | _SOURCE_BUILTINS | {"context", "__name__"})):
            # A later assignment must not bless an earlier name supplied by an
            # opaque wildcard export (e.g. noon.version, which reads metadata).
            raise _OpaqueSource()
        self.scopes = [bindings.names | {"context", "__name__"} | _SOURCE_BUILTINS]
        self.members = _SOURCE_MEMBERS | frozenset().union(*_SOURCE_MODULES.values())
        self.code_constructors = {"Code"} | {
            alias.asname or alias.name for node in tree.body
            if isinstance(node, ast.ImportFrom) and node.module in {"noon", "manim"}
            for alias in node.names if alias.name == "Code"
        }
        self.admitted_code_loads = set()

    def visit_Name(self, node):
        # exec/eval installs __builtins__ before user module assignments. A
        # later binding cannot turn that intrinsic (or compiler helpers) into
        # owned data. __name__ is the one explicit string in our namespace.
        if node.id.startswith("_") and node.id != "__name__":
            raise _OpaqueSource()
        if (isinstance(node.ctx, ast.Load) and node.id in self.code_constructors
                and id(node) not in self.admitted_code_loads):
            raise _OpaqueSource()
        if isinstance(node.ctx, ast.Load) and not any(node.id in scope for scope in self.scopes):
            raise _OpaqueSource()

    def visit_Call(self, node):
        # Code(code_file=...) reads the host filesystem. Only the explicit
        # in-memory constructor form has the scene/data-only contract; aliases,
        # positional paths and expanded keyword dictionaries are not proven.
        if isinstance(node.func, ast.Name) and node.func.id in self.code_constructors:
            keywords = {keyword.arg for keyword in node.keywords}
            if node.args or "code_string" not in keywords or "code_file" in keywords or None in keywords:
                raise _OpaqueSource()
            self.admitted_code_loads.add(id(node.func))
        self.generic_visit(node)

    def visit_Attribute(self, node):
        if not isinstance(node.ctx, ast.Load) or node.attr not in self.members:
            raise _OpaqueSource()
        self.visit(node.value)

    def visit_Import(self, node):
        if len(self.scopes) != 1 or any(alias.name not in _SOURCE_MODULES for alias in node.names):
            raise _OpaqueSource()

    def visit_ImportFrom(self, node):
        if len(self.scopes) != 1 or node.level:
            raise _OpaqueSource()
        noon_names = _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS | _SOURCE_CONSTANTS
        allowed = noon_names if node.module in {"noon", "manim"} else _SOURCE_MODULES.get(node.module, ())
        for alias in node.names:
            if alias.name == "*" and node.module in {"noon", "manim"}:
                self.scopes[0].update(noon_names)
            elif alias.name not in allowed:
                raise _OpaqueSource()

    def _scope(self, body, parameters=()):
        bindings = _SourceBindings()
        for node in body:
            bindings.visit(node)
        self.scopes.append(bindings.names | set(parameters))
        try:
            for node in body:
                self.visit(node)
        finally:
            self.scopes.pop()

    def visit_FunctionDef(self, node):
        if node.name.startswith("_") or node.decorator_list or node.returns or getattr(node, "type_params", ()):
            raise _OpaqueSource()
        arguments = [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]
        if any(argument.annotation for argument in arguments) or node.args.vararg or node.args.kwarg:
            raise _OpaqueSource()
        for value in [*node.args.defaults, *filter(None, node.args.kw_defaults)]:
            self.visit(value)
        self._scope(node.body, (argument.arg for argument in arguments))

    visit_AsyncFunctionDef = visit_FunctionDef

    def visit_ClassDef(self, node):
        if (node.decorator_list or node.keywords or getattr(node, "type_params", ()) or len(node.bases) != 1
                or not isinstance(node.bases[0], ast.Name) or node.bases[0].id != "Scene"):
            raise _OpaqueSource()
        self.visit(node.bases[0])
        if any(not isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Expr, ast.Pass))
               or isinstance(member, ast.Expr) and not isinstance(member.value, ast.Constant)
               for member in node.body):
            raise _OpaqueSource()
        # Methods resolve module globals, not names in their class dictionary.
        for member in node.body:
            self.visit(member)

    def visit_Lambda(self, node):
        self.visit_FunctionDef(ast.FunctionDef(name="callback", args=node.args,
            body=[ast.Expr(value=node.body)], decorator_list=[], returns=None))

    def _comprehension(self, node):
        self.scopes.append(set())
        try:
            for generator in node.generators:
                if generator.is_async:
                    raise _OpaqueSource()
                self.visit(generator.iter)
                bindings = _SourceBindings()
                bindings.visit(generator.target)
                self.scopes[-1].update(bindings.names)
                for condition in generator.ifs:
                    self.visit(condition)
            for value in ([node.key, node.value] if isinstance(node, ast.DictComp) else [node.elt]):
                self.visit(value)
        finally:
            self.scopes.pop()

    visit_ListComp = visit_SetComp = visit_DictComp = visit_GeneratorExp = _comprehension

    def visit_Subscript(self, node):
        if not isinstance(node.ctx, ast.Load):
            raise _OpaqueSource()
        self.generic_visit(node)

    def _opaque(self, node):
        raise _OpaqueSource()

    visit_Global = visit_Nonlocal = visit_With = visit_AsyncWith = _opaque
    visit_Delete = visit_NamedExpr = _opaque


def source_is_scene_only(source: str) -> bool:
    """Prove a closed trusted-API subset; unknown Python remains opaque.

    This assumes a pristine interpreter (or only preceding proven sources).
    The worker latches opaque execution as taint, since it may mutate libraries.
    This is source-effect metadata only: Rust still checks callbacks/input,
    replay, resources and actual continuation/publication receipts separately.
    """
    try:
        tree = ast.parse(source)
        exports = {}
        for node in ast.walk(tree):
            if (isinstance(node, ast.ImportFrom) and node.module in {"noon", "manim"}
                    and any(alias.name == "*" for alias in node.names)):
                if node.module == "noon":
                    import noon
                module = sys.modules.get(node.module)
                if module is None or not isinstance(getattr(module, "__all__", None), (list, tuple)):
                    return False
                exports[node.module] = frozenset(module.__all__)
        _SceneOnlySource(tree, exports).visit(tree)
        return True
    except (_OpaqueSource, SyntaxError, RecursionError):
        return False
RESOURCE_BARRIER_METHODS = frozenset({
    "add_effect", "add", "add_foreground_mobject", "add_foreground_mobjects",
    "bring_to_front", "bring_to_back", "replace",
})


@dataclass
class _SourceInvocation:
    cleanup: ExitStack
    authoring_scene_selected: bool = False

    def select_authoring_scene(self, scene):
        # Module source between barriers needs the same host routing context as
        # a construct body. Restore the enclosing context once when source exits.
        from _manim_reactive import _enter_authoring_scene, _leave_authoring_scene

        token = _enter_authoring_scene(scene)
        if not self.authoring_scene_selected:
            self.authoring_scene_selected = True
            self.cleanup.callback(_leave_authoring_scene, token)


_SOURCE_INVOCATION: ContextVar[_SourceInvocation | None] = ContextVar(
    "noon_source_invocation", default=None
)


def current_source_invocation():
    return _SOURCE_INVOCATION.get()


@contextmanager
def authoring_source_scope():
    """Scope top-level host execution and restore continuation flags on exit.

    Scene state and timing stay in Rust. The scope carries host context and
    cleanup only; construct dispatch retains its portable/async handling.
    """
    with ExitStack() as cleanup:
        token = _SOURCE_INVOCATION.set(_SourceInvocation(cleanup))
        cleanup.callback(_SOURCE_INVOCATION.reset, token)
        yield

# Known Scene calls, with suspension handled only at the explicit barriers above.
# Unknown methods (including super().construct()) keep their original host path.
_SCENE_CALLS = frozenset({
    "play", "wait", "add", "remove", "clear", "on_click", "set_drag_targets",
    "add_foreground_mobject", "add_foreground_mobjects",
    "activate_zooming", "get_zoomed_display_pop_out_animation",
    "bring_to_front", "bring_to_back", "replace", "next_section",
})


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
            and (isinstance(call.func.value, ast.Name)
                and call.func.value.id == self.receiver
                and call.func.attr in {"play", "wait"}
            )
        ):
            # Inspect arguments as usual, but admit this particular receiver and
            # barrier. A nested call to play/wait is not a statement barrier.
            self.allowed_receivers.add(id(call.func.value))
            call.args = [self.visit(arg) for arg in call.args]
            call.keywords = [self.visit(keyword) for keyword in call.keywords]
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
        if isinstance(func, ast.Attribute) and func.attr in RESOURCE_BARRIER_METHODS:
            self.allowed_receivers.add(id(func.value))
            if func.attr == "add_effect" and isinstance(func.value, ast.Name) and func.value.id == self.receiver:
                self.unsupported = True
            node = self.generic_visit(node)
            self.barriers += 1
            return ast.copy_location(ast.Await(value=ast.Call(
                func=ast.Name(id=MODULE_BARRIER_GLOBAL, ctx=ast.Load()),
                args=[node.func, *node.args], keywords=node.keywords,
            )), node)
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

    def visit_GeneratorExp(self, node):
        # A deferred generator cannot suspend its synchronous consumer. Keep the
        # original synchronous/explicit-async host path for hidden resource calls.
        if any(isinstance(child, ast.Call) and isinstance(child.func, ast.Attribute)
               and child.func.attr in RESOURCE_BARRIER_METHODS for child in ast.walk(node)):
            self.unsupported = True
        return self._visit_callback(node)

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
            call = self.generic_visit(call)
            self.barriers += 1
            wrapped = ast.Call(
                func=ast.Name(id=MODULE_BARRIER_GLOBAL, ctx=ast.Load()),
                args=[call.func, *call.args], keywords=call.keywords,
            )
            return ast.copy_location(
                ast.Expr(value=ast.copy_location(ast.Await(value=wrapped), call)), node
            )
        return self.generic_visit(node)

    def visit_Call(self, node):
        if isinstance(node.func, ast.Attribute) and node.func.attr in RESOURCE_BARRIER_METHODS:
            node = self.generic_visit(node)
            self.barriers += 1
            return ast.copy_location(ast.Await(value=ast.Call(
                func=ast.Name(id=MODULE_BARRIER_GLOBAL, ctx=ast.Load()),
                args=[node.func, *node.args], keywords=node.keywords,
            )), node)
        return self.generic_visit(node)

    def visit_FunctionDef(self, node):
        return node

    visit_AsyncFunctionDef = visit_FunctionDef
    visit_ClassDef = visit_FunctionDef
    visit_Lambda = visit_FunctionDef
    visit_GeneratorExp = visit_FunctionDef


def compile_authoring_source(
    source: str, filename: str = "<string>", *, portable: bool = True
) -> tuple[CodeType, dict[CodeType, CodeType]]:
    """Return executable module code and optional portable construct code pairs."""
    original = compile(
        source, filename, "exec",
        flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT, dont_inherit=True,
    )
    if not portable or any(name in source for name in (BARRIER_GLOBAL, MODULE_BARRIER_GLOBAL)):
        return original, {}
    originals = list(_function_codes(original))
    has_construct = any(
        code.co_name == "construct"
        and not code.co_flags & (inspect.CO_COROUTINE | inspect.CO_GENERATOR)
        and {"play", "wait", "add_effect"}.intersection(code.co_names)
        for code in originals
    )
    has_module_barrier = bool({"play", "wait", "add_effect"}.intersection(original.co_names))
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
