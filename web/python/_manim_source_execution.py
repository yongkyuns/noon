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
Dot Line Arrow DoubleArrow DashedLine VMobject VGroup Group Text MarkupText Code MathTypst
Color Vec2 Vec3 VectorPath Create Write Uncreate FadeIn FadeOut Transform
ImageMobject
RadialGradient NoisePaint HatchPaint
ReplacementTransform Indicate GrowFromCenter AnimationGroup Succession LaggedStart
LaggedStartMap SpinInFromNothing ShrinkToCenter
TransformMatchingShapes
ShowIncreasingSubsets ShowSubmobjectsOneByOne
ValueTracker DecimalNumber Integer Pixels Glow Blur BackdropBlur GlowPulse Dissolve
""".split())
_SOURCE_MAPPED_ANIMATION_FACTORIES = frozenset("FadeIn FadeOut".split())
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
abs all any bool bytes bytearray dict divmod enumerate filter float frozenset int
isinstance len list map max min next range reversed round set slice sorted str
sum tuple zip ValueError RuntimeError
""".split())
_SOURCE_MEMBERS = frozenset("""
add remove clear play wait on_click set_drag_targets next_section
add_foreground_mobject add_foreground_mobjects bring_to_front bring_to_back replace
shift move_to scale rotate stretch to_edge to_corner next_to align_to arrange
set_fill set_stroke set_color set_opacity set_z_index set_glow remove_glow set_effect
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
    def __init__(self, node=None, reason="unproven_source"):
        self.node = node
        self.reason = reason


class _SourceBindings(ast.NodeVisitor):
    """Lexical bindings only; nested functions/classes/comprehensions own theirs."""
    def __init__(self, integer_names=()):
        self.names = set()
        self.locations = {}
        self.binding_counts = {}
        self.container_stores = set()
        self.container_names = set()
        self.other_names = set()
        self.integer_names = frozenset(integer_names)

    def visit_Assign(self, node):
        def record(target, value):
            if isinstance(target, ast.Name):
                fresh = isinstance(value, (ast.List, ast.Dict, ast.ListComp, ast.DictComp))
                if isinstance(value, ast.BinOp) and isinstance(value.op, ast.Mult):
                    fresh = any(isinstance(items, ast.List) and (
                        isinstance(count, ast.Constant) and type(count.value) is int
                        or isinstance(count, ast.Name) and count.id in self.integer_names) for items, count in
                        ((value.left, value.right), (value.right, value.left)))
                if fresh:
                    self.container_stores.add(id(target))
            elif (isinstance(target, (ast.Tuple, ast.List)) and isinstance(value, (ast.Tuple, ast.List))
                    and len(target.elts) == len(value.elts)):
                for item, origin in zip(target.elts, value.elts):
                    record(item, origin)
        for target in node.targets:
            record(target, node.value)
        self.generic_visit(node)

    def _bind(self, name, node, container=False):
        self.names.add(name)
        self.locations.setdefault(name, node)
        self.binding_counts[name] = self.binding_counts.get(name, 0) + 1
        (self.container_names if container else self.other_names).add(name)

    def visit_Name(self, node):
        if isinstance(node.ctx, ast.Store):
            self._bind(node.id, node, id(node) in self.container_stores)

    def visit_FunctionDef(self, node):
        self._bind(node.name, node)

    visit_AsyncFunctionDef = visit_FunctionDef
    visit_ClassDef = visit_FunctionDef

    def visit_Import(self, node):
        for alias in node.names:
            self._bind(alias.asname or alias.name.split(".")[0], node)

    def visit_ImportFrom(self, node):
        for alias in node.names:
            if alias.name != "*":
                self._bind(alias.asname or alias.name, node)

    def visit_ExceptHandler(self, node):
        if node.name:
            self._bind(node.name, node)
        self.generic_visit(node)

    def visit_MatchAs(self, node):
        if node.name:
            self._bind(node.name, node)
        self.generic_visit(node)

    visit_MatchStar = visit_MatchAs

    def visit_MatchMapping(self, node):
        if node.rest:
            self._bind(node.rest, node)
        self.generic_visit(node)

    def visit_ListComp(self, node):
        pass

    visit_SetComp = visit_DictComp = visit_GeneratorExp = visit_ListComp


class _SceneOnlySource(ast.NodeVisitor):
    def __init__(self, tree, star_exports):
        self.admitted_lagged_map_loads = set()
        bindings = _SourceBindings()
        for node in tree.body:
            bindings.visit(node)
        # A later module binding must not make an earlier builtin call look safe.
        blocked = bindings.names.intersection(vars(builtins)) | {
            name for name in bindings.names if name.startswith("_")
        }
        if blocked:
            name = next(name for name in bindings.locations if name in blocked)
            raise _OpaqueSource(bindings.locations[name], "unproven_binding")
        self.module_bindings = bindings.names
        self.module_binding_counts = bindings.binding_counts
        self.module_binding_locations = bindings.locations
        self.module_functions = {
            node.name: node for node in tree.body
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        }
        self.admitted_image_loads = set()
        self.image_pixel_sources = [set()]
        self.pixel_factory_names = set()
        self.admitted_pixel_factory_loads = set()
        self.has_image_constructor = any(
            isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
            and node.func.id == "ImageMobject" for node in ast.walk(tree)
        )
        if self.has_image_constructor:
            self.closed_pixel_factory_names = {
                name for name in self.module_functions if self._closed_pixel_factory(name)
            }
            for function in ast.walk(tree):
                if not isinstance(function, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                function_bindings = _SourceBindings()
                for statement in function.body:
                    function_bindings.visit(statement)
                parameters = {arg.arg for arg in [*function.args.posonlyargs, *function.args.args,
                                                    *function.args.kwonlyargs]}
                self._image_pixel_source_names(function.body, function_bindings, parameters)
        noon_names = _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS | _SOURCE_CONSTANTS
        imported = frozenset().union(*star_exports.values())
        conflicts = (imported - noon_names).intersection(bindings.names | _SOURCE_BUILTINS | {"context", "__name__"})
        if conflicts:
            # A later assignment must not bless an earlier name supplied by an
            # opaque wildcard export (e.g. noon.version, which reads metadata).
            name = next((name for name in bindings.locations if name in conflicts), None)
            origin = bindings.locations[name] if name is not None else next(
                node for node in ast.walk(tree) if isinstance(node, ast.ImportFrom)
                and node.module in star_exports and any(alias.name == "*" for alias in node.names))
            raise _OpaqueSource(origin, "unproven_wildcard_binding")
        self.scopes = [bindings.names | {"context", "__name__"} | _SOURCE_BUILTINS]
        self.owned_containers = [set()]
        self.members = _SOURCE_MEMBERS | frozenset().union(*_SOURCE_MODULES.values())
        # Constructor aliases and wildcard exports are proven only for direct
        # module declarations, never imports hidden in a conditional suite.
        self.module_imports = {node for node in tree.body if isinstance(node, (ast.Import, ast.ImportFrom))}
        self.code_constructors = {"Code"} | {
            alias.asname or alias.name for node in tree.body
            if isinstance(node, ast.ImportFrom) and node.module in {"noon", "manim"}
            for alias in node.names if alias.name == "Code"
        }
        self.admitted_code_loads = set()

    def visit(self, node):
        try:
            return super().visit(node)
        except _OpaqueSource as error:
            if error.node is None:
                error.node = node
            raise

    def visit_Name(self, node):
        # exec/eval installs __builtins__ before user module assignments. A
        # later binding cannot turn that intrinsic (or compiler helpers) into
        # owned data. __name__ is the one explicit string in our namespace;
        # a local discard target is permitted only as a write, never a read.
        local_discard_store = (
            node.id == "_" and isinstance(node.ctx, ast.Store) and len(self.scopes) > 1
        )
        if node.id.startswith("_") and node.id != "__name__" and not local_discard_store:
            raise _OpaqueSource(node, "private_name")
        if (isinstance(node.ctx, ast.Load) and node.id in self.code_constructors
                and id(node) not in self.admitted_code_loads):
            raise _OpaqueSource(node, "unproven_resource_reference")
        if (isinstance(node.ctx, ast.Load) and node.id == "ImageMobject"
                and id(node) not in self.admitted_image_loads):
            raise _OpaqueSource(node, "unproven_image_reference")
        if (isinstance(node.ctx, ast.Load) and node.id in self.pixel_factory_names
                and id(node) not in self.admitted_pixel_factory_loads):
            raise _OpaqueSource(node, "unproven_image_factory_reference")
        if (isinstance(node.ctx, ast.Load) and node.id == "LaggedStartMap"
                and id(node) not in self.admitted_lagged_map_loads):
            raise _OpaqueSource(node, "unproven_callback_factory")
        if isinstance(node.ctx, ast.Load) and not any(node.id in scope for scope in self.scopes):
            raise _OpaqueSource(node, "unknown_name")

    def _closed_pixel_factory(self, name):
        """Prove one zero-argument helper returns only fresh builtin int lists.

        This is an AST type pass: it never invokes source or iterates a range.
        """
        function = self.module_functions.get(name)
        if (not isinstance(function, ast.FunctionDef) or self.module_binding_counts.get(name) != 1
                or self.module_binding_locations.get(name) is not function):
            return False
        args = function.args
        if (args.posonlyargs or args.kwonlyargs or args.vararg or args.kwarg
                or len(args.defaults) != len(args.args)
                or any(not isinstance(value, ast.Constant) or type(value.value) is not int
                       for value in args.defaults)):
            return False
        bindings = _SourceBindings()
        for statement in function.body:
            bindings.visit(statement)
        if {"range", "abs"}.intersection(bindings.names | {arg.arg for arg in args.args}):
            return False
        values = {arg.arg: "int" for arg in args.args}
        transferred = set()
        empty_list = object()

        def merge(left, right, empty_item=False):
            if left is empty_list:
                return right if empty_item else None
            if right is empty_list:
                return left if empty_item else None
            if left is None or right is None:
                return None
            if left[0] != right[0]:
                return None
            if left[0] == "list":
                item = merge(left[1], right[1], empty_item=True)
                return ("list", item)
            return left

        def value(node, state):
            if isinstance(node, ast.Constant) and type(node.value) in (int, float):
                return "int" if type(node.value) is int else "float"
            if isinstance(node, ast.Name):
                return state.get(node.id)
            if isinstance(node, ast.List):
                items = [value(item, state) for item in node.elts]
                if any(item and item[0] == "list" and not isinstance(node.elts[index], ast.List)
                       for index, item in enumerate(items)):
                    return None
                return (("list", items[0] if items else empty_list)
                        if all(item is not None for item in items)
                        and all(item == items[0] for item in items[1:]) else None)
            if isinstance(node, ast.BinOp):
                left, right = value(node.left, state), value(node.right, state)
                if left not in {"int", "float"} or right not in {"int", "float"}:
                    return None
                if isinstance(node.op, (ast.Add, ast.Sub, ast.Mult, ast.FloorDiv, ast.Mod)):
                    return "float" if "float" in (left, right) else "int"
                if isinstance(node.op, ast.Div):
                    return "float"
                if isinstance(node.op, ast.Pow):
                    exp = node.right.value if isinstance(node.right, ast.Constant) else None
                    return "int" if left == right == "int" and type(exp) is int and exp >= 0 else "float"
            if isinstance(node, ast.Compare) and all(
                (kind := value(part, state)) in {"int", "float"}
                for part in [node.left, *node.comparators]
            ):
                return "bool"
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) \
                    and node.func.id == "abs" and len(node.args) == 1 and not node.keywords:
                kind = value(node.args[0], state)
                return kind if kind in {"int", "float"} else None
            return None

        def block(body, state, moved):
            for statement in body:
                if isinstance(statement, ast.Assign) and len(statement.targets) == 1 \
                        and isinstance(statement.targets[0], ast.Name):
                    target = statement.targets[0].id
                    if isinstance(statement.value, ast.Name) and state.get(statement.value.id, (None,))[0] == "list":
                        return None
                    inferred = value(statement.value, state)
                    if inferred is None:
                        return None
                    state[target] = inferred
                    moved.discard(target)
                elif isinstance(statement, ast.Expr) and isinstance(statement.value, ast.Call):
                    call = statement.value
                    if (not isinstance(call.func, ast.Attribute) or call.func.attr != "append"
                            or not isinstance(call.func.value, ast.Name) or len(call.args) != 1 or call.keywords):
                        return None
                    owner, item = call.func.value.id, call.args[0]
                    owner_type, item_type = state.get(owner), value(item, state)
                    if (owner in moved or owner_type is None or owner_type[0] != "list" or item_type is None
                            or owner_type[1] not in (None, empty_list) and owner_type[1] != item_type
                            or item_type[0] == "list" and (not isinstance(item, ast.Name) or item.id in moved)):
                        return None
                    state[owner] = ("list", item_type)
                    if item_type[0] == "list":
                        moved.add(item.id)
                elif isinstance(statement, ast.For) and isinstance(statement.target, ast.Name) \
                        and isinstance(statement.iter, ast.Call) and isinstance(statement.iter.func, ast.Name) \
                        and statement.iter.func.id == "range" and len(statement.iter.args) == 1 \
                        and not statement.iter.keywords and not statement.orelse:
                    if value(statement.iter.args[0], state) != "int":
                        return None
                    body_state, body_moved = state.copy(), moved.copy()
                    body_state[statement.target.id] = "int"
                    if block(statement.body, body_state, body_moved) is None:
                        return None
                    for name in state.keys() & body_state.keys():
                        state[name] = merge(state[name], body_state[name])
                        if state[name] is None:
                            return None
                    moved.update(body_moved)
                elif isinstance(statement, ast.If):
                    if value(statement.test, state) != "bool":
                        return None
                    branches = []
                    for branch in (statement.body, statement.orelse):
                        branch_state, branch_moved = state.copy(), moved.copy()
                        if block(branch, branch_state, branch_moved) is None:
                            return None
                        branches.append((branch_state, branch_moved))
                    first, second = branches
                    for name in state.keys() | first[0].keys() | second[0].keys():
                        state[name] = merge(first[0].get(name), second[0].get(name))
                    moved.update(first[1] | second[1])
                else:
                    return None
            return state

        body = function.body
        if body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant) \
                and isinstance(body[0].value.value, str):
            body = body[1:]
        if not body or not isinstance(body[-1], ast.Return) or not isinstance(body[-1].value, ast.Name):
            return False
        if block(body[:-1], values, transferred) is None:
            return False
        result = values.get(body[-1].value.id)
        if result is None or result[0] != "list" or body[-1].value.id in transferred:
            return False

        def numeric_tree(kind):
            return kind is empty_list or kind == "int" or (
                isinstance(kind, tuple) and kind[0] == "list" and numeric_tree(kind[1])
            )

        return numeric_tree(result)

    def _image_pixel_source_names(self, body, bindings, parameters):
        if not self.has_image_constructor:
            return set()
        class Uses(ast.NodeVisitor):
            def __init__(self):
                self.calls = []
                self.loads = []

            def visit_FunctionDef(self, node):
                return

            visit_AsyncFunctionDef = visit_ClassDef = visit_Lambda = visit_FunctionDef

            def visit_Call(self, node):
                if isinstance(node.func, ast.Name) and node.func.id == "ImageMobject":
                    self.calls.append(node)
                self.generic_visit(node)

            def visit_Name(self, node):
                if isinstance(node.ctx, ast.Load):
                    self.loads.append(node)

        uses = Uses()
        for statement in body:
            uses.visit(statement)
        if not uses.calls:
            return set()

        requested_sources = {
            call.args[0].id for call in uses.calls
            if len(call.args) == 1 and isinstance(call.args[0], ast.Name)
        }
        sources = set()
        for statement in body:
            if (not isinstance(statement, ast.Assign) or len(statement.targets) != 1
                    or not isinstance(statement.targets[0], ast.Name)
                    or statement.targets[0].id not in requested_sources
                    or not isinstance(statement.value, ast.Call)
                    or not isinstance(statement.value.func, ast.Name)
                    or statement.value.args or statement.value.keywords):
                continue
            target, helper = statement.targets[0].id, statement.value.func.id
            if (bindings.binding_counts.get(target) == 1 and helper not in bindings.names
                    and helper not in parameters and helper in self.closed_pixel_factory_names):
                sources.add(target)
                self.pixel_factory_names.add(helper)
                self.admitted_pixel_factory_loads.add(id(statement.value.func))
        accepted = set()
        for call in uses.calls:
            keywords = {keyword.arg for keyword in call.keywords}
            if (len(call.args) != 1 or any(keyword.arg is None for keyword in call.keywords)
                    or not keywords.issubset({"height", "resampling_algorithm", "opacity", "z_index",
                                               "scale_to_resolution"})
                    or not isinstance(call.args[0], ast.Name)
                    or call.args[0].id not in sources):
                raise _OpaqueSource(call, "unproven_image_source")
            for keyword in call.keywords:
                value = keyword.value
                if keyword.arg == "resampling_algorithm":
                    allowed = (isinstance(value, ast.Constant)
                        and ((type(value.value) is str and value.value.lower() in
                              {"nearest", "none", "bilinear", "linear", "bicubic", "cubic"})
                             or (type(value.value) is int and value.value in {0, 2, 3})))
                else:
                    allowed = isinstance(value, ast.Constant) and type(value.value) in (int, float)
                if not allowed:
                    raise _OpaqueSource(value, "unproven_image_option")
            accepted.add(id(call.args[0]))
        for name in sources:
            if any(node.id == name and id(node) not in accepted for node in uses.loads):
                raise _OpaqueSource(next(node for node in uses.loads if node.id == name),
                                    "unproven_image_source")
        return sources

    def visit_Call(self, node):
        if isinstance(node.func, ast.Name) and node.func.id == "ImageMobject":
            if "ImageMobject" in self.module_bindings or any(
                "ImageMobject" in scope for scope in self.scopes[1:]
            ):
                raise _OpaqueSource(node.func, "unproven_shadowed_image_constructor")
            if (len(node.args) != 1 or not isinstance(node.args[0], ast.Name)
                    or node.args[0].id not in self.image_pixel_sources[-1]):
                raise _OpaqueSource(node, "unproven_image_source")
            self.admitted_image_loads.add(id(node.func))
        if isinstance(node.func, ast.Name) and node.func.id == "LaggedStartMap":
            shadowed = "LaggedStartMap" in self.module_bindings or any(
                "LaggedStartMap" in scope for scope in self.scopes[1:]
            )
            if shadowed:
                raise _OpaqueSource(node.func, "unproven_shadowed_constructor")
            self.admitted_lagged_map_loads.add(id(node.func))
            if (any(isinstance(argument, ast.Starred) for argument in node.args)
                    or any(keyword.arg is None for keyword in node.keywords)):
                raise _OpaqueSource(node, "unproven_callback_arguments")
            factories = ([node.args[0]] if node.args else []) + [
                keyword.value for keyword in node.keywords if keyword.arg == "animation_class"
            ]
            for factory in factories:
                if not isinstance(factory, ast.Name):
                    raise _OpaqueSource(factory, "unproven_callback_factory")
                if factory.id == "Code":
                    continue  # Preserve its more specific resource-reference proof failure.
                if factory.id == "LaggedStartMap":
                    raise _OpaqueSource(factory, "unproven_callback_factory")
                direct_constructor = factory.id in _SOURCE_MAPPED_ANIMATION_FACTORIES and (
                    factory.id not in self.module_bindings
                    and not any(factory.id in scope for scope in self.scopes[1:])
                )
                if not direct_constructor:
                    raise _OpaqueSource(factory, "unproven_callback_factory")
            creators = ([node.args[2]] if len(node.args) > 2 else []) + [
                keyword.value for keyword in node.keywords if keyword.arg == "arg_creator"
            ]
            if len(creators) > 1 or any(not self._is_inert_arg_creator(value) for value in creators):
                callback = creators[0] if creators else node
                raise _OpaqueSource(callback, "unproven_callback_argument")
        # Code(code_file=...) reads the host filesystem. Only the explicit
        # in-memory constructor form has the scene/data-only contract; aliases,
        # positional paths and expanded keyword dictionaries are not proven.
        if isinstance(node.func, ast.Name) and node.func.id in self.code_constructors:
            keywords = {keyword.arg for keyword in node.keywords}
            if node.args or "code_string" not in keywords or "code_file" in keywords or None in keywords:
                raise _OpaqueSource(node, "unproven_resource_input")
            self.admitted_code_loads.add(id(node.func))
        self.generic_visit(node)

    @staticmethod
    def _is_inert_arg_creator(node):
        if isinstance(node, ast.Constant) and node.value is None:
            return True
        if (not isinstance(node, ast.Lambda) or node.args.posonlyargs
                or len(node.args.args) != 1 or node.args.defaults or node.args.kwonlyargs
                or node.args.kw_defaults or node.args.vararg or node.args.kwarg):
            return False
        parameter = node.args.args[0].arg
        return (isinstance(node.body, ast.Name) and node.body.id == parameter) or (
            isinstance(node.body, ast.Tuple) and len(node.body.elts) == 1
            and isinstance(node.body.elts[0], ast.Name) and node.body.elts[0].id == parameter
        )

    def visit_Attribute(self, node):
        if not isinstance(node.ctx, ast.Load) or node.attr not in self.members:
            raise _OpaqueSource(node, "unsupported_member" if isinstance(node.ctx, ast.Load) else "unproven_member_write")
        self.visit(node.value)

    def visit_Import(self, node):
        if node not in self.module_imports or len(self.scopes) != 1 or any(alias.name not in _SOURCE_MODULES for alias in node.names):
            raise _OpaqueSource(node, "unsupported_import")

    def visit_ImportFrom(self, node):
        if node not in self.module_imports or len(self.scopes) != 1 or node.level:
            raise _OpaqueSource(node, "unsupported_import")
        if node.module in {"noon", "manim"} and any(
            alias.name == "LaggedStartMap" and alias.asname is not None for alias in node.names
        ):
            raise _OpaqueSource(node, "unproven_callback_factory")
        if node.module in {"noon", "manim"} and any(
            alias.name == "ImageMobject" and alias.asname is not None for alias in node.names
        ):
            raise _OpaqueSource(node, "unproven_image_constructor")
        noon_names = _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS | _SOURCE_CONSTANTS
        allowed = noon_names if node.module in {"noon", "manim"} else _SOURCE_MODULES.get(node.module, ())
        for alias in node.names:
            if alias.name == "*" and node.module in {"noon", "manim"}:
                self.scopes[0].update(noon_names)
            elif alias.name not in allowed:
                raise _OpaqueSource(node, "unsupported_import")

    def _guarded_integer_parameters(self, body, parameters, bindings):
        shadowed = bindings.names | parameters | set().union(*self.scopes[1:])
        if {"int", "bool", "isinstance"}.intersection(shadowed):
            return set()
        if (body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant)
                and isinstance(body[0].value.value, str)):
            body = body[1:]
        if (not body or not isinstance(body[0], ast.If) or len(body[0].body) != 1
                or not isinstance(body[0].body[0], (ast.Raise, ast.Return))):
            return set()
        test = body[0].test
        clauses = test.values if isinstance(test, ast.BoolOp) and isinstance(test.op, ast.Or) else [test]
        bool_checks = set()
        for clause in clauses:
            negated = isinstance(clause, ast.UnaryOp) and isinstance(clause.op, ast.Not)
            call = clause.operand if negated else clause
            if (not isinstance(call, ast.Call) or not isinstance(call.func, ast.Name) or call.func.id != "isinstance"
                    or len(call.args) != 2 or call.keywords or not isinstance(call.args[0], ast.Name)
                    or not isinstance(call.args[1], ast.Name)):
                return set()
            name, kind = call.args[0].id, call.args[1].id
            if name not in parameters or name in bindings.names:
                return set()
            if negated and kind == "int" and bool_checks.issubset({name}):
                return {name}
            if negated or kind != "bool":
                return set()
            bool_checks.add(name)
        return set()

    def _scope(self, body, parameters=()):
        parameters = set(parameters)
        bindings = _SourceBindings()
        for node in body:
            bindings.visit(node)
        integer_names = self._guarded_integer_parameters(body, parameters, bindings)
        if integer_names:
            # Only a leading, terminating builtin guard proves parameter type.
            # Re-scan origins with that fact; no source statement is executed.
            bindings = _SourceBindings(integer_names)
            for node in body:
                bindings.visit(node)
        image_pixel_sources = self._image_pixel_source_names(body, bindings, parameters)
        self.scopes.append(bindings.names | parameters)
        self.image_pixel_sources.append(image_pixel_sources)
        # Every lexical binding must have a fresh origin; a parameter, alias,
        # conditional rebind or loop target cannot inherit another value's proof.
        self.owned_containers.append(bindings.container_names - bindings.other_names - parameters)
        try:
            for node in body:
                self.visit(node)
        finally:
            self.image_pixel_sources.pop()
            self.scopes.pop()
            self.owned_containers.pop()

    def visit_FunctionDef(self, node):
        if node.name.startswith("_") or node.decorator_list or node.returns or getattr(node, "type_params", ()):
            raise _OpaqueSource(node, "unproven_function_contract")
        arguments = [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]
        if any(argument.annotation for argument in arguments) or node.args.vararg or node.args.kwarg:
            raise _OpaqueSource(node, "unproven_function_contract")
        for value in [*node.args.defaults, *filter(None, node.args.kw_defaults)]:
            self.visit(value)
        self._scope(node.body, (argument.arg for argument in arguments))

    visit_AsyncFunctionDef = visit_FunctionDef

    def visit_ClassDef(self, node):
        if (node.decorator_list or node.keywords or getattr(node, "type_params", ()) or len(node.bases) != 1
                or not isinstance(node.bases[0], ast.Name) or node.bases[0].id != "Scene"):
            raise _OpaqueSource(node, "unsupported_class_contract")
        self.visit(node.bases[0])
        if any(not isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Expr, ast.Pass))
               or isinstance(member, ast.Expr) and not isinstance(member.value, ast.Constant)
               for member in node.body):
            raise _OpaqueSource(node, "unsupported_class_contract")
        # Methods resolve module globals, not names in their class dictionary.
        for member in node.body:
            self.visit(member)

    def visit_Lambda(self, node):
        self.visit_FunctionDef(ast.copy_location(ast.FunctionDef(name="callback", args=node.args,
            body=[ast.Expr(value=node.body)], decorator_list=[], returns=None), node))

    def _comprehension(self, node):
        self.scopes.append(set())
        self.owned_containers.append(set())
        self.image_pixel_sources.append(self.image_pixel_sources[-1].copy())
        try:
            for generator in node.generators:
                if generator.is_async:
                    raise _OpaqueSource()
                self.visit(generator.iter)
                bindings = _SourceBindings()
                bindings.visit(generator.target)
                self.scopes[-1].update(bindings.names)
                self.image_pixel_sources[-1].difference_update(bindings.names)
                for condition in generator.ifs:
                    self.visit(condition)
            for value in ([node.key, node.value] if isinstance(node, ast.DictComp) else [node.elt]):
                self.visit(value)
        finally:
            self.image_pixel_sources.pop()
            self.scopes.pop()
            self.owned_containers.pop()

    visit_ListComp = visit_SetComp = visit_DictComp = visit_GeneratorExp = _comprehension

    def visit_Subscript(self, node):
        if not isinstance(node.ctx, ast.Load):
            name = node.value.id if isinstance(node.value, ast.Name) else None
            owner = next((owned for scope, owned in zip(reversed(self.scopes), reversed(self.owned_containers))
                if name in scope), ())
            if name not in owner:
                raise _OpaqueSource(node, "unproven_ownership")
        self.generic_visit(node)

    def visit_AugAssign(self, node):
        if isinstance(node.target, ast.Name) and node.target.id == "_":
            raise _OpaqueSource(node.target, "private_name")
        self.generic_visit(node)

    def _opaque(self, node):
        raise _OpaqueSource(node, "unproven_scope_effect")

    visit_Global = visit_Nonlocal = visit_With = visit_AsyncWith = _opaque
    visit_Delete = visit_NamedExpr = _opaque


def source_execution_proof(source: str) -> dict:
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
                    raise _OpaqueSource(node, "unproven_wildcard_exports")
                exports[node.module] = frozenset(module.__all__)
        _SceneOnlySource(tree, exports).visit(tree)
        return {"admitted": True, "reason": None}
    except _OpaqueSource as error:
        node = error.node
        # AST columns are zero-based UTF-8 offsets; lines are one-based.
        span = None if node is None or not hasattr(node, "lineno") else {
            "line": node.lineno, "column": node.col_offset,
            "endLine": node.end_lineno, "endColumn": node.end_col_offset,
        }
        return {"admitted": False, "reason": {
            "code": error.reason, "construct": type(node).__name__ if node else None,
            "span": span,
        }}
    except SyntaxError as error:
        lines = source.splitlines()
        line, end_line = error.lineno or 1, error.end_lineno or error.lineno or 1
        text = lines[line - 1] if line <= len(lines) else ""
        end_text = lines[end_line - 1] if end_line <= len(lines) else ""
        column = len(text[:max(0, (error.offset or 1) - 1)].encode())
        end_column = len(end_text[:max(0, (error.end_offset or error.offset or 1) - 1)].encode())
        return {"admitted": False, "reason": {
            "code": "invalid_syntax", "span": {"line": line, "column": column,
                "endLine": end_line, "endColumn": max(column, end_column) if line == end_line else end_column},
        }}
    except RecursionError:
        return {"admitted": False, "reason": {
            "code": "source_depth_limit",
            "span": None,
        }}


def source_is_scene_only(source: str) -> bool:
    return source_execution_proof(source)["admitted"]
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
