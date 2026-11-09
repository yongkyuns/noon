"""Compile portable host continuations, without changing scene semantics.

The module namespace and all definition effects execute exactly once. Direct
module-level play/wait statements use top-level await without an async-function
wrapper that would turn globals into locals. The deferred callbacks themselves
stay synchronous and are never rewritten.

The original construct definitions are retained. A second, never-executed module compilation
supplies coroutine code for ordinary ``construct`` methods with direct scene
barriers and resource-dependent effect/membership calls,
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
import math
import sys
from contextlib import ExitStack, contextmanager
from contextvars import ContextVar
from dataclasses import dataclass
from time import perf_counter
from types import CodeType, FunctionType, MethodType

BARRIER_GLOBAL = "_noon_await_source_barrier"
MODULE_BARRIER_GLOBAL = "_noon_await_module_barrier"
COOPERATIVE_CHECKPOINT_GLOBAL = "_noon_source_checkpoint"
_COOPERATIVE_YIELD_QUANTUM_SECONDS = 0.001

# A closed source may use these trusted, scene/data-only entry points. This is
# deliberately a positive capability list, not a blacklist of known I/O calls.
# Unknown dispatch, descriptors and source hooks keep observed execution.
_SOURCE_CONSTRUCTORS = frozenset("""
Scene Circle Square Rectangle RoundedRectangle Triangle Polygon RegularPolygon
Dot Line Arrow DoubleArrow DashedLine VMobject VGroup Group Text MarkupText Code MathTypst
Color Vec2 Vec3 VectorPath Create Write Uncreate FadeIn FadeOut Transform AudioSource
ImageMobject ImagePaint ImageMapping RadialGradient NoisePaint HatchPaint Axes ThreeDScene MovingCameraScene LinearTransformationScene
ReplacementTransform Indicate GrowFromCenter AnimationGroup Succession LaggedStart
LaggedStartMap SpinInFromNothing ShrinkToCenter
TransformMatchingShapes Rotate
ShowIncreasingSubsets ShowSubmobjectsOneByOne
ValueTracker DecimalNumber Integer Pixels Glow Blur BackdropBlur Bloom GlowPulse Dissolve SVGMobject
""".split())
_SOURCE_MAPPED_ANIMATION_FACTORIES = frozenset("FadeIn FadeOut".split())
_SOURCE_FUNCTIONS = frozenset("""
linear smooth rush_into rush_from there_and_back color_from_hex interpolate_color
always_redraw
""".split())
# LaTeX preparation performs an explicit, bounded worker resource fetch. It is
# not a generally pure function and is admitted only as a direct awaited call.
_SOURCE_RESOURCE_PREPARATIONS = frozenset({"prepare_latex"})
_SOURCE_CONTROLLED_TEXT_COMPILERS = frozenset({"MathTex"})
_SOURCE_SHADER_CONSTRUCTORS = frozenset({"WgslEffectProgram", "ShaderParameter", "ShaderEffect"})
_SOURCE_TYPED_PAINT_CONSTRUCTORS = frozenset({"LinearGradient"})
_SOURCE_CONSTANTS = frozenset("""
BLACK WHITE BLUE BLUE_A BLUE_B BLUE_C BLUE_D BLUE_E GREEN GREEN_A GREEN_B GREEN_C
GREEN_D GREEN_E RED RED_A RED_B RED_C RED_D RED_E YELLOW ORANGE PINK LIGHT_PINK
PURPLE PURPLE_A PURPLE_B PURPLE_C PURPLE_D PURPLE_E GREY GRAY GOLD TEAL MAROON
UP DOWN LEFT RIGHT IN OUT ORIGIN UL UR DL DR PI TAU DEGREES
""".split())
_SOURCE_BUILTINS = frozenset("""
abs all any bool bytes bytearray complex dict divmod enumerate filter float format frozenset int
isinstance len list map max min next range reversed round set slice sorted str
sum tuple type zip ValueError RuntimeError
""".split())
_SOURCE_MEMBERS = frozenset("""
add remove clear play wait audio on_click set_drag_targets next_section
add_foreground_mobject add_foreground_mobjects bring_to_front bring_to_back replace
shift move_to scale rotate stretch to_edge to_corner next_to align_to arrange
set_fill set_stroke set_color set_opacity set_emission set_object_opacity set_z_index set_glow remove_glow set_effect
add_effect remove_effect get_center get_x get_y get_width get_height get_value get_left get_right
set_fill_image_mapping set_value increment_value copy become add_updater remove_updater clear_updaters
plot plot_samples
start_new_path add_line_to add_cubic_bezier_curve_to close_path set_points_as_corners
set_points set_object_opacity family_members_with_points get_subpaths get_arc_length
point_from_proportion from_string
animate id width height radius append extend insert pop count index get keys values
items update union intersection difference join split strip lstrip rstrip lower upper replace bit_length
startswith endswith encode decode hex fromhex tobytes reverse sort small_discovery
""".split())
_SOURCE_SPATIAL_MEMBERS = frozenset("""
set_camera_orientation move_camera point_light add_world_mobjects
add_fixed_in_frame_mobjects add_fixed_orientation_mobjects parametric sphere c2p
""".split())
_SOURCE_MOVING_CAMERA_MEMBERS = frozenset("camera frame set_view_color_profile".split())
_CANONICAL_SCENE_FIELDS = frozenset("""
_owner _binding_handles _object_keys _object_key_ids _next_object_id
_canonical_authoring_context
_register_top_level _edit_membership _bind_camera_frame
""".split())
_SOURCE_LINEAR_MEMBERS = frozenset("background_plane add_coordinates add_vector apply_matrix".split())
_SOURCE_SPATIAL_CONSTRUCTORS = frozenset("Dot3D Mesh3D ThreeDAxes Typst WorldTransformTo".split())
_SOURCE_COLOR_CONSTANTS = frozenset("""
BLACK WHITE BLUE BLUE_A BLUE_B BLUE_C BLUE_D BLUE_E GREEN GREEN_A GREEN_B GREEN_C
GREEN_D GREEN_E RED RED_A RED_B RED_C RED_D RED_E YELLOW ORANGE PINK LIGHT_PINK
PURPLE PURPLE_A PURPLE_B PURPLE_C PURPLE_D PURPLE_E GREY GRAY GOLD TEAL MAROON
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

# Only the numeric subset used by the original Fourier helpers is source-safe.
# These are exact call paths, not a general NumPy attribute or object-dispatch grant.
_NUMPY_ARRAY_CALLS = frozenset({
    "np.array", "np.asarray", "np.concatenate", "np.cumsum", "np.diff", "np.exp",
    "np.interp", "np.linspace", "np.outer", "np.zeros", "np.fft.fft",
})
_NUMPY_SCALAR_CALLS = frozenset({
    "np.abs", "np.all", "np.isfinite", "np.max", "np.min", "np.ptp", "np.sum",
})
_NUMPY_CALLS = _NUMPY_ARRAY_CALLS | _NUMPY_SCALAR_CALLS
_NUMPY_ARRAY_METHODS = frozenset({"reshape", "copy"})
_NUMPY_ARRAY_ATTRIBUTES = frozenset({"real", "imag", "ndim", "shape"})


def _walk_scope(body):
    class ScopeWalk(ast.NodeVisitor):
        def __init__(self):
            self.nodes = []

        def generic_visit(self, node):
            self.nodes.append(node)
            super().generic_visit(node)

        def visit_FunctionDef(self, node):
            self.nodes.append(node)

        visit_AsyncFunctionDef = visit_ClassDef = visit_Lambda = visit_FunctionDef

    walker = ScopeWalk()
    for statement in body:
        walker.visit(statement)
    return walker.nodes


def _attribute_path(node):
    parts = []
    while isinstance(node, ast.Attribute):
        parts.append(node.attr)
        node = node.value
    if not isinstance(node, ast.Name):
        return None
    return ".".join([node.id, *reversed(parts)])


def _numpy_array_expression(node, names, array_functions=()):
    """Recognize only array-producing results needed by this closed data subset."""
    if isinstance(node, ast.Name):
        return node.id in names
    if isinstance(node, ast.Call):
        path = _attribute_path(node.func)
        if path in _NUMPY_ARRAY_CALLS:
            return True
        if isinstance(node.func, ast.Name) and node.func.id in array_functions:
            return True
        if (isinstance(node.func, ast.Attribute) and node.func.attr in _NUMPY_ARRAY_METHODS
                and _numpy_array_expression(node.func.value, names, array_functions)):
            return True
        if path in {"np.abs", "np.isfinite"} and node.args:
            return _numpy_array_expression(node.args[0], names, array_functions)
        return False
    if isinstance(node, ast.Attribute) and node.attr in _NUMPY_ARRAY_ATTRIBUTES:
        return _numpy_array_expression(node.value, names, array_functions)
    if isinstance(node, ast.Subscript):
        return _numpy_array_expression(node.value, names, array_functions)
    if isinstance(node, ast.BinOp):
        return (_numpy_array_expression(node.left, names, array_functions)
                or _numpy_array_expression(node.right, names, array_functions))
    if isinstance(node, ast.Compare):
        return any(_numpy_array_expression(value, names, array_functions)
                   for value in [node.left, *node.comparators])
    if isinstance(node, ast.UnaryOp):
        return _numpy_array_expression(node.operand, names, array_functions)
    return False


def _array_names_in_body(body, parameters, array_functions=(), tuple_outputs=None):
    """Conservatively mark local bindings whose expressions stay ndarray-like."""
    names = set(parameters)
    changed = True
    while changed:
        changed = False
        for statement in _walk_scope(body):
            if isinstance(statement, (ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)):
                for generator in statement.generators:
                    if _numpy_array_expression(generator.iter, names, array_functions):
                        targets = generator.target.elts if isinstance(generator.target, (ast.Tuple, ast.List)) else [generator.target]
                        for item in targets:
                            if isinstance(item, ast.Name) and item.id not in names:
                                names.add(item.id)
                                changed = True
            if (isinstance(statement, ast.For) and isinstance(statement.iter, ast.Call)
                    and isinstance(statement.iter.func, ast.Name) and statement.iter.func.id == "zip"
                    and any(_numpy_array_expression(arg, names, array_functions)
                            for arg in statement.iter.args)):
                targets = statement.target.elts if isinstance(statement.target, (ast.Tuple, ast.List)) else [statement.target]
                for item in targets:
                    if isinstance(item, ast.Name) and item.id not in names:
                        names.add(item.id)
                        changed = True
            if (isinstance(statement, ast.For) and isinstance(statement.iter, ast.Call)
                    and isinstance(statement.iter.func, ast.Name) and statement.iter.func.id == "enumerate"
                    and statement.iter.args
                    and _numpy_array_expression(statement.iter.args[0], names, array_functions)
                    and isinstance(statement.target, (ast.Tuple, ast.List)) and len(statement.target.elts) == 2
                    and isinstance(statement.target.elts[1], ast.Name)
                    and statement.target.elts[1].id not in names):
                names.add(statement.target.elts[1].id)
                changed = True
            if not isinstance(statement, ast.Assign):
                continue
            for target in statement.targets:
                targets = target.elts if isinstance(target, (ast.Tuple, ast.List)) else [target]
                unpacking = isinstance(target, (ast.Tuple, ast.List))
                outputs = None
                if (isinstance(statement.value, ast.Call) and isinstance(statement.value.func, ast.Name)
                        and tuple_outputs is not None):
                    outputs = tuple_outputs.get(statement.value.func.id)
                is_array = _numpy_array_expression(statement.value, names, array_functions)
                for index, item in enumerate(targets):
                    target_is_array = ((is_array and not unpacking)
                                       or outputs is not None and index < len(outputs) and outputs[index])
                    if target_is_array and isinstance(item, ast.Name) and item.id not in names:
                        names.add(item.id)
                        changed = True
    invalid = set()
    for statement in _walk_scope(body):
        if not isinstance(statement, ast.Assign):
            continue
        for target in statement.targets:
            targets = target.elts if isinstance(target, (ast.Tuple, ast.List)) else [target]
            outputs = None
            if isinstance(statement.value, ast.Call) and isinstance(statement.value.func, ast.Name):
                outputs = (tuple_outputs or {}).get(statement.value.func.id)
            is_array = _numpy_array_expression(statement.value, names, array_functions)
            for index, item in enumerate(targets):
                if not isinstance(item, ast.Name) or item.id not in names:
                    continue
                if not (is_array and not isinstance(target, (ast.Tuple, ast.List))
                        or outputs is not None and index < len(outputs) and outputs[index]):
                    invalid.add(item.id)
    return names - invalid


def _static_numpy_literal(node):
    if isinstance(node, ast.Constant):
        value = node.value
        if value is None or type(value) in (bool, int):
            return True
        if type(value) is float:
            return math.isfinite(value)
        if type(value) is complex:
            return math.isfinite(value.real) and math.isfinite(value.imag)
        return False
    if isinstance(node, (ast.List, ast.Tuple)):
        return all(_static_numpy_literal(item) for item in node.elts)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
        return isinstance(node.operand, ast.Constant) and type(node.operand.value) in (int, float, complex)
    return False


def _finite_source_number(value):
    if type(value) is int:
        try:
            value = float(value)
        except OverflowError:
            return False
    return type(value) is float and math.isfinite(value)


def _reserved_scene_field(name):
    return name in _CANONICAL_SCENE_FIELDS or name.startswith("_noon_")


def _numpy_numeric_expression(node, arrays, containers, module_literals, local=()):
    if _static_numpy_literal(node):
        return True
    if isinstance(node, ast.Name):
        return node.id in arrays or node.id in containers or node.id in local or node.id in module_literals
    if isinstance(node, (ast.List, ast.Tuple)):
        return all(_numpy_numeric_expression(item, arrays, containers, module_literals, local)
                   for item in node.elts)
    if isinstance(node, ast.BinOp):
        return (_numpy_numeric_expression(node.left, arrays, containers, module_literals, local)
                and _numpy_numeric_expression(node.right, arrays, containers, module_literals, local))
    if isinstance(node, ast.UnaryOp):
        return _numpy_numeric_expression(node.operand, arrays, containers, module_literals, local)
    if isinstance(node, ast.Attribute) and node.attr in {"real", "imag"}:
        return _numpy_array_expression(node.value, arrays)
    if isinstance(node, ast.Subscript):
        return ((_numpy_array_expression(node.value, arrays)
                 or isinstance(node.value, ast.Name) and node.value.id in containers | set(local))
                and _numpy_index_expression(node.slice, arrays, module_literals, local))
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
        if node.func.id in {"int", "float", "complex", "abs", "round", "min", "max", "len"}:
            return (bool(node.args) and not node.keywords
                    and all(_numpy_numeric_expression(arg, arrays, containers, module_literals, local)
                            for arg in node.args))
        if node.func.id == "range":
            return bool(node.args) and not node.keywords and all(
                _numpy_numeric_expression(arg, arrays, containers, module_literals, local) for arg in node.args)
    if isinstance(node, ast.Call):
        path = _attribute_path(node.func)
        if path in {"np.all", "np.isfinite", "np.max", "np.min", "np.ptp", "np.sum"}:
            return bool(node.args) and (
                _numpy_array_expression(node.args[0], arrays)
                or _numpy_numeric_expression(node.args[0], arrays, containers, module_literals, local))
    if isinstance(node, ast.ListComp):
        local_names = set(local)
        for generator in node.generators:
            if not _numpy_numeric_expression(generator.iter, arrays, containers, module_literals, local_names):
                return False
            target = generator.target.elts if isinstance(generator.target, (ast.Tuple, ast.List)) else [generator.target]
            local_names.update(item.id for item in target if isinstance(item, ast.Name))
            if any(not _numpy_numeric_expression(test, arrays, containers, module_literals, local_names)
                   for test in generator.ifs):
                return False
        return _numpy_numeric_expression(node.elt, arrays, containers, module_literals, local_names)
    return False


def _numpy_index_expression(node, arrays, module_literals, integer_names):
    if node is None:
        return True
    if isinstance(node, ast.Constant):
        return node.value is None or type(node.value) is int
    if isinstance(node, ast.Name):
        value = module_literals.get(node.id)
        return node.id in integer_names or isinstance(value, ast.Constant) and type(value.value) is int
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
        return _numpy_index_expression(node.operand, arrays, module_literals, integer_names)
    if isinstance(node, ast.Slice):
        return all(value is None or _numpy_index_expression(value, arrays, module_literals, integer_names)
                   for value in (node.lower, node.upper, node.step))
    if isinstance(node, (ast.Tuple, ast.List)):
        return all(_numpy_index_expression(item, arrays, module_literals, integer_names) for item in node.elts)
    if _numpy_array_expression(node, arrays):
        return True
    if isinstance(node, ast.BinOp) and isinstance(node.op, (ast.Add, ast.Sub, ast.Mult, ast.FloorDiv, ast.Mod)):
        return (_numpy_index_expression(node.left, arrays, module_literals, integer_names)
                and _numpy_index_expression(node.right, arrays, module_literals, integer_names))
    if _static_numpy_literal(node) and isinstance(node, (ast.List, ast.Tuple)):
        return True
    return False


def _integer_expression(node, names, module_literals):
    if isinstance(node, ast.Constant):
        return type(node.value) is int
    if isinstance(node, ast.Name):
        value = module_literals.get(node.id)
        return node.id in names or (isinstance(value, ast.Constant) and type(value.value) is int)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
        return _integer_expression(node.operand, names, module_literals)
    if isinstance(node, ast.BinOp) and isinstance(node.op, (ast.Add, ast.Sub, ast.Mult, ast.FloorDiv, ast.Mod,
                                                            ast.LShift, ast.RShift, ast.BitAnd, ast.BitOr, ast.BitXor)):
        return (_integer_expression(node.left, names, module_literals)
                and _integer_expression(node.right, names, module_literals))
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in {"int", "len", "min", "max"}:
        return bool(node.args) and not node.keywords and all(
            _integer_expression(argument, names, module_literals) for argument in node.args)
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "bit_length":
        return not node.args and not node.keywords and _integer_expression(node.func.value, names, module_literals)
    return False


def _target_names(target):
    return target.elts if isinstance(target, (ast.Tuple, ast.List)) else [target]


def _integer_iteration_targets(iterator, target, known, module_literals):
    if (isinstance(iterator, ast.Call) and isinstance(iterator.func, ast.Name)
            and iterator.func.id == "range" and iterator.args and not iterator.keywords
            and all(_integer_expression(arg, known, module_literals) for arg in iterator.args)):
        return {item.id for item in _target_names(target) if isinstance(item, ast.Name)}
    if (isinstance(iterator, ast.Call) and isinstance(iterator.func, ast.Name)
            and iterator.func.id == "enumerate" and len(iterator.args) == 1 and not iterator.keywords
            and isinstance(target, (ast.Tuple, ast.List)) and len(target.elts) == 2
            and isinstance(target.elts[0], ast.Name)):
        return {target.elts[0].id}
    literal = module_literals.get(iterator.id) if isinstance(iterator, ast.Name) else iterator
    if (isinstance(literal, (ast.List, ast.Tuple))
            and all(_integer_expression(item, known, module_literals) for item in literal.elts)):
        return {item.id for item in _target_names(target) if isinstance(item, ast.Name)}
    return set()


def _integer_names_at_node(body, node, parameters, module_literals, *, names=None, parents=None):
    names = set(_integer_names_in_body(body, parameters, module_literals) if names is None else names)
    if parents is None:
        parents = {}
        for statement in body:
            for parent in ast.walk(statement):
                for child in ast.iter_child_nodes(parent):
                    parents[child] = parent
    ancestors = []
    current = parents.get(node)
    while current is not None:
        ancestors.append(current)
        current = parents.get(current)

    def bind_comprehension_generators(generators, stop_after=None):
        for generator in generators:
            if any(candidate is node for candidate in ast.walk(generator.iter)):
                break
            proven = _integer_iteration_targets(generator.iter, generator.target, names, module_literals)
            names.difference_update(item.id for item in _target_names(generator.target)
                                    if isinstance(item, ast.Name))
            names.update(proven)
            in_filter = any(candidate is node for condition in generator.ifs for candidate in ast.walk(condition))
            if generator is stop_after or in_filter:
                break

    for ancestor in ancestors:
        if isinstance(ancestor, (ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)):
            bind_comprehension_generators(ancestor.generators)
            continue
        if not isinstance(ancestor, ast.comprehension):
            continue
        owner = parents.get(ancestor)
        if isinstance(owner, (ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)):
            bind_comprehension_generators(owner.generators, ancestor)
    return names


def _integer_names_in_body(body, parameters, module_literals):

    nodes = list(_walk_scope(body))
    names = set(parameters)
    changed = True
    while changed:
        changed = False
        for node in nodes:
            origins = []
            if isinstance(node, ast.Assign):
                origins.extend((target, _integer_expression(node.value, names, module_literals))
                               for target in node.targets)
            elif isinstance(node, ast.AnnAssign) and node.value is not None:
                origins.append((node.target, _integer_expression(node.value, names, module_literals)))
            elif isinstance(node, ast.AugAssign):
                origins.append((node.target, isinstance(node.target, ast.Name) and node.target.id in names
                                and _integer_expression(node.value, names, module_literals)
                                and isinstance(node.op, (ast.Add, ast.Sub, ast.Mult, ast.FloorDiv,
                                                         ast.Mod, ast.LShift, ast.RShift, ast.BitAnd,
                                                         ast.BitOr, ast.BitXor))))
            elif isinstance(node, (ast.For, ast.AsyncFor)):
                proven = _integer_iteration_targets(node.iter, node.target, names, module_literals)
                origins.extend((item, isinstance(item, ast.Name) and item.id in proven)
                               for item in _target_names(node.target))
            for target, proven in origins:
                if proven:
                    for item in _target_names(target):
                        if isinstance(item, ast.Name) and item.id not in names:
                            names.add(item.id)
                            changed = True

    events = {}

    def event(name, proven):
        events.setdefault(name, []).append(bool(proven))

    for node in nodes:
        if isinstance(node, ast.Assign):
            proven = _integer_expression(node.value, names, module_literals)
            for target in node.targets:
                for item in _target_names(target):
                    if isinstance(item, ast.Name):
                        event(item.id, proven)
        elif isinstance(node, ast.AnnAssign):
            if isinstance(node.target, ast.Name):
                event(node.target.id, node.value is not None
                      and _integer_expression(node.value, names, module_literals))
        elif isinstance(node, ast.AugAssign) and isinstance(node.target, ast.Name):
            event(node.target.id, node.target.id in names
                  and _integer_expression(node.value, names, module_literals)
                  and isinstance(node.op, (ast.Add, ast.Sub, ast.Mult, ast.FloorDiv, ast.Mod,
                                           ast.LShift, ast.RShift, ast.BitAnd, ast.BitOr, ast.BitXor)))
        elif isinstance(node, (ast.For, ast.AsyncFor)):
            proven = _integer_iteration_targets(node.iter, node.target, names, module_literals)
            for item in _target_names(node.target):
                if isinstance(item, ast.Name):
                    event(item.id, item.id in proven)
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            event(node.name, False)

    facts = names | set(parameters)
    while True:
        invalid = {name for name, origins in events.items() if not all(origins)}
        narrowed = facts - invalid
        if narrowed == facts:
            return facts
        facts = narrowed


def _integer_index_container_names(body, arrays, integer_names, module_literals):
    bindings = _SourceBindings()
    for statement in body:
        bindings.visit(statement)
    parents = {}
    for statement in body:
        for parent in ast.walk(statement):
            for child in ast.iter_child_nodes(parent):
                parents[child] = parent
    candidates = {statement.targets[0].id for statement in _walk_scope(body)
                  if isinstance(statement, ast.Assign) and len(statement.targets) == 1
                  and isinstance(statement.targets[0], ast.Name)
                  and isinstance(statement.value, ast.List)
                  and all(_integer_expression(item, integer_names, module_literals)
                          for item in statement.value.elts)
                  and bindings.binding_counts.get(statement.targets[0].id) == 1}
    safe = set()
    for name in candidates:
        valid = True
        for node in (node for statement in body for node in ast.walk(statement)):
            if not isinstance(node, ast.Name) or node.id != name:
                continue
            parent = parents.get(node)
            if isinstance(node.ctx, ast.Store):
                valid &= (isinstance(parent, ast.Assign) and isinstance(parent.value, ast.List)
                          and all(_integer_expression(item, integer_names, module_literals)
                                  for item in parent.value.elts) and node in parent.targets)
            elif isinstance(parent, ast.Attribute) and parent.value is node and parent.attr == "append":
                call = parents.get(parent)
                valid &= (isinstance(call, ast.Call) and len(call.args) == 1 and not call.keywords
                          and _integer_expression(call.args[0], integer_names, module_literals))
            elif isinstance(parent, ast.Subscript) and parent.slice is node:
                valid &= (isinstance(parent.ctx, ast.Load)
                          and _numpy_array_expression(parent.value, arrays))
            elif (isinstance(parent, ast.Call) and isinstance(parent.func, ast.Name)
                  and parent.func.id == "len" and parent.args == [node]):
                valid &= True
            else:
                valid = False
            if not valid:
                break
        if valid:
            safe.add(name)
    return safe


def _numpy_array_collection(node, arrays, module_literals, array_functions=()):
    if _numpy_array_expression(node, arrays, array_functions):
        return True
    if _static_numpy_literal(node):
        return True
    if isinstance(node, ast.Name) and node.id in module_literals:
        return _static_numpy_literal(module_literals[node.id])
    return isinstance(node, (ast.List, ast.Tuple)) and all(
        _numpy_array_collection(item, arrays, module_literals, array_functions) for item in node.elts)


def _numpy_scalar_names_in_body(body, parameters, arrays, containers, module_literals):
    names = set(parameters)
    changed = True
    while changed:
        changed = False
        for node in _walk_scope(body):
            if isinstance(node, (ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)):
                for generator in node.generators:
                    if (_numpy_array_expression(generator.iter, arrays)
                            or _numpy_numeric_expression(generator.iter, arrays, containers, module_literals, names)):
                        target = generator.target.elts if isinstance(generator.target, (ast.Tuple, ast.List)) else [generator.target]
                        for item in target:
                            if isinstance(item, ast.Name) and item.id not in names:
                                names.add(item.id)
                                changed = True
            if isinstance(node, ast.For):
                iteration = node.iter
                proven = _numpy_array_expression(iteration, arrays) or _numpy_numeric_expression(
                    iteration, arrays, containers, module_literals, names)
                if proven:
                    target = node.target.elts if isinstance(node.target, (ast.Tuple, ast.List)) else [node.target]
                    for item in target:
                        if isinstance(item, ast.Name) and item.id not in names:
                            names.add(item.id)
                            changed = True
            if not isinstance(node, ast.Assign):
                continue
            if not _numpy_numeric_expression(node.value, arrays, containers, module_literals, names):
                continue
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id not in names:
                    names.add(target.id)
                    changed = True
    invalid = set()
    for node in _walk_scope(body):
        if not isinstance(node, ast.Assign):
            continue
        if _numpy_numeric_expression(node.value, arrays, containers, module_literals, names):
            continue
        for target in node.targets:
            if isinstance(target, ast.Name):
                invalid.add(target.id)
    return names - invalid


def _loaded_names(node):
    return {item.id for item in ast.walk(node)
            if isinstance(item, ast.Name) and isinstance(item.ctx, ast.Load)}


def _scope_name_dependencies(body):
    dependencies = {}
    for node in _walk_scope(body):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    dependencies[target.id] = _loaded_names(node.value)
    return dependencies


def _safe_numpy_containers(body):
    """Prove only the local sampled-point list is safe numeric input."""
    bindings = _SourceBindings()
    for statement in body:
        bindings.visit(statement)
    svg_names = set()
    contour_names = set()
    safe_appends = set()
    for statement in _walk_scope(body):
        if (isinstance(statement, ast.Assign) and len(statement.targets) == 1
                and isinstance(statement.targets[0], ast.Name)
                and isinstance(statement.value, ast.Call)
                and isinstance(statement.value.func, ast.Attribute)
                and statement.value.func.attr == "from_string"
                and isinstance(statement.value.func.value, ast.Name)
                and statement.value.func.value.id == "SVGMobject"
                and bindings.binding_counts.get(statement.targets[0].id) == 1):
            svg_names.add(statement.targets[0].id)
    safe_lists = set()
    for outer in (node for statement in body for node in ast.walk(statement)):
        if not isinstance(outer, ast.For) or not isinstance(outer.iter, ast.Call):
            continue
        iterator = outer.iter
        if (not isinstance(iterator.func, ast.Attribute) or iterator.func.attr != "family_members_with_points"
                or not isinstance(iterator.func.value, ast.Name) or iterator.func.value.id not in svg_names):
            continue
        member_targets = outer.target.elts if isinstance(outer.target, (ast.Tuple, ast.List)) else [outer.target]
        member_names = {item.id for item in member_targets if isinstance(item, ast.Name)}
        if len(member_names) != 1 or any(bindings.binding_counts.get(name) != 1 for name in member_names):
            continue
        for inner in (node for statement in outer.body for node in ast.walk(statement)):
            if not isinstance(inner, ast.For) or not isinstance(inner.iter, ast.Call):
                continue
            subpaths = inner.iter
            if (not isinstance(subpaths.func, ast.Attribute) or subpaths.func.attr != "get_subpaths"
                    or not isinstance(subpaths.func.value, ast.Name)
                    or subpaths.func.value.id not in member_names):
                continue
            point_targets = inner.target.elts if isinstance(inner.target, (ast.Tuple, ast.List)) else [inner.target]
            point_names = {item.id for item in point_targets if isinstance(item, ast.Name)}
            if len(point_names) != 1 or any(bindings.binding_counts.get(name) != 1 for name in point_names):
                continue
            for statement in (node for item in inner.body for node in ast.walk(item)):
                if not isinstance(statement, ast.Expr) or not isinstance(statement.value, ast.Call):
                    continue
                append = statement.value
                if (not isinstance(append.func, ast.Attribute) or append.func.attr != "append"
                        or not isinstance(append.func.value, ast.Name) or len(append.args) != 1):
                    continue
                path = append.args[0]
                if (isinstance(path, ast.Call) and isinstance(path.func, ast.Attribute)
                        and path.func.attr == "set_points" and isinstance(path.func.value, ast.Call)
                        and isinstance(path.func.value.func, ast.Name) and path.func.value.func.id == "VMobject"
                        and len(path.args) == 1 and isinstance(path.args[0], ast.Name)
                        and path.args[0].id in point_names
                        and bindings.binding_counts.get(append.func.value.id) == 1):
                    contour_names.add(append.func.value.id)
                    safe_appends.add(id(append))
    safe_contours = {
        name for name in contour_names
        if bindings.binding_counts.get(name) == 1 and any(
            isinstance(statement, ast.Assign) and len(statement.targets) == 1
            and isinstance(statement.targets[0], ast.Name) and statement.targets[0].id == name
            and isinstance(statement.value, ast.List) and not statement.value.elts
            for statement in _walk_scope(body))
        and all(id(call) in safe_appends for call in _walk_scope(body)
                if isinstance(call, ast.Call) and isinstance(call.func, ast.Attribute)
                and call.func.attr == "append" and isinstance(call.func.value, ast.Name)
                and call.func.value.id == name)
    }
    safe_paths = {
        statement.targets[0].id for statement in _walk_scope(body)
        if isinstance(statement, ast.Assign) and len(statement.targets) == 1
        and isinstance(statement.targets[0], ast.Name)
        and isinstance(statement.value, ast.Subscript) and isinstance(statement.value.value, ast.Name)
        and statement.value.value.id in safe_contours
        and bindings.binding_counts.get(statement.targets[0].id) == 1
        and any(isinstance(candidate, ast.Call)
                and isinstance(candidate.func, ast.Attribute) and candidate.func.attr == "sort"
                and isinstance(candidate.func.value, ast.Name) and candidate.func.value.id in safe_contours
                for candidate in _walk_scope(body))
    }
    for statement in _walk_scope(body):
        if (isinstance(statement, ast.Assign) and len(statement.targets) == 1
                and isinstance(statement.targets[0], ast.Name)
                and isinstance(statement.value, ast.ListComp)):
            expression = statement.value.elt
            generator = statement.value.generators[0] if len(statement.value.generators) == 1 else None
            if (isinstance(expression, ast.Call) and isinstance(expression.func, ast.Attribute)
                    and expression.func.attr == "point_from_proportion"
                    and isinstance(expression.func.value, ast.Name)
                    and expression.func.value.id in safe_paths
                    and generator is not None
                    and isinstance(generator.iter, ast.Call)
                    and _attribute_path(generator.iter.func) == "np.linspace"
                    and bindings.binding_counts.get(statement.targets[0].id) == 1):
                safe_lists.add(statement.targets[0].id)
    if not safe_lists or len(safe_contours) != 1 or len(safe_paths) != 1:
        return set()

    parents = {}
    for statement in body:
        for parent in ast.walk(statement):
            for child in ast.iter_child_nodes(parent):
                parents[child] = parent

    def closed_uses(name, permitted):
        for node in (node for statement in body for node in ast.walk(statement)):
            if not isinstance(node, ast.Name) or node.id != name:
                continue
            parent = parents.get(node)
            if not permitted(node, parent, parents):
                return False
        return True

    contours_name = next(iter(safe_contours))
    path_name = next(iter(safe_paths))
    sampled_name = next(iter(safe_lists))

    def contours_use(node, parent, parents):
        if isinstance(node.ctx, ast.Store):
            return (isinstance(parent, ast.Assign) and isinstance(parent.value, ast.List)
                    and not parent.value.elts and node in parent.targets)
        if (isinstance(parent, ast.Call) and isinstance(parent.func, ast.Name)
                and parent.func.id == "len" and parent.args == [node]):
            return True
        if not isinstance(parent, (ast.Attribute, ast.Subscript)):
            return False
        if isinstance(parent, ast.Attribute) and parent.value is node:
            return (parent.attr in {"append", "sort"} and isinstance(parents.get(parent), ast.Call)
                    and (parent.attr == "sort" or id(parents[parent]) in safe_appends))
        if isinstance(parent, ast.Subscript) and parent.value is node:
            assignment = parents.get(parent)
            return (isinstance(parent.ctx, ast.Load) and isinstance(assignment, ast.Assign)
                    and len(assignment.targets) == 1 and isinstance(assignment.targets[0], ast.Name)
                    and assignment.targets[0].id == path_name)
        return False

    def sampled_use(node, parent, parents):
        if isinstance(node.ctx, ast.Store):
            return (isinstance(parent, ast.Assign) and isinstance(parent.value, ast.ListComp)
                    and node in parent.targets)
        if isinstance(parent, ast.comprehension) and parent.iter is node:
            comp = parents.get(parent)
            call = parents.get(comp) if comp is not None else None
            return (isinstance(comp, ast.ListComp) and isinstance(call, ast.Call)
                    and _attribute_path(call.func) == "np.array")
        return False

    def path_use(node, parent, parents):
        if isinstance(node.ctx, ast.Store):
            return (isinstance(parent, ast.Assign) and isinstance(parent.value, ast.Subscript)
                    and node in parent.targets)
        if isinstance(parent, ast.Attribute) and parent.value is node and parent.attr == "point_from_proportion":
            call = parents.get(parent)
            return isinstance(call, ast.Call) and isinstance(parents.get(call), ast.ListComp)
        if isinstance(parent, ast.Attribute) and parent.value is node and parent.attr == "get_arc_length":
            call = parents.get(parent)
            current = parents.get(call)
            while current is not None:
                if (isinstance(current, ast.Call) and isinstance(current.func, ast.Attribute)
                        and current.func.attr == "sort" and isinstance(current.func.value, ast.Name)
                        and current.func.value.id == contours_name):
                    return True
                current = parents.get(current)
        return False

    if not (closed_uses(contours_name, contours_use)
            and closed_uses(path_name, path_use)
            and closed_uses(sampled_name, sampled_use)):
        return set()
    return safe_lists


def _nested_scope_references(body, arrays, numeric_containers, integer_names, module_literals,
                             integer_parameters):
    if not arrays and not numeric_containers:
        return False
    parents = {}
    for statement in body:
        for parent in ast.walk(statement):
            for child in ast.iter_child_nodes(parent):
                parents[child] = parent
    for statement in body:
        for scope in ast.walk(statement):
            if not isinstance(scope, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda, ast.ClassDef)):
                continue
            nested_names = set(integer_names)
            if isinstance(scope, (ast.FunctionDef, ast.AsyncFunctionDef)):
                parameters = {arg.arg for arg in [*scope.args.posonlyargs, *scope.args.args,
                                                   *scope.args.kwonlyargs]}
                nested_names.difference_update(parameters)
                nested_names.update(integer_parameters.get(scope.name, ()))
                nested_names = _integer_names_in_body(scope.body, nested_names, module_literals)
            for node in ast.walk(scope):
                if not isinstance(node, ast.Name) or not isinstance(node.ctx, ast.Load):
                    continue
                if node.id in numeric_containers:
                    return True
                if node.id in arrays:
                    parent = parents.get(node)
                    if not (isinstance(parent, ast.Subscript) and parent.value is node
                            and isinstance(parent.ctx, ast.Load)
                            and _numpy_index_expression(parent.slice, arrays, module_literals, nested_names)):
                        return True
    return False


def _fresh_numpy_writes(body, arrays, array_functions, containers=(), module_literals=None, scalars=()):
    fresh_calls = _NUMPY_ARRAY_CALLS
    safe_writes = set()
    state = {}

    def origin(node, current):
        if isinstance(node, ast.Name):
            return current.get(node.id)
        if isinstance(node, ast.Subscript):
            return "view" if _numpy_array_expression(node, arrays, array_functions) else None
        if isinstance(node, ast.Call):
            path = _attribute_path(node.func)
            if path == "np.asarray":
                return "view"
            if path in fresh_calls:
                return "fresh"
            if path == "np.abs" and node.args and _numpy_array_expression(node.args[0], arrays, array_functions):
                return "fresh"
            if isinstance(node.func, ast.Attribute) and node.func.attr == "copy" \
                    and origin(node.func.value, current) in {"view", "fresh"}:
                return "fresh"
            if isinstance(node.func, ast.Attribute) and node.func.attr == "reshape" \
                    and origin(node.func.value, current) in {"view", "fresh"}:
                return "view"
        if isinstance(node, (ast.BinOp, ast.UnaryOp, ast.Compare)) \
                and _numpy_array_expression(node, arrays, array_functions):
            return "fresh"
        return None

    def block(statements, current):
        current = current.copy()
        for statement in statements:
            if isinstance(statement, ast.Assign):
                value_origin = origin(statement.value, current)
                for target in statement.targets:
                    targets = target.elts if isinstance(target, (ast.Tuple, ast.List)) else [target]
                    for item in targets:
                        if isinstance(item, ast.Subscript) and isinstance(item.value, ast.Name):
                            index = item.slice
                            integer_index = (isinstance(index, ast.Constant) and type(index.value) is int
                                             or isinstance(index, ast.UnaryOp)
                                             and isinstance(index.op, (ast.UAdd, ast.USub))
                                             and isinstance(index.operand, ast.Constant)
                                             and type(index.operand.value) is int)
                            if (current.get(item.value.id) == "fresh" and integer_index
                                    and _numpy_numeric_expression(statement.value, arrays, set(containers),
                                                                  module_literals or {}, set(scalars))):
                                safe_writes.add(id(item))
                        elif isinstance(item, ast.Name) and item.id in arrays:
                            current[item.id] = value_origin
            elif isinstance(statement, ast.If):
                left = block(statement.body, current)
                right = block(statement.orelse, current) if statement.orelse else current.copy()
                current = {name: left.get(name) if left.get(name) == right.get(name) else None
                           for name in left.keys() | right.keys()}
            elif isinstance(statement, (ast.For, ast.While)):
                after = block(statement.body, current)
                current = {name: current.get(name) if current.get(name) == after.get(name) else None
                           for name in current.keys() | after.keys()}
        return current

    block(body, state)
    return safe_writes


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
                    fresh = fresh or any(isinstance(items, ast.Call) and isinstance(items.func, ast.Name)
                                         and items.func.id == "array"
                                         for items, _ in ((value.left, value.right), (value.right, value.left)))
                if isinstance(value, ast.Call) and isinstance(value.func, ast.Name) \
                        and value.func.id in {"bytearray", "array"}:
                    fresh = True
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
        self.tree = tree
        self.admitted_lagged_map_loads = set()
        self.admitted_plot_loads = set()
        self.admitted_spatial_constructor_loads = set()
        self.spatial_method_depth = 0
        self.linear_method_depth = 0
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
        self.module_string_literals = {
            node.targets[0].id: node.value.value for node in tree.body
            if isinstance(node, ast.Assign) and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
            and bindings.binding_counts.get(node.targets[0].id) == 1
            and isinstance(node.value, ast.Constant) and type(node.value.value) is str
        }
        self.module_imports = {node for node in tree.body if isinstance(node, (ast.Import, ast.ImportFrom))}
        self.module_modules = {}
        self.module_import_functions = {}
        for node in self.module_imports:
            if isinstance(node, ast.Import):
                for alias in node.names:
                    self.module_modules[alias.asname or alias.name.split(".")[0]] = alias.name
            else:
                for alias in node.names:
                    if alias.name != "*":
                        self.module_import_functions[alias.asname or alias.name] = (node.module, alias.name)
        self.shader_function_names = set()
        self.shader_helper_specs = {}
        self.shader_program_call_specs = {}
        self.admitted_shader_loads = set()
        self.admitted_shader_attributes = set()
        self.admitted_shader_helper_loads = set()
        self.admitted_typed_paint_loads = set()
        self.shader_call_values = {}
        self.shader_expression_values = {}
        self.shader_values = []
        self.shader_control_depth = 0
        self.scopes = [set()]
        for name, function in self.module_functions.items():
            body = function.body
            if (self.module_binding_counts.get(name) != 1
                    or not isinstance(function, ast.FunctionDef)
                    or function.decorator_list or function.returns
                    or len(function.args.args) != 1 or function.args.posonlyargs
                    or function.args.kwonlyargs or function.args.defaults
                    or function.args.kw_defaults or function.args.vararg or function.args.kwarg
                    or len(body) != 1 or not isinstance(body[0], ast.Return)
                    or not isinstance(body[0].value, ast.Call)):
                continue
            call = body[0].value
            coordinate = next((keyword.value for keyword in call.keywords
                               if keyword.arg == "coordinates"), None)
            if (not isinstance(coordinate, ast.Name)
                    or coordinate.id != function.args.args[0].arg):
                continue
            try:
                schema = self._shader_program_spec(call, coordinate_parameter=coordinate.id)
            except _OpaqueSource:
                continue
            self.shader_helper_specs[name] = schema
        self.shader_function_names = set(self.shader_helper_specs)
        self.resource_preparation_imports = {
            alias.name
            for node in tree.body
            if isinstance(node, ast.ImportFrom) and node.module in {"noon", "manim"}
            for alias in node.names
            if alias.name in _SOURCE_RESOURCE_PREPARATIONS and alias.asname is None
        }
        self.controlled_text_imports = {
            alias.name
            for node in tree.body
            if isinstance(node, ast.ImportFrom) and node.module in {"noon", "manim"}
            for alias in node.names
            if alias.name in _SOURCE_CONTROLLED_TEXT_COMPILERS and alias.asname is None
        }
        self.admitted_resource_preparation_loads = set()
        self.admitted_controlled_text_loads = set()
        self.resource_preparation_scopes = []
        self.admitted_regex_attributes = []
        self.admitted_local_group_attributes = []
        self.admitted_optional_scene_getters = set()
        self.unreachable_scene_nodes = set()
        self.scene_helper_names = set()
        self.admitted_scene_owned_attributes = set()
        self.admitted_scene_helper_attributes = set()
        self.numpy_module_functions = {
            name for name, function in self.module_functions.items()
            if bindings.binding_counts.get(name) == 1 and bindings.locations.get(name) is function
        }
        self.source_callable_imports = set()
        for node in tree.body:
            if isinstance(node, ast.ImportFrom) and node.module in {"noon", "manim"}:
                for alias in node.names:
                    if alias.name in _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS:
                        self.source_callable_imports.add(alias.asname or alias.name)
            elif isinstance(node, ast.ImportFrom) and node.module in _SOURCE_MODULES:
                for alias in node.names:
                    if alias.name in _SOURCE_MODULES[node.module]:
                        self.source_callable_imports.add(alias.asname or alias.name)
        callable_sources = (_SOURCE_BUILTINS | _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS
                           | self.numpy_module_functions | self.source_callable_imports)
        self.module_callable_aliases = set()
        changed = True
        while changed:
            changed = False
            for node in tree.body:
                if (isinstance(node, ast.Assign) and len(node.targets) == 1
                        and isinstance(node.targets[0], ast.Name)
                        and bindings.binding_counts.get(node.targets[0].id) == 1
                        and isinstance(node.value, ast.Name)
                        and node.targets[0].id not in self.module_callable_aliases
                        and node.value.id in callable_sources | self.module_callable_aliases):
                    self.module_callable_aliases.add(node.targets[0].id)
                    changed = True
        self.numpy_functions = {
            node.name: node for node in ast.walk(tree)
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        }
        self.numpy_function_nodes = {
            function: _walk_scope(function.body) for function in self.numpy_functions.values()
        }
        self.numpy_function_outputs = {}
        self.numpy_array_functions = set()
        self.numpy_array_parameters = {name: set() for name in self.numpy_functions}
        self.numpy_scalar_parameters = {name: set() for name in self.numpy_functions}
        self.module_numpy_literals = {
            node.targets[0].id: node.value for node in tree.body
            if isinstance(node, ast.Assign) and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
            and bindings.binding_counts.get(node.targets[0].id) == 1
            and _static_numpy_literal(node.value)
        }
        self.module_integer_literals = {
            node.targets[0].id: node.value for node in tree.body
            if isinstance(node, ast.Assign) and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
            and bindings.binding_counts.get(node.targets[0].id) == 1
            and isinstance(node.value, ast.Constant) and type(node.value.value) is int
        }
        self.integer_record_schemas = self._integer_record_list_schemas()
        self.integer_row_parameters, self.integer_row_widths = self._integer_row_parameter_types()
        self.function_context = []
        self.numpy_integer_parameters = {name: set() for name in self.numpy_functions}
        integer_names_by_call = {}
        integer_names_by_scope = {}
        integer_parents_by_function = {}

        def integer_names_at_call(function, call, parameters):
            # Each call belongs to one immutable lexical body. Reuse its proof
            # across call sites with the same entry facts; only ancestor lookup
            # remains call-specific.
            parameter_facts = frozenset(parameters)
            key = (call, parameter_facts)
            if key not in integer_names_by_call:
                scope_key = (function, parameter_facts)
                if scope_key not in integer_names_by_scope:
                    integer_names_by_scope[scope_key] = _integer_names_in_body(
                        function.body, parameter_facts, self.module_numpy_literals)
                if function not in integer_parents_by_function:
                    parents = {}
                    for statement in function.body:
                        for parent in ast.walk(statement):
                            for child in ast.iter_child_nodes(parent):
                                parents[child] = parent
                    integer_parents_by_function[function] = parents
                integer_names_by_call[key] = _integer_names_at_node(
                    function.body, call, parameter_facts, self.module_numpy_literals,
                    names=integer_names_by_scope[scope_key],
                    parents=integer_parents_by_function[function])
            return integer_names_by_call[key]

        changed = True
        while changed:
            changed = False
            call_sites = {name: [] for name in self.numpy_functions}
            for caller, function in self.numpy_functions.items():
                for call in (node for node in self.numpy_function_nodes[function]
                             if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                             and node.func.id in self.numpy_functions):
                    call_sites[call.func.id].append((caller, function, call))
            for callee, sites in call_sites.items():
                if not sites:
                    continue
                function = self.numpy_functions[callee]
                formals = [arg.arg for arg in [*function.args.posonlyargs, *function.args.args,
                                                *function.args.kwonlyargs]]
                for formal in formals:
                    proven_at_any_call = False
                    for caller, caller_function, call in sites:
                        caller_names = integer_names_at_call(
                            caller_function, call, self.numpy_integer_parameters[caller])
                        actuals = dict(zip(formals, call.args))
                        actuals.update({keyword.arg: keyword.value for keyword in call.keywords
                                        if keyword.arg in formals})
                        actual = actuals.get(formal)
                        if actual is not None and _integer_expression(
                                actual, caller_names, self.module_numpy_literals):
                            proven_at_any_call = True
                            break
                    if proven_at_any_call and formal not in self.numpy_integer_parameters[callee]:
                        self.numpy_integer_parameters[callee].add(formal)
                        changed = True
        # Requiring every call site closes the candidate facts after recursive
        # and mutually recursive calls have received their entry facts.
        changed = True
        while changed:
            changed = False
            for callee, parameters in self.numpy_integer_parameters.items():
                sites = [(caller, function, call)
                         for caller, function in self.numpy_functions.items()
                         for call in self.numpy_function_nodes[function]
                         if isinstance(call, ast.Call) and isinstance(call.func, ast.Name)
                         and call.func.id == callee]
                formals = [arg.arg for arg in [*self.numpy_functions[callee].args.posonlyargs,
                                                *self.numpy_functions[callee].args.args,
                                                *self.numpy_functions[callee].args.kwonlyargs]]
                for formal in tuple(parameters):
                    proven_at_every_call = bool(sites)
                    for caller, function, call in sites:
                        caller_names = integer_names_at_call(
                            function, call, self.numpy_integer_parameters[caller])
                        actuals = dict(zip(formals, call.args))
                        actuals.update({keyword.arg: keyword.value for keyword in call.keywords
                                        if keyword.arg in formals})
                        actual = actuals.get(formal)
                        if actual is None or not _integer_expression(actual, caller_names, self.module_numpy_literals):
                            proven_at_every_call = False
                            break
                    if not proven_at_every_call:
                        parameters.remove(formal)
                        changed = True
        self.numpy_integer_functions_with_parameters = {
            name for name, parameters in self.numpy_integer_parameters.items() if parameters
        }
        self.admitted_integer_function_loads = set()
        changed = True
        while changed:
            changed = False
            for name, function in self.numpy_functions.items():
                formal = [arg.arg for arg in [*function.args.posonlyargs, *function.args.args,
                                               *function.args.kwonlyargs]]
                caller_parameters = set(formal)
                parameters = self.numpy_array_parameters[name]
                arrays = _array_names_in_body(function.body, parameters,
                                               self.numpy_array_functions,
                                               self.numpy_function_outputs)
                containers = _safe_numpy_containers(function.body)
                scalar_candidates = (set(formal) - parameters) | self.numpy_scalar_parameters[name]
                scalar_names = _numpy_scalar_names_in_body(function.body, scalar_candidates,
                                                            arrays, containers, self.module_numpy_literals)
                returns = [node.value for node in self.numpy_function_nodes[function]
                           if isinstance(node, ast.Return) and node.value is not None]
                if returns:
                    shape = None
                    for value in returns:
                        current = (tuple(_numpy_array_expression(item, arrays, self.numpy_array_functions)
                                         for item in value.elts)
                                   if isinstance(value, ast.Tuple) else
                                   (True,) if _numpy_array_expression(value, arrays, self.numpy_array_functions) else ())
                        if shape is None:
                            shape = current
                        elif shape != current:
                            shape = ()
                            break
                    if shape and self.numpy_function_outputs.get(name) != shape:
                        self.numpy_function_outputs[name] = shape
                        if len(shape) == 1 and shape[0]:
                            self.numpy_array_functions.add(name)
                        changed = True
                dependencies = _scope_name_dependencies(function.body)
                for np_call in (node for node in self.numpy_function_nodes[function]
                                if isinstance(node, ast.Call) and _attribute_path(node.func) in _NUMPY_CALLS):
                    for argument in np_call.args:
                        if (_numpy_numeric_expression(argument, arrays, containers,
                                                      self.module_numpy_literals, scalar_names)
                                and not _numpy_array_expression(argument, arrays,
                                                                self.numpy_array_functions)):
                            pending = list(_loaded_names(argument))
                            seen = set()
                            while pending:
                                candidate = pending.pop()
                                if candidate in seen:
                                    continue
                                seen.add(candidate)
                                if candidate in caller_parameters and candidate not in parameters \
                                        and candidate not in self.numpy_scalar_parameters[name]:
                                    self.numpy_scalar_parameters[name].add(candidate)
                                    changed = True
                                pending.extend(dependencies.get(candidate, ()))
                for call in (node for node in self.numpy_function_nodes[function]
                             if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                             and node.func.id in self.numpy_functions):
                    callee = call.func.id
                    callee_formals = [arg.arg for arg in [*self.numpy_functions[callee].args.posonlyargs,
                                                          *self.numpy_functions[callee].args.args,
                                                          *self.numpy_functions[callee].args.kwonlyargs]]
                    actuals = dict(zip(callee_formals, call.args))
                    actuals.update({keyword.arg: keyword.value for keyword in call.keywords
                                    if keyword.arg in callee_formals})
                    for formal in callee_formals:
                        actual = actuals.get(formal)
                        if actual is None:
                            continue
                        if (_numpy_array_expression(actual, arrays, self.numpy_array_functions)
                                and formal not in self.numpy_array_parameters[callee]):
                            self.numpy_array_parameters[callee].add(formal)
                            changed = True
                        if (formal in self.numpy_scalar_parameters[callee]
                                and _numpy_numeric_expression(actual, arrays, containers,
                                                              self.module_numpy_literals, scalar_names)):
                            pending = list(_loaded_names(actual))
                            seen = set()
                            while pending:
                                candidate = pending.pop()
                                if candidate in seen:
                                    continue
                                seen.add(candidate)
                                if candidate in caller_parameters and candidate not in parameters \
                                        and candidate not in self.numpy_scalar_parameters[name]:
                                    self.numpy_scalar_parameters[name].add(candidate)
                                    changed = True
                                pending.extend(dependencies.get(candidate, ()))
        self.numpy_array_functions_with_parameters = {
            name for name, parameters in self.numpy_array_parameters.items() if parameters
        }
        for name, parameters in self.numpy_array_parameters.items():
            self.numpy_scalar_parameters[name].difference_update(parameters)
        self.numpy_scalar_functions_with_parameters = {
            name for name, parameters in self.numpy_scalar_parameters.items() if parameters
        }
        self.admitted_image_loads = set()
        self.admitted_image_paint_loads = set()
        self.admitted_scene_audio_calls = set()
        self.admitted_audio_factory_attributes = set()
        self.admitted_svg_from_string_attributes = set()
        self.admitted_numpy_attributes = set()
        self.admitted_numpy_function_loads = set()
        self.numpy_import = None
        self.image_pixel_sources = [set()]
        self.image_pixel_numeric_values = [set()]
        self.image_numeric_values_by_body = {}
        self.pixel_factory_names = set()
        self.admitted_pixel_factory_loads = set()
        self.closed_image_resource_factories = {}
        self.admitted_image_resource_factory_loads = set()
        self.empty_pixel_list = object()
        self.has_image_constructor = any(
            isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
            and node.func.id in {"ImageMobject", "ImagePaint"} for node in ast.walk(tree)
        )
        if self.has_image_constructor:
            self.closed_png_encoders = {
                name for name in self.module_functions if self._closed_png_encoder(name)
            }
            self.closed_image_resource_factories = {
                name: result for name in self.module_functions
                if (result := self._closed_image_resource_factory(name)) is not None
            }
            self.admitted_image_resource_factory_loads = set()
            self.closed_pixel_factory_names = {
                name for name in self.module_functions if self._closed_pixel_proof(name)
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
        noon_names = (_SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS | _SOURCE_CONSTANTS
                      | _SOURCE_RESOURCE_PREPARATIONS | _SOURCE_CONTROLLED_TEXT_COMPILERS
                      | _SOURCE_SHADER_CONSTRUCTORS | _SOURCE_TYPED_PAINT_CONSTRUCTORS)
        imported = frozenset().union(*star_exports.values())
        conflicts = (imported - noon_names).intersection(bindings.names | _SOURCE_BUILTINS | {"context", "__name__"})
        # Optional namespace installation can export np before this source's
        # explicit canonical import replaces it. Other rebinding stays opaque.
        if self._canonical_numpy_import(bindings.locations.get("np")):
            conflicts -= {"np"}
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
        self.numpy_safe_containers = [set()]
        self.numpy_array_names = [set()]
        self.numpy_integer_names = [set()]
        self.numpy_scalar_names = [set()]
        self.safe_numpy_writes = [set()]
        self.local_discard_names = [set()]
        self.local_scope_names = [set()]
        self.local_function_names = [set()]
        self.safe_callable_aliases = [set()]
        self.members = _SOURCE_MEMBERS | frozenset().union(*_SOURCE_MODULES.values())
        # Constructor aliases and wildcard exports are proven only for direct
        # module declarations, never imports hidden in a conditional suite.
        self.code_constructors = {"Code"} | {
            alias.asname or alias.name for node in tree.body
            if isinstance(node, ast.ImportFrom) and node.module in {"noon", "manim"}
            for alias in node.names if alias.name == "Code"
        }
        self.admitted_code_loads = set()

    def _shader_role_unshadowed(self, name):
        if any(name in scope for scope in self.scopes[1:]):
            return False
        if name not in self.module_bindings:
            return True
        origin = self.module_binding_locations.get(name)
        return (isinstance(origin, ast.ImportFrom) and origin in self.tree.body
                and origin.module in {"noon", "manim"}
                and self.module_binding_counts.get(name) == 1
                and any(alias.name == name and alias.asname is None for alias in origin.names))

    def _shader_number(self, node):
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
            value = self._shader_number(node.operand)
            return None if value is None else value if isinstance(node.op, ast.UAdd) else -value
        if isinstance(node, ast.Constant) and type(node.value) in (int, float):
            try:
                return node.value if math.isfinite(node.value) else None
            except OverflowError:
                return None
        if isinstance(node, ast.Name):
            if node.id not in {"PI", "TAU", "DEGREES"} or not self._shader_role_unshadowed(node.id):
                return None
            return {"PI": math.pi, "TAU": math.tau, "DEGREES": math.pi / 180}.get(node.id)
        if isinstance(node, ast.BinOp) and isinstance(node.op, (ast.Add, ast.Sub, ast.Mult, ast.Div)):
            left, right = (self._shader_number(value) for value in (node.left, node.right))
            if left is None or right is None or isinstance(node.op, ast.Div) and right == 0:
                return None
            try:
                result = (left + right if isinstance(node.op, ast.Add) else
                          left - right if isinstance(node.op, ast.Sub) else
                          left * right if isinstance(node.op, ast.Mult) else left / right)
                return result if math.isfinite(result) else None
            except (OverflowError, ValueError):
                return None
        return None

    def _shader_container_reference(self, node, *, skip_calls=()):
        """Whether an expression can carry a proven program/key collection."""
        pending = [node]
        while pending:
            item = pending.pop()
            if id(item) in skip_calls:
                continue
            value = self._shader_value(item)
            if value is not None and value[0] in {"programs", "keys"}:
                return True
            pending.extend(ast.iter_child_nodes(item))
        return False

    def _reject_shader_collection_mutation(self, node, receiver):
        value = self._shader_value(receiver)
        mutating_methods = {
            "append", "extend", "insert", "pop", "clear", "remove",
            "reverse", "sort", "update", "add", "discard",
            "difference_update", "intersection_update", "symmetric_difference_update",
        }
        subscript_store = isinstance(node, ast.Subscript) and not isinstance(node.ctx, ast.Load)
        augmented_store = isinstance(node, ast.AugAssign)
        method_store = (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                        and node.func.attr in mutating_methods)
        if value and value[0] in {"programs", "keys"} and (
            subscript_store or augmented_store or method_store
        ):
            raise _OpaqueSource(node, "unproven_shader_collection_mutation")

    def _shader_color(self, node):
        if (isinstance(node, ast.Name) and node.id in _SOURCE_COLOR_CONSTANTS
                and self._shader_role_unshadowed(node.id)):
            return True
        if (not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name)
                or node.func.id != "Color" or not self._shader_role_unshadowed(node.func.id)
                or node.keywords or len(node.args) not in {3, 4}
                or any(self._shader_number(value) is None for value in node.args)):
            return False
        return True

    def _shader_parameter_spec(self, node):
        if (not isinstance(node, ast.Call) or not isinstance(node.func, ast.Attribute)
                or not isinstance(node.func.value, ast.Name) or node.func.value.id != "ShaderParameter"
                or not self._shader_role_unshadowed(node.func.value.id)
                or node.func.attr not in {"scalar", "color"} or len(node.args) != 1
                or not isinstance(node.args[0], ast.Constant) or type(node.args[0].value) is not str
                or not node.args[0].value or any(keyword.arg is None for keyword in node.keywords)):
            raise _OpaqueSource(node, "unproven_shader_schema")
        values = {keyword.arg: keyword.value for keyword in node.keywords}
        if len(values) != len(node.keywords):
            raise _OpaqueSource(node, "unproven_shader_schema")
        name = node.args[0].value
        if node.func.attr == "scalar":
            if set(values) not in ({"default", "minimum", "maximum"},
                                   {"default", "minimum", "maximum", "unit"}):
                raise _OpaqueSource(node, "unproven_shader_schema")
            numbers = [self._shader_number(values[key]) for key in ("default", "minimum", "maximum")]
            unit = values.get("unit")
            valid_unit = (unit is None or isinstance(unit, ast.Constant) and type(unit.value) is str
                          and unit.value in {"unitless", "normalized", "scene", "pixels", "radians", "seconds"})
            if (any(value is None for value in numbers) or numbers[1] > numbers[2]
                    or not numbers[1] <= numbers[0] <= numbers[2] or not valid_unit):
                raise _OpaqueSource(node, "unproven_shader_schema")
        elif set(values) != {"default"} or not self._shader_color(values["default"]):
            raise _OpaqueSource(node, "unproven_shader_schema")
        self.admitted_shader_attributes.add(id(node.func))
        self.admitted_shader_loads.add(id(node.func.value))
        return name

    def _shader_program_spec(self, node, coordinate_parameter=None):
        if (not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name)
                or node.func.id != "WgslEffectProgram" or not self._shader_role_unshadowed(node.func.id)
                or len(node.args) != 1 or any(isinstance(arg, ast.Starred) for arg in node.args)
                or any(keyword.arg is None for keyword in node.keywords)):
            raise _OpaqueSource(node, "unproven_shader_program")
        source = node.args[0]
        source_text = (source.value if isinstance(source, ast.Constant) and type(source.value) is str
                       else self.module_string_literals.get(source.id)
                       if isinstance(source, ast.Name)
                       and not any(source.id in scope for scope in self.scopes[1:]) else None)
        if source_text is None or not source_text.strip() or len(source_text.encode("utf-8")) > 1024 * 1024:
            raise _OpaqueSource(source, "unproven_shader_source")
        values = {keyword.arg: keyword.value for keyword in node.keywords}
        if len(values) != len(node.keywords) or not set(values).issubset(
                {"coordinates", "parameters", "authored_time", "seed"}):
            raise _OpaqueSource(node, "unproven_shader_program")
        coordinates = values.get("coordinates")
        if coordinate_parameter is not None and isinstance(coordinates, ast.Name) \
                and coordinates.id == coordinate_parameter:
            pass
        elif coordinates is not None and (
                not isinstance(coordinates, ast.Constant) or type(coordinates.value) is not str
                or coordinates.value not in {"local", "scene", "pixels"}):
            raise _OpaqueSource(coordinates, "unproven_shader_program")
        for dependency in ("authored_time", "seed"):
            value = values.get(dependency)
            if value is not None and (not isinstance(value, ast.Constant) or value.value is not False):
                raise _OpaqueSource(value, "unproven_shader_dependency")
        parameters = values.get("parameters")
        if parameters is None or not isinstance(parameters, ast.List):
            raise _OpaqueSource(parameters or node, "unproven_shader_schema")
        names = [self._shader_parameter_spec(item) for item in parameters.elts]
        if len(names) != len(set(names)):
            raise _OpaqueSource(parameters, "unproven_shader_schema")
        self.admitted_shader_loads.add(id(node.func))
        self.shader_program_call_specs[id(node)] = frozenset(names)
        return frozenset(names)

    def _shader_value(self, node):
        if id(node) in self.shader_expression_values:
            return self.shader_expression_values[id(node)]
        if id(node) in self.shader_call_values:
            return self.shader_call_values[id(node)]
        if isinstance(node, ast.Name):
            if self._shader_number(node) is not None:
                return ("number",)
            return next((values.get(node.id) for values in reversed(self.shader_values)
                         if node.id in values), None)
        if isinstance(node, ast.Constant) and type(node.value) in (int, float):
            return ("number",)
        if isinstance(node, (ast.List, ast.Tuple)):
            items = tuple(self._shader_value(item) for item in node.elts)
            if isinstance(node, ast.List) and items and all(item and item[0] in {"program", "programs"} for item in items):
                return ("programs", tuple(entry for item in items for entry in item[1]))
            if isinstance(node, ast.List) and items and all(item and item[0] in {"keys", "key"} for item in items):
                return ("keys", tuple(entry for item in items for entry in item[1]))
            return ("sequence", items)
        if isinstance(node, ast.Subscript):
            base, index = self._shader_value(node.value), self._shader_number(node.slice)
            if base and base[0] == "sequence" and index is not None and int(index) == index:
                try:
                    return base[1][int(index)]
                except IndexError:
                    return None
        if isinstance(node, ast.BinOp) and self._shader_number(node) is not None:
            return ("number",)
        if isinstance(node, ast.ListComp):
            return self.shader_expression_values.get(id(node))
        if (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id == "zip" and self._shader_role_unshadowed("zip")):
            return ("zip", tuple(self._shader_value(arg) for arg in node.args))
        return None

    def _play_shader_zip_iterators(self, node):
        """Allow only the showcase's direct zipped set_effect generator in play."""
        if (not isinstance(node.func, ast.Attribute) or node.func.attr != "play"
                or not isinstance(node.func.value, ast.Name) or node.func.value.id != "self"):
            return set()
        allowed = set()
        for argument in node.args:
            if not isinstance(argument, ast.Starred) or not isinstance(argument.value, ast.GeneratorExp):
                continue
            generator = argument.value
            if (len(generator.generators) != 1
                    or not isinstance(generator.elt, ast.Call)
                    or not isinstance(generator.elt.func, ast.Attribute)
                    or generator.elt.func.attr != "set_effect"):
                continue
            clause = generator.generators[0]
            iterator = clause.iter
            if (not isinstance(clause.target, (ast.Tuple, ast.List)) or len(clause.target.elts) != 2
                    or not isinstance(iterator, ast.Call) or not isinstance(iterator.func, ast.Name)
                    or iterator.func.id != "zip" or not self._shader_role_unshadowed("zip")
                    or len(iterator.args) != 2 or iterator.keywords
                    or not all(isinstance(value, ast.Name) for value in iterator.args)):
                continue
            collection = self._shader_value(iterator.args[1])
            if collection and collection[0] in {"programs", "keys"}:
                allowed.add(id(iterator))
        return allowed

    @staticmethod
    def _merge_shader_values(values):
        values = tuple(values)
        if not values or any(value is None for value in values):
            return None
        if all(value[0] in {"program", "programs"} for value in values):
            entries = tuple(entry for value in values for entry in value[1])
            return ("programs", entries)
        if all(value[0] == "keys" for value in values):
            entries = tuple(entry for value in values for entry in value[1])
            return ("keys", entries)
        if all(value == values[0] for value in values):
            return values[0]
        if all(value and value[0] == "sequence" for value in values):
            rows = [value[1] for value in values]
            if not rows or any(len(row) != len(rows[0]) for row in rows):
                return None
            columns = [
                _SceneOnlySource._merge_shader_values(row[index] for row in rows)
                for index in range(len(rows[0]))
            ]
            return ("sequence", tuple(columns)) if all(columns) else None
        return None

    def _assign_shader_value(self, target, value):
        if isinstance(target, ast.Name):
            if self.shader_values:
                self.shader_values[-1][target.id] = value
            return
        if isinstance(target, (ast.Tuple, ast.List)):
            parts = value[1] if value and value[0] == "sequence" else ()
            for index, item in enumerate(target.elts):
                self._assign_shader_value(item, parts[index] if index < len(parts) else None)

    def _loop_shader_value(self, iterable, index):
        if iterable and iterable[0] == "zip":
            value = iterable[1][index] if index < len(iterable[1]) else None
            if value and value[0] == "programs":
                return ("program", value[1])
            if value and value[0] == "keys":
                return ("key", value[1])
            return value
        if iterable and iterable[0] == "programs":
            return ("program", iterable[1])
        if iterable and iterable[0] == "keys":
            return ("key", iterable[1])
        if iterable and iterable[0] == "sequence":
            rows = iterable[1]
            return self._merge_shader_values(
                row[1][index] for row in rows
                if row and row[0] == "sequence" and index < len(row[1])
            )
        return None

    def _shader_parameter_mapping(self, node, programs=None):
        if not isinstance(node, ast.Dict) or any(key is None for key in node.keys):
            raise _OpaqueSource(node, "unproven_shader_key")
        program_ids = {identity for identity, _ in programs[1]} if programs else None
        for key, value in zip(node.keys, node.values):
            origin = self._shader_value(key)
            if (not origin or origin[0] not in {"keys", "key"} or not origin[1]
                    or programs is not None and any(identity not in program_ids
                                                    for identity, _ in origin[1])
                    or not self._shader_numeric_value(value)):
                raise _OpaqueSource(key, "unproven_shader_key")

    def _linear_gradient(self, node):
        if (not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name)
                or node.func.id != "LinearGradient" or not self._shader_role_unshadowed(node.func.id)
                or len(node.args) != 1 or any(isinstance(arg, ast.Starred) for arg in node.args)
                or any(keyword.arg is None for keyword in node.keywords)):
            raise _OpaqueSource(node, "unproven_typed_paint")
        stops = node.args[0]
        if not isinstance(stops, (ast.List, ast.Tuple)) or not stops.elts:
            raise _OpaqueSource(stops, "unproven_typed_paint")
        for stop in stops.elts:
            if (not isinstance(stop, (ast.List, ast.Tuple)) or len(stop.elts) != 2
                    or self._shader_number(stop.elts[0]) is None
                    or not self._shader_color(stop.elts[1])):
                raise _OpaqueSource(stop, "unproven_typed_paint")
        options = {keyword.arg: keyword.value for keyword in node.keywords}
        if len(options) != len(node.keywords) or not set(options).issubset({"start", "end"}):
            raise _OpaqueSource(node, "unproven_typed_paint")
        for option in options.values():
            if (not isinstance(option, (ast.Tuple, ast.List)) or len(option.elts) != 2
                    or any(self._shader_number(value) is None for value in option.elts)):
                raise _OpaqueSource(option, "unproven_typed_paint")
        self.admitted_typed_paint_loads.add(id(node.func))

    def _shader_numeric_value(self, node):
        return self._shader_number(node) is not None or self._shader_value(node) == ("number",)

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
        local_discard_store = (isinstance(node.ctx, ast.Store) and len(self.scopes) > 1
                               and (node.id == "_" or node.id in self.local_discard_names[-1]))
        if node.id.startswith("_") and node.id != "__name__" and not local_discard_store:
            raise _OpaqueSource(node, "private_name")
        if (isinstance(node.ctx, ast.Load) and node.id in _SOURCE_SHADER_CONSTRUCTORS
                and id(node) not in self.admitted_shader_loads):
            raise _OpaqueSource(node, "unproven_shader_reference")
        if (isinstance(node.ctx, ast.Load) and node.id in _SOURCE_TYPED_PAINT_CONSTRUCTORS
                and id(node) not in self.admitted_typed_paint_loads):
            raise _OpaqueSource(node, "unproven_typed_paint")
        if (isinstance(node.ctx, ast.Load) and node.id in self.shader_function_names
                and id(node) not in self.admitted_shader_helper_loads):
            raise _OpaqueSource(node, "unproven_shader_factory")
        if (isinstance(node.ctx, ast.Load) and node.id in self.code_constructors
                and id(node) not in self.admitted_code_loads):
            raise _OpaqueSource(node, "unproven_resource_reference")
        if (isinstance(node.ctx, ast.Load) and node.id == "ImageMobject"
                and id(node) not in self.admitted_image_loads):
            raise _OpaqueSource(node, "unproven_image_reference")
        if (isinstance(node.ctx, ast.Load) and node.id == "ImagePaint"
                and id(node) not in self.admitted_image_paint_loads):
            raise _OpaqueSource(node, "unproven_image_paint_reference")
        if (isinstance(node.ctx, ast.Load) and node.id in _SOURCE_RESOURCE_PREPARATIONS
                and id(node) not in self.admitted_resource_preparation_loads):
            raise _OpaqueSource(node, "unproven_resource_preparation")
        if (isinstance(node.ctx, ast.Load) and node.id in _SOURCE_CONTROLLED_TEXT_COMPILERS
                and id(node) not in self.admitted_controlled_text_loads):
            raise _OpaqueSource(node, "unproven_text_compiler")
        if (isinstance(node.ctx, ast.Load) and node.id in self.pixel_factory_names
                and id(node) not in self.admitted_pixel_factory_loads):
            raise _OpaqueSource(node, "unproven_image_factory_reference")
        if (isinstance(node.ctx, ast.Load) and node.id in self.numpy_array_functions_with_parameters
                and id(node) not in self.admitted_numpy_function_loads):
            raise _OpaqueSource(node, "unproven_function_reference")
        if (isinstance(node.ctx, ast.Load) and node.id in self.numpy_integer_functions_with_parameters
                and id(node) not in self.admitted_integer_function_loads):
            raise _OpaqueSource(node, "unproven_integer_helper_reference")
        if (isinstance(node.ctx, ast.Load) and node.id in self.closed_image_resource_factories
                and id(node) not in self.admitted_image_resource_factory_loads):
            raise _OpaqueSource(node, "unproven_image_factory_reference")
        if (isinstance(node.ctx, ast.Load) and node.id == "LaggedStartMap"
                and id(node) not in self.admitted_lagged_map_loads):
            raise _OpaqueSource(node, "unproven_callback_factory")
        if (isinstance(node.ctx, ast.Load) and node.id in _SOURCE_SPATIAL_CONSTRUCTORS
                and id(node) not in self.admitted_spatial_constructor_loads):
            raise _OpaqueSource(node, "unproven_spatial_constructor")
        if ((self.spatial_method_depth or self.linear_method_depth) and isinstance(node.ctx, ast.Load)
                and node.id in (_SOURCE_SPATIAL_CONSTRUCTORS | _SOURCE_COLOR_CONSTANTS | {"Color"})
                and self._spatial_symbol_shadowed(node.id)):
            raise _OpaqueSource(node, "unproven_shadowed_spatial_constructor")
        if isinstance(node.ctx, ast.Load) and node.id == "getattr" and id(node) in self.admitted_optional_scene_getters:
            return
        if isinstance(node.ctx, ast.Load) and not any(node.id in scope for scope in self.scopes):
            raise _OpaqueSource(node, "unknown_name")

    def visit_Assign(self, node):
        self.visit(node.value)
        value = self._shader_value(node.value)
        carries_collection = self._shader_container_reference(node.value)
        if (carries_collection and (not value or value[0] not in {"programs", "keys"}
                or any(not isinstance(target, ast.Name) for target in node.targets)
                or self.shader_control_depth)):
            raise _OpaqueSource(node, "unproven_shader_collection_escape")
        if self.shader_control_depth:
            value = None
        for target in node.targets:
            self.visit(target)
            self._assign_shader_value(target, value)

    def visit_If(self, node):
        if id(node) in self.unreachable_scene_nodes:
            self.visit(node.test)
            for statement in node.orelse:
                self.visit(statement)
            return
        self.visit(node.test)
        self.shader_control_depth += 1
        try:
            for statement in [*node.body, *node.orelse]:
                self.visit(statement)
        finally:
            self.shader_control_depth -= 1

    def visit_For(self, node):
        self.visit(node.iter)
        self.visit(node.target)
        iterable = self._shader_value(node.iter)
        if isinstance(node.target, (ast.Tuple, ast.List)):
            values = [self._loop_shader_value(iterable, index)
                      for index in range(len(node.target.elts))]
            self._assign_shader_value(node.target, ("sequence", tuple(values)))
        else:
            self._assign_shader_value(node.target, self._loop_shader_value(iterable, 0))
        self.shader_control_depth += 1
        try:
            for statement in [*node.body, *node.orelse]:
                self.visit(statement)
        finally:
            self.shader_control_depth -= 1
        for target in ast.walk(node.target):
            if isinstance(target, ast.Name) and self.shader_values:
                self.shader_values[-1][target.id] = None

    def _closed_resource_function(self, name, active=None, active_data=None):
        """Check a local data factory's call surface without executing Python."""
        function = self.module_functions.get(name)
        if (not isinstance(function, ast.FunctionDef) or self.module_binding_counts.get(name) != 1
                or self.module_binding_locations.get(name) is not function
                or function.decorator_list or function.returns or getattr(function, "type_params", ())):
            return False
        active = set() if active is None else active
        if name in active:
            return False
        active.add(name)
        try:
            args = [*function.args.posonlyargs, *function.args.args, *function.args.kwonlyargs]
            bindings = _SourceBindings()
            for statement in function.body:
                bindings.visit(statement)
            if (any(arg.annotation for arg in args) or function.args.vararg or function.args.kwarg
                    or any(value is not None for value in function.args.kw_defaults)
                    or bindings.names.intersection(_SOURCE_BUILTINS)):
                return False
            local_names = bindings.names | {arg.arg for arg in args}
            local_functions = set()
            for nested in ast.walk(function):
                if isinstance(nested, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    local_functions.add(nested.name)
                    if nested is not function:
                        nested_bindings = _SourceBindings()
                        for statement in nested.body:
                            nested_bindings.visit(statement)
                        local_names.update(nested_bindings.names)
                        local_names.update(arg.arg for arg in
                                           [*nested.args.posonlyargs, *nested.args.args, *nested.args.kwonlyargs])
                if isinstance(nested, ast.comprehension):
                    local_names.update(item.id for item in ast.walk(nested.target)
                                       if isinstance(item, ast.Name))
            parents = {child: parent for parent in ast.walk(function)
                       for child in ast.iter_child_nodes(parent)}
            for loop in (node for node in ast.walk(function) if isinstance(node, ast.While)):
                loop_nodes = set(ast.walk(loop))
                has_own_break = False
                for candidate in (node for node in loop_nodes if isinstance(node, ast.Break)):
                    parent = parents.get(candidate)
                    while parent is not None and not isinstance(parent, (ast.For, ast.AsyncFor, ast.While)):
                        parent = parents.get(parent)
                    if parent is loop:
                        has_own_break = True
                        break
                if isinstance(loop.test, ast.Constant) and loop.test.value is True and not has_own_break:
                    return False
                for candidate in loop_nodes:
                    if (isinstance(candidate, ast.Call) and isinstance(candidate.func, ast.Name)
                            and (candidate.func.id in {"bytearray", "array", "list", "dict", "set"}
                                 or self.module_import_functions.get(candidate.func.id) == ("array", "array")
                                 or candidate.func.id in self.module_functions)
                            or isinstance(candidate, ast.Call) and isinstance(candidate.func, ast.Attribute)
                            and candidate.func.attr in {"append", "add"}):
                        return False
                    if (isinstance(candidate, ast.Assign) and isinstance(candidate.value,
                            (ast.List, ast.Set, ast.Dict, ast.ListComp, ast.SetComp, ast.DictComp))):
                        return False
            owned = set()
            encoded_values = set()
            for node in ast.walk(function):
                if (isinstance(node, ast.Assign) and len(node.targets) == 1
                        and isinstance(node.targets[0], (ast.Tuple, ast.List))
                        and isinstance(node.value, (ast.Tuple, ast.List))
                        and len(node.targets[0].elts) == len(node.value.elts)):
                    for target, value in zip(node.targets[0].elts, node.value.elts):
                        if isinstance(target, ast.Name) and isinstance(
                                value, (ast.List, ast.Set, ast.Dict, ast.ListComp, ast.SetComp, ast.DictComp)):
                            owned.add(target.id)
                if isinstance(node, ast.Assign) and len(node.targets) == 1 \
                        and isinstance(node.targets[0], ast.Name):
                    target, value = node.targets[0], node.value
                    if (isinstance(value, (ast.List, ast.Set, ast.Dict, ast.ListComp, ast.SetComp, ast.DictComp))
                            or isinstance(value, ast.Call) and isinstance(value.func, ast.Name)
                            and (value.func.id == "bytearray" or self.module_import_functions.get(value.func.id)
                                 == ("array", "array"))
                            or isinstance(value, ast.BinOp) and any(
                                isinstance(child, ast.Call) and isinstance(child.func, ast.Name)
                                and self.module_import_functions.get(child.func.id) == ("array", "array")
                                for child in ast.walk(value))):
                        owned.add(target.id)
                    if (isinstance(value, ast.Call) and isinstance(value.func, ast.Attribute)
                            and value.func.attr == "join" and isinstance(value.func.value, ast.Constant)
                            and type(value.func.value.value) is bytes):
                        encoded_values.add(target.id)
            for node in ast.walk(function):
                if isinstance(node, (ast.Global, ast.Nonlocal, ast.Delete, ast.With, ast.AsyncWith,
                                     ast.Try, ast.AsyncFor)):
                    return False
                if isinstance(node, ast.Name) and node.id in {"context", "self", "__name__"}:
                    return False
                if isinstance(node, ast.Call):
                    call = node.func
                    if isinstance(call, ast.Name):
                        callee = call.id
                        if callee in _SOURCE_BUILTINS:
                            if self._resource_builtin_shadowed(node, callee, parents):
                                return False
                            continue
                        if callee in local_functions:
                            continue
                        if callee in local_names:
                            return False
                        origin = self.module_import_functions.get(callee)
                        if origin and origin[0] in _SOURCE_MODULES and origin[1] in _SOURCE_MODULES[origin[0]]:
                            continue
                        if callee in self.module_functions:
                            if not self._closed_resource_function(callee, active, active_data):
                                return False
                            continue
                        return False
                    if isinstance(call, ast.Attribute):
                        if isinstance(call.value, ast.Name) and call.value.id in self.module_modules:
                            if call.value.id in local_names:
                                return False
                            module = self.module_modules[call.value.id]
                            if (module not in _SOURCE_MODULES or call.attr not in _SOURCE_MODULES[module]
                                    or self.module_binding_counts.get(call.value.id) != 1):
                                return False
                            continue
                        if (call.attr in {"append", "add", "join"}
                                and isinstance(call.value, ast.Name) and call.value.id in owned
                                or call.attr == "join" and isinstance(call.value, ast.Constant)
                                and type(call.value.value) is bytes):
                            continue
                    return False
                if isinstance(node, ast.Assign) and len(node.targets) == 1 \
                        and isinstance(node.targets[0], ast.Name):
                    # Mutable buffers and collections cannot escape through aliases.
                    references_owned = any(isinstance(value, ast.Name) and isinstance(value.ctx, ast.Load)
                                            and value.id in owned for value in ast.walk(node.value))
                    if (isinstance(node.value, ast.Name) and node.value.id in owned
                            or node.targets[0].id in owned and references_owned):
                        return False
                    if isinstance(node.value, ast.Name) and node.value.id in encoded_values:
                        return False
                if isinstance(node, ast.Attribute) and isinstance(node.ctx, ast.Load):
                    if isinstance(node.value, ast.Name) and node.value.id in self.module_modules:
                        if node.value.id in local_names:
                            return False
                        module = self.module_modules[node.value.id]
                        if module not in _SOURCE_MODULES or node.attr not in _SOURCE_MODULES[module]:
                            return False
                    elif node.attr not in {"append", "add", "join"}:
                        return False
                if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load):
                    if (node.id not in local_names and node.id not in _SOURCE_BUILTINS
                            and node.id not in self.module_functions and node.id not in self.module_modules
                            and node.id not in self.module_import_functions and node.id not in self.module_bindings):
                        return False
                    if node.id in self.module_bindings and node.id not in self.module_functions \
                            and node.id not in self.module_modules and node.id not in self.module_import_functions:
                        if node.id in {"context", "__name__"}:
                            return False
                        if not self._closed_module_data(node.id, active_data):
                            return False
            for node in ast.walk(function):
                if (isinstance(node, ast.AugAssign) and isinstance(node.target, ast.Name)
                        and node.target.id in encoded_values):
                    return False
                if (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                        and node.func.id in local_functions
                        and any(isinstance(value, ast.Name) and value.id in owned
                                for arg in [*node.args, *(keyword.value for keyword in node.keywords)]
                                for value in ast.walk(arg))):
                    return False
            for nested in (node for node in ast.walk(function)
                           if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda))
                           and node is not function):
                if any(isinstance(node, ast.Name) and node.id in owned | encoded_values
                       for node in ast.walk(nested)):
                    return False
            for resource_name in encoded_values:
                if bindings.binding_counts.get(resource_name) != 1:
                    return False
            return all(
                isinstance(node.value, ast.Name) and node.value.id in owned
                for node in ast.walk(function)
                if isinstance(node, ast.Subscript) and not isinstance(node.ctx, ast.Load)
            )
        finally:
            active.remove(name)

    @staticmethod
    def _resource_builtin_shadowed(call, name, parents):
        child = call
        node = parents.get(child)
        inside_function = False
        while node is not None:
            if isinstance(node, ast.comprehension):
                if any(isinstance(item, ast.Name) and item.id == name for item in ast.walk(node.target)):
                    return True
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
                in_body = child in node.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) else child is node.body
                if in_body:
                    args = [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]
                    if any(arg.arg == name for arg in args):
                        return True
                    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        bindings = _SourceBindings()
                        for statement in node.body:
                            bindings.visit(statement)
                        if name in bindings.names:
                            return True
                    inside_function = True
            elif isinstance(node, ast.ClassDef) and not inside_function:
                bindings = _SourceBindings()
                for statement in node.body:
                    bindings.visit(statement)
                if name in bindings.names:
                    return True
            child, node = node, parents.get(node)
        return False

    def _closed_module_data(self, name, active=None):
        active = set() if active is None else active
        if name in active or self.module_binding_counts.get(name) != 1:
            return False
        active.add(name)
        for statement in self.tree.body:
            value = None
            if (isinstance(statement, ast.Assign) and len(statement.targets) == 1
                    and isinstance(statement.targets[0], ast.Name) and statement.targets[0].id == name):
                value = statement.value
            elif (isinstance(statement, ast.Assign) and len(statement.targets) == 1
                  and isinstance(statement.targets[0], (ast.Tuple, ast.List))
                  and isinstance(statement.value, (ast.Tuple, ast.List))
                  and len(statement.targets[0].elts) == len(statement.value.elts)):
                value = next((item for target, item in zip(statement.targets[0].elts, statement.value.elts)
                              if isinstance(target, ast.Name) and target.id == name), None)
            elif (isinstance(statement, ast.AnnAssign) and isinstance(statement.target, ast.Name)
                  and statement.target.id == name):
                value = statement.value
            if value is None:
                continue
            result = self._closed_data_expression(value, set(), active)
            active.remove(name)
            return result
        active.remove(name)
        return False

    def _closed_data_expression(self, expression, local_names, active=None):
        """Prove a value is assembled from literals and closed deterministic helpers."""
        active = set() if active is None else active
        if isinstance(expression, ast.Name):
            return (expression.id in local_names or expression.id in _SOURCE_BUILTINS
                    or expression.id in self.module_modules and self.module_binding_counts.get(expression.id) == 1
                    or expression.id in self.module_import_functions
                    and self.module_binding_counts.get(expression.id) == 1
                    or expression.id in self.module_functions
                    and self._closed_resource_function(expression.id, active_data=active)
                    or expression.id in self.module_bindings
                    and self._closed_module_data(expression.id, active))
        if isinstance(expression, ast.Constant):
            return type(expression.value) in (int, float, str, bytes, bool, type(None))
        if isinstance(expression, ast.Call):
            if isinstance(expression.func, ast.Name):
                callee = expression.func.id
                trusted = (callee in {"tuple", "sorted", "set", "max", "min", "round", "range"}
                           and callee not in self.module_bindings)
                trusted = trusted or callee in self.module_functions and self._closed_resource_function(
                    callee, active_data=active)
            elif isinstance(expression.func, ast.Attribute) and isinstance(expression.func.value, ast.Name):
                module = expression.func.value.id
                trusted = (module in self.module_modules and self.module_binding_counts.get(module) == 1
                           and self.module_modules[module] in _SOURCE_MODULES
                           and expression.func.attr in _SOURCE_MODULES[self.module_modules[module]])
            else:
                trusted = False
            return (trusted and all(self._closed_data_expression(arg, local_names, active)
                                    for arg in expression.args)
                    and all(self._closed_data_expression(item.value, local_names, active)
                            for item in expression.keywords))
        if isinstance(expression, (ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)):
            names = set(local_names)
            for generator in expression.generators:
                if not self._closed_data_expression(generator.iter, names, active):
                    return False
                names.update(item.id for item in ast.walk(generator.target) if isinstance(item, ast.Name))
                if any(not self._closed_data_expression(test, names, active) for test in generator.ifs):
                    return False
            values = ([expression.key, expression.value] if isinstance(expression, ast.DictComp)
                      else [expression.elt])
            return all(self._closed_data_expression(value, names, active) for value in values)
        if isinstance(expression, ast.Attribute):
            return (isinstance(expression.value, ast.Name) and expression.value.id in self.module_modules
                    and self.module_binding_counts.get(expression.value.id) == 1
                    and self.module_modules[expression.value.id] in _SOURCE_MODULES
                    and expression.attr in _SOURCE_MODULES[self.module_modules[expression.value.id]])
        if isinstance(expression, (ast.Tuple, ast.List, ast.Set)):
            return all(self._closed_data_expression(item, local_names, active) for item in expression.elts)
        if isinstance(expression, ast.Dict):
            return all(self._closed_data_expression(item, local_names, active)
                       for item in [*expression.keys, *expression.values] if item is not None)
        if isinstance(expression, ast.Slice):
            return all(value is None or self._closed_data_expression(value, local_names, active)
                       for value in (expression.lower, expression.upper, expression.step))
        if isinstance(expression, ast.keyword):
            return self._closed_data_expression(expression.value, local_names, active)
        if isinstance(expression, (ast.operator, ast.unaryop, ast.boolop, ast.cmpop, ast.expr_context)):
            return True
        if isinstance(expression, ast.expr):
            return (type(expression).__name__ in {
                "BinOp", "UnaryOp", "Subscript", "Compare", "BoolOp", "IfExp", "Load", "Add", "Sub",
                "Mult", "Div", "FloorDiv", "Mod", "Pow", "UAdd", "USub", "Not", "And", "Or",
                "Eq", "NotEq", "Lt", "LtE", "Gt", "GtE", "In", "NotIn"
            } and all(self._closed_data_expression(child, local_names, active)
                      for child in ast.iter_child_nodes(expression)))
        return False

    def _closed_png_encoder(self, name):
        function = self.module_functions.get(name)
        if not self._closed_resource_function(name):
            return False
        body = function.body
        returns = [node for node in body if isinstance(node, ast.Return)]
        if not returns or not isinstance(returns[-1].value, ast.BinOp):
            return False
        returned = list(ast.walk(returns[-1].value))
        if not any(isinstance(node, ast.Constant) and node.value == b"\x89PNG\r\n\x1a\n"
                   for node in returned):
            return False
        bindings = _SourceBindings()
        for statement in function.body:
            bindings.visit(statement)
        scanlines = {
            node.targets[0].id for node in ast.walk(function)
            if isinstance(node, ast.Assign) and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
            and isinstance(node.value, ast.Call) and isinstance(node.value.func, ast.Attribute)
            and node.value.func.attr == "join" and isinstance(node.value.func.value, ast.Constant)
            and type(node.value.func.value.value) is bytes
        }
        parents = {child: node for node in ast.walk(function) for child in ast.iter_child_nodes(node)}
        for value in scanlines:
            loads = [node for node in ast.walk(function) if isinstance(node, ast.Name)
                     and node.id == value and isinstance(node.ctx, ast.Load)]
            if bindings.binding_counts.get(value) != 1 or len(loads) != 1:
                return False
            parent = parents.get(loads[0])
            while parent is not None and not isinstance(parent, ast.Call):
                parent = parents.get(parent)
            if (not isinstance(parent, ast.Call) or not parent.args or not isinstance(parent.func, ast.Attribute)
                    or parent.func.attr != "compress" or parent.args[0] is not loads[0]):
                return False
        chunk_names = {node.name for node in function.body if isinstance(node, ast.FunctionDef)}
        chunks = [node for node in returned if isinstance(node, ast.Call)
                  and isinstance(node.func, ast.Name) and node.func.id in chunk_names]
        ids = [node for node in chunks if node.args and isinstance(node.args[0], ast.Constant)]
        chunk_function = next((node for node in function.body if isinstance(node, ast.FunctionDef)
                               and chunks and node.name == chunks[0].func.id), None)
        chunk_return = next((node.value for node in chunk_function.body if isinstance(node, ast.Return)
                             and isinstance(node.value, ast.BinOp)), None) if chunk_function else None
        chunk_nodes = list(ast.walk(chunk_return)) if chunk_return else []
        return (
            len(chunks) >= 3
            and any(node.args[0].value == b"IDAT" and len(node.args) == 2
                    and isinstance(node.args[1], ast.Call)
                    and isinstance(node.args[1].func, ast.Attribute)
                    and node.args[1].func.attr == "compress"
                    and isinstance(node.args[1].func.value, ast.Name)
                    and self.module_modules.get(node.args[1].func.value.id) == "zlib" for node in ids)
            and any(node.args[0].value == b"IEND" and len(node.args) == 2
                    and isinstance(node.args[1], ast.Constant) and node.args[1].value == b"" for node in ids)
            and any(isinstance(node.func, ast.Attribute) and node.func.attr == "pack"
                    and isinstance(node.func.value, ast.Name)
                    and self.module_modules.get(node.func.value.id) == "struct" for node in chunk_nodes
                    if isinstance(node, ast.Call))
            and any(isinstance(node.func, ast.Attribute) and node.func.attr == "crc32"
                    and isinstance(node.func.value, ast.Name)
                    and self.module_modules.get(node.func.value.id) == "zlib" for node in chunk_nodes
                    if isinstance(node, ast.Call))
        )

    def _closed_image_resource_factory(self, name):
        function = self.module_functions.get(name)
        if not self._closed_resource_function(name):
            return None
        bindings = _SourceBindings()
        for statement in function.body:
            bindings.visit(statement)
        buffers = set()
        lists = set()
        for statement in ast.walk(function):
            if isinstance(statement, ast.Assign) and len(statement.targets) == 1 \
                    and isinstance(statement.targets[0], ast.Name):
                target, value = statement.targets[0].id, statement.value
                if isinstance(value, ast.List) and not value.elts:
                    lists.add(target)
                if (isinstance(value, ast.Call) and isinstance(value.func, ast.Name)
                        and (value.func.id == "bytearray"
                             or self.module_import_functions.get(value.func.id) == ("array", "array"))):
                    buffers.add(target)

        def is_png_call(node):
            return (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                    and node.func.id in self.closed_png_encoders
                    and node.func.id not in bindings.names
                    and self.module_binding_counts.get(node.func.id) == 1
                    and self.module_binding_locations.get(node.func.id) is self.module_functions[node.func.id]
                    and len(node.args) == 3 and isinstance(node.args[2], ast.Name)
                    and node.args[2].id in buffers
                    and bindings.binding_counts.get(node.args[2].id) == 1 and not node.keywords)

        direct_returns = [node.value for node in function.body if isinstance(node, ast.Return)]
        defaults = function.args.defaults
        if (len(direct_returns) == 1 and is_png_call(direct_returns[0])
                and len(function.args.defaults) == len(function.args.args)
                and not function.args.posonlyargs and not function.args.kwonlyargs
                and len(defaults) == 2
                and all(isinstance(value, ast.Constant) and type(value.value) is int
                        and 0 < value.value <= 1024 for value in defaults)
                and defaults[0].value * defaults[1].value <= 1_048_576):
            return "png"

        for returned in direct_returns:
            if not isinstance(returned, ast.Tuple) or not returned.elts \
                    or not isinstance(returned.elts[0], ast.Name):
                continue
            target = returned.elts[0].id
            if (target not in lists or bindings.binding_counts.get(target) != 1
                    or len(returned.elts) != 2 or not isinstance(returned.elts[1], ast.Tuple)
                    or len(returned.elts[1].elts) != 4
                    or any(not isinstance(value, ast.Name) for value in returned.elts[1].elts)):
                continue
            appends = [node for node in ast.walk(function) if isinstance(node, ast.Call)
                       and isinstance(node.func, ast.Attribute) and node.func.attr == "append"
                       and isinstance(node.func.value, ast.Name) and node.func.value.id == target]
            parents = {child: parent for parent in ast.walk(function)
                       for child in ast.iter_child_nodes(parent)}
            loads = [node for node in ast.walk(function) if isinstance(node, ast.Name)
                     and node.id == target and isinstance(node.ctx, ast.Load)]
            allowed_loads = all(
                isinstance(parents.get(node), ast.Attribute) and parents[node].attr == "append"
                and parents[node].value is node
                or isinstance(parents.get(node), ast.Tuple) and isinstance(parents.get(parents[node]), ast.Return)
                for node in loads
            )
            mutated = any(
                isinstance(node, (ast.Assign, ast.AnnAssign, ast.AugAssign))
                and any(isinstance(child, ast.Subscript) and isinstance(child.value, ast.Name)
                        and child.value.id == target and isinstance(child.ctx, (ast.Store, ast.Del))
                        for child in ast.walk(node))
                for node in ast.walk(function)
            )
            append_loops = []
            append_scopes = []
            append_ancestors = []
            for append in appends:
                ancestors = []
                parent = parents.get(append)
                while parent is not None and parent is not function:
                    ancestors.append(parent)
                    parent = parents.get(parent)
                append_loops.append([node for node in ancestors if isinstance(node, (ast.For, ast.While))])
                append_ancestors.append(ancestors)
                append_scopes.append(any(isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda))
                                         for node in ancestors))
            checkpoint_guard = False
            parameters = {arg.arg for arg in [*function.args.posonlyargs, *function.args.args,
                                               *function.args.kwonlyargs]}
            if len(appends) == 1 and len(append_loops[0]) == 1:
                loop = append_loops[0][0]
                if (isinstance(loop, ast.For) and isinstance(loop.target, (ast.Tuple, ast.List))
                        and len(loop.target.elts) == 2 and isinstance(loop.target.elts[0], ast.Name)
                        and isinstance(loop.iter, ast.Call) and isinstance(loop.iter.func, ast.Name)
                        and loop.iter.func.id == "enumerate" and "enumerate" not in bindings.names | parameters
                        and self.module_binding_counts.get("enumerate", 0) == 0 and len(loop.iter.args) == 2
                        and isinstance(loop.iter.args[1], ast.Constant) and loop.iter.args[1].value == 1
                        and isinstance(loop.iter.args[0], ast.Call)
                        and isinstance(loop.iter.args[0].func, ast.Name)
                        and loop.iter.args[0].func.id == "zip" and "zip" not in bindings.names | parameters
                        and self.module_binding_counts.get("zip", 0) == 0
                        and len(loop.iter.args[0].args) == 2
                        and isinstance(loop.iter.args[0].args[0], ast.Name)
                        and loop.iter.args[0].args[0].id in {arg.arg for arg in function.args.args}
                        and isinstance(loop.iter.args[0].args[1], ast.Subscript)
                        and isinstance(loop.iter.args[0].args[1].value, ast.Name)
                        and loop.iter.args[0].args[1].value.id == loop.iter.args[0].args[0].id
                        and isinstance(loop.iter.args[0].args[1].slice, ast.Slice)
                        and isinstance(loop.iter.args[0].args[1].slice.lower, ast.Constant)
                        and loop.iter.args[0].args[1].slice.lower.value == 1):
                    counter = loop.target.elts[0].id
                    checkpoint_guard = (bindings.binding_counts.get(counter) == 1
                        and "set" not in bindings.names | parameters
                        and self.module_binding_counts.get("set", 0) == 0
                        and any(
                            self._closed_checkpoint_guard_if(
                                function, node, counter, append, parents, parameters, bindings)
                            for node in append_ancestors[0] if isinstance(node, ast.If)
                        )
                    )
            if (len(appends) == 1 and allowed_loads and not mutated and checkpoint_guard
                    and not any(append_scopes)
                    and all(len(node.args) == 1 and not node.keywords and is_png_call(node.args[0])
                            for node in appends)):
                return "tuple_png_list_numeric_tuple"
        return None

    @staticmethod
    def _closed_checkpoint_parameter(function, wanted, parameters, bindings):
        assignments = [node for node in ast.walk(function)
                       if isinstance(node, ast.Assign) and len(node.targets) == 1
                       and isinstance(node.targets[0], ast.Name) and node.targets[0].id == wanted
                       and isinstance(node.value, ast.Call) and isinstance(node.value.func, ast.Name)
                       and node.value.func.id == "set" and len(node.value.args) == 1
                       and isinstance(node.value.args[0], ast.Name)]
        if len(assignments) != 1 or bindings.binding_counts.get(wanted) != 1:
            return None
        name = assignments[0].value.args[0].id
        return name if name in parameters and bindings.binding_counts.get(name, 0) == 0 else None

    def _closed_checkpoint_guard_if(self, function, node, counter, append, parents, parameters, bindings):
        if (not isinstance(node.test, ast.Compare) or not isinstance(node.test.left, ast.Name)
                or node.test.left.id != counter or len(node.test.ops) != 1
                or not isinstance(node.test.ops[0], ast.In) or len(node.test.comparators) != 1
                or not isinstance(node.test.comparators[0], ast.Name)
                or not any(append is descendant for statement in node.body
                           for descendant in ast.walk(statement))):
            return False
        wanted = node.test.comparators[0].id
        parameter = self._closed_checkpoint_parameter(function, wanted, parameters, bindings)
        return parameter is not None and self._closed_checkpoint_references(
            function, parents, parameter, wanted)

    @staticmethod
    def _closed_checkpoint_references(function, parents, parameter, wanted):
        def parent_of(node):
            return parents.get(node)

        for node in ast.walk(function):
            if not isinstance(node, ast.Name) or node.id != parameter:
                continue
            if isinstance(node.ctx, (ast.Store, ast.Del)):
                return False
            parent = parents.get(node)
            if (isinstance(parent, ast.Call) and isinstance(parent.func, ast.Name)
                    and parent.args and parent.args[0] is node):
                outer = parent_of(parent)
                if (parent.func.id == "set"
                        and (isinstance(outer, ast.Assign) and outer.value is parent
                             and any(isinstance(target, ast.Name) and target.id == wanted
                                     for target in outer.targets)
                             or isinstance(outer, ast.Call) and isinstance(outer.func, ast.Name)
                             and outer.func.id == "sorted" and outer.args and outer.args[0] is parent)):
                    continue
                if (parent.func.id == "tuple" and isinstance(outer, ast.Compare)
                        and (outer.left is parent or parent in outer.comparators)):
                    continue
            if (isinstance(parent, ast.UnaryOp) and isinstance(parent.op, ast.Not)
                    and isinstance(parent_of(parent), ast.BoolOp)):
                continue
            if (isinstance(parent, ast.comprehension) and parent.iter is node
                    and isinstance(parent_of(parent), (ast.ListComp, ast.GeneratorExp))):
                continue
            if (isinstance(parent, ast.Subscript) and parent.value is node
                    and isinstance(parent.slice, ast.UnaryOp) and isinstance(parent.slice.op, ast.USub)
                    and isinstance(parent.slice.operand, ast.Constant) and parent.slice.operand.value == 1
                    and isinstance(parent_of(parent), ast.Compare)):
                continue
            return False
        return True

    def _closed_pixel_proof(self, name=None, *, expression=None, initial_values=None, return_type=False):
        """Type-check fresh numeric pixel data without running source or ranges."""
        function = self.module_functions.get(name) if expression is None else None
        if expression is None:
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
        else:
            function = None
            values = dict(initial_values or {})
        transferred = set()
        empty_list = self.empty_pixel_list

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
            if left[0] == "tuple" and len(left[1]) == len(right[1]):
                items = tuple(merge(a, b) for a, b in zip(left[1], right[1]))
                return ("tuple", items) if all(item is not None for item in items) else None
            return left

        def value(node, state):
            if isinstance(node, ast.Constant) and type(node.value) in (int, float):
                return "int" if type(node.value) is int else "float"
            if isinstance(node, ast.Name):
                return state.get(node.id)
            if isinstance(node, ast.Tuple):
                items = tuple(value(item, state) for item in node.elts)
                return ("tuple", items) if all(item is not None for item in items) else None
            if isinstance(node, ast.List):
                items = []
                for element in node.elts:
                    if isinstance(element, ast.Starred):
                        expanded = value(element.value, state)
                        if expanded is None or expanded[0] != "tuple":
                            return None
                        items.extend(expanded[1])
                    else:
                        items.append(value(element, state))
                return (("list", items[0] if items else empty_list)
                        if all(item is not None for item in items)
                        and all(item == items[0] for item in items[1:]) else None)
            if isinstance(node, ast.Subscript):
                container, index = value(node.value, state), value(node.slice, state)
                if index != "int" or container is None:
                    return None
                if container[0] == "list":
                    return container[1]
                if container[0] == "tuple" and container[1] \
                        and all(item == container[1][0] for item in container[1][1:]):
                    return container[1][0]
                return None
            if isinstance(node, ast.ListComp):
                local = state.copy()
                for generator in node.generators:
                    if (generator.is_async or generator.ifs or not isinstance(generator.target, ast.Name)
                            or not isinstance(generator.iter, ast.Call)
                            or not isinstance(generator.iter.func, ast.Name)
                            or generator.iter.func.id != "range" or len(generator.iter.args) != 1
                            or generator.iter.keywords or value(generator.iter.args[0], local) != "int"):
                        return None
                    local[generator.target.id] = "int"
                item = value(node.elt, local)
                return ("list", item) if item is not None else None
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
            if isinstance(node, ast.IfExp) and value(node.test, state) in {"bool", "int", "float"}:
                return merge(value(node.body, state), value(node.orelse, state))
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
                            or item_type[0] == "list" and isinstance(item, ast.Name) and item.id in moved):
                        return None
                    state[owner] = ("list", item_type)
                    if item_type[0] == "list" and isinstance(item, ast.Name):
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

        def numeric_tree(kind, tuples=False):
            if kind is empty_list or kind == "int":
                return True
            if not isinstance(kind, tuple):
                return False
            if kind[0] == "list":
                return numeric_tree(kind[1], tuples)
            if kind[0] == "tuple":
                return tuples and all(numeric_tree(item, tuples) for item in kind[1])
            return False

        if expression is not None:
            result = value(expression, values)
            return result if return_type else numeric_tree(result, tuples=True)

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
        return numeric_tree(result)

    def _image_pixel_source_names(self, body, bindings, parameters):
        if not self.has_image_constructor:
            self.image_numeric_values_by_body[id(body)] = set()
            return set()
        class Uses(ast.NodeVisitor):
            def __init__(self):
                self.calls = []
                self.paint_calls = []
                self.loads = []

            def visit_FunctionDef(self, node):
                return

            visit_AsyncFunctionDef = visit_ClassDef = visit_Lambda = visit_FunctionDef

            def visit_Call(self, node):
                if isinstance(node.func, ast.Name) and node.func.id == "ImageMobject":
                    self.calls.append(node)
                elif isinstance(node.func, ast.Name) and node.func.id == "ImagePaint":
                    self.paint_calls.append(node)
                self.generic_visit(node)

            def visit_Name(self, node):
                if isinstance(node.ctx, ast.Load):
                    self.loads.append(node)

        uses = Uses()
        for statement in body:
            uses.visit(statement)
        if not uses.calls and not uses.paint_calls:
            self.image_numeric_values_by_body[id(body)] = set()
            return set()

        for call in uses.calls:
            if (len(call.args) == 1 and isinstance(call.args[0], ast.Call)
                    and isinstance(call.args[0].func, ast.Name)
                    and not call.args[0].args and not call.args[0].keywords):
                helper = call.args[0].func.id
                if (helper not in bindings.names and helper not in parameters
                        and helper in self.closed_pixel_factory_names):
                    self.pixel_factory_names.add(helper)
                    self.admitted_pixel_factory_loads.add(id(call.args[0].func))

                if (helper not in bindings.names and helper not in parameters
                        and self.closed_image_resource_factories.get(helper) == "png"):
                    self.admitted_image_resource_factory_loads.add(id(call.args[0].func))

        closed_locals = set()
        for statement in body:
            if not isinstance(statement, ast.Assign):
                continue
            value = statement.value
            if (isinstance(value, ast.Call) and isinstance(value.func, ast.Name)
                    and self.closed_image_resource_factories.get(value.func.id)
                    == "tuple_png_list_numeric_tuple"):
                names = {node.id for arg in value.args for node in ast.walk(arg) if isinstance(node, ast.Name)}
                if (not all(self._closed_data_expression(arg, closed_locals) for arg in value.args)
                        or any(name in closed_locals and bindings.binding_counts.get(name) != 1 for name in names)):
                    raise _OpaqueSource(value, "unproven_image_source")
            elif self._closed_data_expression(value, closed_locals):
                for target in statement.targets:
                    closed_locals.update(node.id for node in ast.walk(target) if isinstance(node, ast.Name))
            else:
                for target in statement.targets:
                    closed_locals.difference_update(node.id for node in ast.walk(target) if isinstance(node, ast.Name))

        png_lists = set()
        png_bounds = set()
        for statement in body:
            if (isinstance(statement, ast.Assign) and len(statement.targets) == 1
                    and isinstance(statement.targets[0], (ast.Tuple, ast.List))
                    and isinstance(statement.value, ast.Call) and isinstance(statement.value.func, ast.Name)
                    and len(statement.targets[0].elts) == 2
                    and isinstance(statement.targets[0].elts[0], ast.Name)):
                helper = statement.value.func.id
                target = statement.targets[0].elts[0].id
                if (self.closed_image_resource_factories.get(helper) == "tuple_png_list_numeric_tuple"
                        and helper not in bindings.names and helper not in parameters
                        and bindings.binding_counts.get(target) == 1
                        and isinstance(statement.targets[0].elts[1], ast.Name)
                        and bindings.binding_counts.get(statement.targets[0].elts[1].id) == 1):
                    png_lists.add(target)
                    png_bounds.add(statement.targets[0].elts[1].id)
                    self.admitted_image_resource_factory_loads.add(id(statement.value.func))

        numeric_values = set()
        for statement in body:
            if (isinstance(statement, ast.Assign) and len(statement.targets) == 1
                    and isinstance(statement.targets[0], (ast.Tuple, ast.List))
                    and isinstance(statement.value, ast.Name) and statement.value.id in png_bounds):
                targets = [target.id for target in statement.targets[0].elts
                           if isinstance(target, ast.Name)]
                if len(targets) == len(statement.targets[0].elts) and all(
                        bindings.binding_counts.get(target) == 1 for target in targets):
                    numeric_values.update(targets)

        for target in png_lists | png_bounds:
            loads = [node for node in ast.walk(ast.Module(body=body, type_ignores=[]))
                     if isinstance(node, ast.Name) and node.id == target and isinstance(node.ctx, ast.Load)]
            if target in png_bounds:
                allowed = (len(loads) == 1 and any(
                    isinstance(node, ast.Assign) and isinstance(node.value, ast.Name)
                    and node.value.id == target and isinstance(node.targets[0], (ast.Tuple, ast.List))
                    and loads[0] is node.value for node in body))
            else:
                allowed = bool(loads) and all(any(
                    isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                    and node.func.id in {"enumerate", "len"} and node.func.id not in bindings.names
                    and node.func.id not in parameters and len(node.args) == 1 and not node.keywords
                    and node.args[0] is load for node in ast.walk(ast.Module(body=body, type_ignores=[])))
                    for load in loads)
            if not allowed:
                png_lists.discard(target)
                png_bounds.discard(target)
                numeric_values.clear()
                break

        for comp in ast.walk(ast.Module(body=body, type_ignores=[])):
            if not isinstance(comp, (ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp)):
                continue
            targets = {node.id for generator in comp.generators for node in ast.walk(generator.target)
                       if isinstance(node, ast.Name)}
            for call in uses.calls:
                if (any(node is call for node in ast.walk(comp)) and len(call.args) == 1
                        and isinstance(call.args[0], ast.Name) and call.args[0].id in png_lists
                        and call.args[0].id in targets):
                    raise _OpaqueSource(call, "unproven_image_source")

        requested_sources = {
            call.args[0].id for call in uses.calls
            if len(call.args) == 1 and isinstance(call.args[0], ast.Name)
        }
        sources = set()
        for statement in body:
            if isinstance(statement, ast.For):
                target = statement.target.elts if isinstance(statement.target, (ast.Tuple, ast.List)) else []
                iterator = statement.iter
                if (len(target) == 2 and isinstance(target[1], ast.Name)
                        and isinstance(iterator, ast.Call) and isinstance(iterator.func, ast.Name)
                        and iterator.func.id == "enumerate" and len(iterator.args) == 1
                        and not iterator.keywords and isinstance(iterator.args[0], ast.Name)
                        and iterator.args[0].id in png_lists and "enumerate" not in bindings.names
                        and "enumerate" not in parameters
                        and bindings.binding_counts.get(target[1].id) == 1):
                    name = target[1].id
                    for call in uses.calls:
                        shadowed = any(
                            isinstance(comp, (ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp))
                            and any(isinstance(bound, ast.Name) and bound.id == name
                                    for generator in comp.generators
                                    for bound in ast.walk(generator.target))
                            and any(candidate is call for candidate in ast.walk(comp))
                            for comp in ast.walk(statement)
                        )
                        if (not shadowed
                                and any(candidate is call for nested in statement.body for candidate in ast.walk(nested))
                                and len(call.args) == 1 and isinstance(call.args[0], ast.Name)
                                and call.args[0].id == name):
                            sources.add(name)
                continue
            if (not isinstance(statement, ast.Assign) or len(statement.targets) != 1
                    or not isinstance(statement.targets[0], ast.Name)
                    or statement.targets[0].id not in requested_sources):
                continue
            target = statement.targets[0].id
            if bindings.binding_counts.get(target) != 1 or target in parameters:
                continue
            if (isinstance(statement.value, ast.ListComp)
                    and self._bounded_direct_pixel_array(statement.value, bindings, parameters)
                    and self._closed_pixel_proof(expression=statement.value, return_type=True)
                    == ("list", ("list", ("list", "int")))
                    and all(statement.lineno < call.lineno for call in uses.calls
                            if len(call.args) == 1 and isinstance(call.args[0], ast.Name)
                            and call.args[0].id == target)):
                sources.add(target)
                continue
            if (isinstance(statement.value, ast.Call)
                    and isinstance(statement.value.func, ast.Name)
                    and not statement.value.args and not statement.value.keywords):
                helper = statement.value.func.id
                if (helper not in bindings.names and helper not in parameters
                        and helper in self.closed_pixel_factory_names):
                    sources.add(target)
                    self.pixel_factory_names.add(helper)
                    self.admitted_pixel_factory_loads.add(id(statement.value.func))
                if (helper not in bindings.names and helper not in parameters
                        and self.closed_image_resource_factories.get(helper) == "png"):
                    sources.add(target)
                    self.admitted_image_resource_factory_loads.add(id(statement.value.func))
        accepted = set()
        for call in uses.calls:
            keywords = {keyword.arg for keyword in call.keywords}
            source_arg = call.args[0] if len(call.args) == 1 else None
            source_name = (isinstance(source_arg, ast.Name) and source_arg.id in sources)
            source_factory = (isinstance(source_arg, ast.Call)
                and isinstance(source_arg.func, ast.Name)
                and id(source_arg.func) in self.admitted_pixel_factory_loads
                and not source_arg.args and not source_arg.keywords)
            source_image_factory = (isinstance(source_arg, ast.Call)
                and isinstance(source_arg.func, ast.Name)
                and id(source_arg.func) in self.admitted_image_resource_factory_loads
                and self.closed_image_resource_factories.get(source_arg.func.id) == "png"
                and not source_arg.args and not source_arg.keywords)
            if (len(call.args) != 1 or any(keyword.arg is None for keyword in call.keywords)
                    or not keywords.issubset({"height", "resampling_algorithm", "opacity", "z_index",
                                               "scale_to_resolution"})
                    or (not source_name and not source_factory and not source_image_factory)):
                raise _OpaqueSource(call, "unproven_image_source")
            for keyword in call.keywords:
                option = keyword.value
                if keyword.arg == "resampling_algorithm":
                    allowed = (isinstance(option, ast.Constant)
                        and ((type(option.value) is str and option.value.lower() in
                              {"nearest", "none", "bilinear", "linear", "bicubic", "cubic"})
                             or (type(option.value) is int and option.value in {0, 2, 3})))
                else:
                    allowed = (isinstance(option, ast.Constant) and type(option.value) in (int, float)
                               or keyword.arg == "height" and self._numeric_expression(
                                   option, numeric_values))
                if not allowed:
                    raise _OpaqueSource(option, "unproven_image_option")
            if source_name:
                accepted.add(id(source_arg))

        paint_sources = set()
        paint_palette_loads = {}
        for call in uses.paint_calls:
            if (len(call.args) != 1 or call.keywords or not isinstance(call.args[0], ast.Name)):
                raise _OpaqueSource(call, "unproven_image_paint_source")
            target = call.args[0].id
            data_statement = next((statement for statement in body
                if isinstance(statement, ast.Assign) and len(statement.targets) == 1
                and isinstance(statement.targets[0], ast.Name) and statement.targets[0].id == target
                and isinstance(statement.value, ast.ListComp)), None)
            if data_statement is None or bindings.binding_counts.get(target) != 1:
                raise _OpaqueSource(call, "unproven_image_paint_source")
            if "range" in bindings.names or "range" in parameters:
                raise _OpaqueSource(data_statement.value, "unproven_image_paint_source")
            expression = data_statement.value
            target_names = {generator.target.id for generator in ast.walk(expression)
                if isinstance(generator, ast.comprehension)
                and isinstance(generator.target, ast.Name)}
            dependencies = {node.id for node in ast.walk(expression)
                if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load)}
            dependencies.difference_update(target_names | {"range"})
            if len(dependencies) != 1:
                raise _OpaqueSource(expression, "unproven_image_paint_source")
            palette_name = next(iter(dependencies))
            palette_statement = next((statement for statement in body
                if isinstance(statement, ast.Assign) and len(statement.targets) == 1
                and isinstance(statement.targets[0], ast.Name)
                and statement.targets[0].id == palette_name), None)
            if (palette_statement is None or body.index(palette_statement) >= body.index(data_statement)
                    or bindings.binding_counts.get(palette_name) != 1 or palette_name in parameters
                    or not isinstance(palette_statement.value, ast.List)
                    or any(not isinstance(row, ast.Tuple) or len(row.elts) != 3
                           or any(not isinstance(channel, ast.Constant) or type(channel.value) is not int
                                  for channel in row.elts)
                           for row in palette_statement.value.elts)):
                raise _OpaqueSource(expression, "unproven_image_palette")
            palette_type = self._closed_pixel_proof(
                expression=palette_statement.value, return_type=True
            )
            if (palette_type is None or palette_type[0] != "list"
                    or not isinstance(palette_type[1], tuple) or palette_type[1][0] != "tuple"
                    or len(palette_type[1][1]) != 3
                    or any(item != "int" for item in palette_type[1][1])):
                raise _OpaqueSource(palette_statement.value, "unproven_image_palette")
            pixel_type = self._closed_pixel_proof(
                expression=expression, initial_values={palette_name: palette_type}, return_type=True
            )
            if pixel_type != ("list", ("list", ("list", "int"))):
                raise _OpaqueSource(expression, "unproven_image_paint_source")
            paint_palette_loads.setdefault(palette_name, set()).update(id(node) for node in ast.walk(expression)
                if isinstance(node, ast.Name) and node.id == palette_name
                and isinstance(node.ctx, ast.Load))
            paint_sources.add(target)
            self.admitted_image_paint_loads.add(id(call.func))
            accepted.add(id(call.args[0]))

        for name in sources:
            if any(node.id == name and id(node) not in accepted for node in uses.loads):
                raise _OpaqueSource(next(node for node in uses.loads if node.id == name),
                                    "unproven_image_source")
        for name in paint_sources:
            if any(node.id == name and id(node) not in accepted for node in uses.loads):
                raise _OpaqueSource(next(node for node in uses.loads if node.id == name),
                                    "unproven_image_paint_source")
        for name, allowed_loads in paint_palette_loads.items():
            if any(node.id == name and id(node) not in allowed_loads for node in uses.loads):
                raise _OpaqueSource(next(node for node in uses.loads if node.id == name),
                                    "unproven_image_palette")
        self.image_numeric_values_by_body[id(body)] = numeric_values
        return sources | paint_sources

    def _bounded_direct_pixel_array(self, expression, bindings, parameters):
        if {"range", "abs"}.intersection(bindings.names | set(parameters)):
            return False
        ranges = [node for node in ast.walk(expression)
                  if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                  and node.func.id == "range"]
        if not ranges:
            return False
        pixel_count = 1
        for call in ranges:
            if (len(call.args) != 1 or call.keywords or not isinstance(call.args[0], ast.Constant)
                    or type(call.args[0].value) is not int
                    or not 0 < call.args[0].value <= 256):
                return False
            pixel_count *= call.args[0].value
            if pixel_count > 65_536:
                return False
        return True

    def visit_Call(self, node):
        if isinstance(node.func, ast.Name) and node.func.id in {"bytearray", "array"}:
            name = node.func.id
            if any(name in scope for scope in self.scopes[1:]):
                raise _OpaqueSource(node.func, "unproven_shadowed_builtin")
            if name == "array" and self.module_import_functions.get(name) != ("array", "array"):
                raise _OpaqueSource(node.func, "unproven_array_constructor")
        if isinstance(node.func, ast.Attribute):
            self._reject_shader_collection_mutation(node, node.func.value)
        if isinstance(node.func, ast.Name) and node.func.id == "zip" \
                and not self._shader_role_unshadowed("zip"):
            raise _OpaqueSource(node.func, "unproven_shadowed_builtin")
        play_zip_iterators = self._play_shader_zip_iterators(node)
        if (not (isinstance(node.func, ast.Name) and node.func.id == "zip")
                and any(self._shader_container_reference(argument, skip_calls=play_zip_iterators)
                        for argument in [*node.args, *(keyword.value for keyword in node.keywords)])):
            raise _OpaqueSource(node, "unproven_shader_collection_escape")
        if isinstance(node.func, ast.Name) and node.func.id == "LinearGradient":
            self._linear_gradient(node)
        elif isinstance(node.func, ast.Name) and node.func.id == "WgslEffectProgram":
            schema = self.shader_program_call_specs.get(id(node))
            if schema is None:
                schema = self._shader_program_spec(node)
            self.admitted_shader_loads.add(id(node.func))
            self.shader_call_values[id(node)] = ("program", ((id(node), schema),))
        elif isinstance(node.func, ast.Name) and node.func.id in self.shader_helper_specs:
            if (node.func.id in self.module_bindings
                    and self.module_binding_counts.get(node.func.id) != 1
                    or any(node.func.id in scope for scope in self.scopes[1:])
                    or len(node.args) != 1 or node.keywords
                    or not isinstance(node.args[0], ast.Constant)
                    or type(node.args[0].value) is not str
                    or node.args[0].value not in {"local", "scene", "pixels"}):
                raise _OpaqueSource(node, "unproven_shader_factory")
            self.admitted_shader_helper_loads.add(id(node.func))
            self.shader_call_values[id(node)] = (
                "program", ((id(node), self.shader_helper_specs[node.func.id]),)
            )
        elif isinstance(node.func, ast.Attribute) and node.func.attr == "parameter":
            programs = self._shader_value(node.func.value)
            if (not programs or programs[0] not in {"program", "programs"} or len(node.args) != 1
                    or node.keywords or not isinstance(node.args[0], ast.Constant)
                    or type(node.args[0].value) is not str
                    or any(node.args[0].value not in schema for _, schema in programs[1])):
                raise _OpaqueSource(node, "unproven_shader_key")
            self.admitted_shader_attributes.add(id(node.func))
            self.shader_call_values[id(node)] = (
                "key", tuple((identity, node.args[0].value) for identity, _ in programs[1])
            )
        elif isinstance(node.func, ast.Name) and node.func.id == "ShaderEffect":
            if (not self._shader_role_unshadowed(node.func.id)
                    or len(node.args) != 1 or any(isinstance(arg, ast.Starred) for arg in node.args)
                    or any(keyword.arg is None or keyword.arg != "parameters" for keyword in node.keywords)
                    or len(node.keywords) > 1):
                raise _OpaqueSource(node, "unproven_shader_effect")
            programs = self._shader_value(node.args[0])
            if not programs or programs[0] != "program":
                raise _OpaqueSource(node.args[0], "unproven_shader_effect")
            for keyword in node.keywords:
                self._shader_parameter_mapping(keyword.value, programs)
            self.admitted_shader_loads.add(id(node.func))
        elif (isinstance(node.func, ast.Attribute) and node.func.attr == "set_effect"
              and any(keyword.arg == "parameters" for keyword in node.keywords)):
            if any(keyword.arg is None for keyword in node.keywords):
                raise _OpaqueSource(node, "unproven_shader_key")
            mapping = next(keyword.value for keyword in node.keywords if keyword.arg == "parameters")
            self._shader_parameter_mapping(mapping)
        if isinstance(node.func, ast.Attribute) and node.func.attr == "plot":
            # The canonical axes API samples this scalar callable once into a
            # retained Rust path; it never installs a per-frame host callback.
            if (not node.args or any(isinstance(arg, ast.Starred) for arg in node.args)
                    or any(keyword.arg is None for keyword in node.keywords)
                    or not self._scalar_plot_lambda(node.args[0])):
                raise _OpaqueSource(node, "unproven_plot_function")
            self.admitted_plot_loads.add(id(node.func))
        if isinstance(node.func, ast.Name):
            name = node.func.id
            if name in {"int", "float", "complex", "abs", "round", "min", "max", "len", "range", "enumerate"} and (
                    name in self.module_bindings or any(name in scope for scope in self.scopes[1:])):
                raise _OpaqueSource(node.func, "unproven_shadowed_numeric_builtin")
            if name in {"VMobject", "SVGMobject"} and (
                    name in self.module_bindings or any(name in scope for scope in self.scopes[1:])):
                raise _OpaqueSource(node.func, "unproven_shadowed_svg_constructor")
            safe_direct = (name in (_SOURCE_BUILTINS | _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS
                                   | _SOURCE_SHADER_CONSTRUCTORS | _SOURCE_TYPED_PAINT_CONSTRUCTORS
                                   | _SOURCE_SPATIAL_CONSTRUCTORS | _SOURCE_CONTROLLED_TEXT_COMPILERS
                                   | _SOURCE_RESOURCE_PREPARATIONS)
                           | self.source_callable_imports | self.code_constructors
                           | self.module_callable_aliases
                           or name in self.numpy_module_functions
                           and name not in self.local_scope_names[-1]
                           or any(name in local_functions for local_functions in self.local_function_names))
            bound = any(name in scope for scope in self.scopes)
            if bound and not safe_direct and name not in self.safe_callable_aliases[-1]:
                raise _OpaqueSource(node, "unproven_callable")
        numpy_path = _attribute_path(node.func)
        if numpy_path and (numpy_path.startswith("np.") or numpy_path in _NUMPY_CALLS):
            if numpy_path not in _NUMPY_CALLS:
                raise _OpaqueSource(node, "unsupported_numpy_operation")
            if self.numpy_import is None or self.module_binding_counts.get("np") != 1:
                raise _OpaqueSource(node, "unproven_numpy_binding")
            if (any(isinstance(argument, ast.Starred) for argument in node.args)
                    or any(keyword.arg is None for keyword in node.keywords)
                    or len({keyword.arg for keyword in node.keywords}) != len(node.keywords)):
                raise _OpaqueSource(node, "unproven_numpy_arguments")
            allowed_keywords = {
                "np.array": {"dtype"}, "np.asarray": {"dtype"},
                "np.linspace": {"endpoint"}, "np.concatenate": {"axis"},
                "np.cumsum": {"axis"}, "np.zeros": {"dtype"},
            }.get(numpy_path, set())
            if any(keyword.arg not in allowed_keywords for keyword in node.keywords):
                raise _OpaqueSource(node, "unproven_numpy_arguments")
            if numpy_path == "np.asarray":
                if (not node.args or not (_numpy_array_expression(
                        node.args[0], self.numpy_array_names[-1], self.numpy_array_functions)
                        or _static_numpy_literal(node.args[0])
                        or isinstance(node.args[0], ast.Name)
                        and node.args[0].id in self.module_numpy_literals)):
                    raise _OpaqueSource(node, "unproven_numpy_argument_origin")
            arrays = self.numpy_array_names[-1]
            containers = self.numpy_safe_containers[-1]
            scalars = self.numpy_scalar_names[-1]
            for index, argument in enumerate(node.args):
                if numpy_path == "np.concatenate":
                    valid = index == 0 and _numpy_array_collection(
                        argument, arrays, self.module_numpy_literals, self.numpy_array_functions)
                elif numpy_path in {"np.cumsum", "np.diff", "np.exp", "np.interp", "np.outer", "np.fft.fft"}:
                    valid = _numpy_array_expression(argument, arrays, self.numpy_array_functions)
                elif numpy_path in {"np.array", "np.linspace", "np.zeros"}:
                    valid = _numpy_numeric_expression(argument, arrays, containers,
                                                      self.module_numpy_literals, scalars)
                elif numpy_path == "np.asarray":
                    valid = index > 0 or (_numpy_array_expression(argument, arrays,
                        self.numpy_array_functions) or _static_numpy_literal(argument)
                        or isinstance(argument, ast.Name) and argument.id in self.module_numpy_literals)
                else:
                    valid = (_numpy_array_expression(argument, arrays, self.numpy_array_functions)
                             or _numpy_numeric_expression(argument, arrays, containers,
                                                         self.module_numpy_literals, scalars))
                if not valid:
                    raise _OpaqueSource(argument, "unproven_numpy_argument_origin")
            for keyword in node.keywords:
                if keyword.arg == "dtype" and not (
                    isinstance(keyword.value, ast.Name) and keyword.value.id in {"float", "complex"}
                ):
                    raise _OpaqueSource(keyword.value, "unproven_numpy_dtype")
                if keyword.arg == "endpoint" and not (
                    isinstance(keyword.value, ast.Constant) and type(keyword.value.value) is bool
                ):
                    raise _OpaqueSource(keyword.value, "unproven_numpy_arguments")
            self.admitted_numpy_attributes.add(id(node.func))
            cursor = node.func
            while isinstance(cursor, ast.Attribute):
                self.admitted_numpy_attributes.add(id(cursor))
                cursor = cursor.value
        elif isinstance(node.func, ast.Attribute) and node.func.attr in _NUMPY_ARRAY_METHODS:
            array_receiver = _numpy_array_expression(node.func.value, self.numpy_array_names[-1],
                                                     self.numpy_array_functions)
            if node.func.attr == "reshape" or array_receiver:
                if (not array_receiver or node.func.attr == "reshape" and not node.args
                        or node.keywords or self.numpy_import is None):
                    raise _OpaqueSource(node, "unproven_numpy_method")
                self.admitted_numpy_attributes.add(id(node.func))
        if (isinstance(node.func, ast.Name) and node.func.id in self.numpy_module_functions
                and node.func.id in self.numpy_array_parameters):
            if (node.func.id in self.local_scope_names[-1]
                    and node.func.id not in self.local_function_names[-1]):
                raise _OpaqueSource(node, "unproven_shadowed_function")
            self.admitted_numpy_function_loads.add(id(node.func))
            function = self.numpy_functions[node.func.id]
            formals = [arg.arg for arg in [*function.args.posonlyargs, *function.args.args,
                                            *function.args.kwonlyargs]]
            actuals = dict(zip(formals, node.args))
            actuals.update({keyword.arg: keyword.value for keyword in node.keywords
                            if keyword.arg in formals})
            arrays = self.numpy_array_names[-1]
            for formal in formals:
                if formal not in self.numpy_array_parameters[node.func.id]:
                    continue
                if (formal not in actuals or not _numpy_array_expression(
                        actuals[formal], arrays, self.numpy_array_functions)):
                    raise _OpaqueSource(node, "unproven_numpy_argument_origin")
            for formal in self.numpy_scalar_parameters[node.func.id]:
                if (formal not in actuals or not _numpy_numeric_expression(
                        actuals[formal], arrays, self.numpy_safe_containers[-1],
                        self.module_numpy_literals, self.numpy_scalar_names[-1])):
                    raise _OpaqueSource(node, "unproven_numpy_argument_origin")
        if (isinstance(node.func, ast.Name)
                and node.func.id in self.numpy_integer_functions_with_parameters):
            if (node.func.id in self.local_scope_names[-1]
                    and node.func.id not in self.local_function_names[-1]):
                raise _OpaqueSource(node, "unproven_shadowed_function")
            function = self.numpy_functions[node.func.id]
            formals = [arg.arg for arg in [*function.args.posonlyargs, *function.args.args,
                                            *function.args.kwonlyargs]]
            actuals = dict(zip(formals, node.args))
            actuals.update({keyword.arg: keyword.value for keyword in node.keywords
                            if keyword.arg in formals})
            for formal in self.numpy_integer_parameters[node.func.id]:
                if (formal not in actuals
                        or not _integer_expression(actuals[formal], self.numpy_integer_names[-1],
                                                   self.module_numpy_literals)):
                    raise _OpaqueSource(node, "unproven_integer_argument_origin")
            self.admitted_integer_function_loads.add(id(node.func))
        if isinstance(node.func, ast.Attribute) and node.func.attr == "audio":
            if id(node.func) not in self.admitted_scene_audio_calls:
                raise _OpaqueSource(node, "unproven_audio_declaration")
        if isinstance(node.func, ast.Name) and node.func.id == "ImageMobject":
            if "ImageMobject" in self.module_bindings or any(
                "ImageMobject" in scope for scope in self.scopes[1:]
            ):
                raise _OpaqueSource(node.func, "unproven_shadowed_image_constructor")
            pixel_source = len(node.args) == 1 and isinstance(node.args[0], ast.Name) \
                and node.args[0].id in self.image_pixel_sources[-1]
            pixel_factory = (len(node.args) == 1 and isinstance(node.args[0], ast.Call)
                and isinstance(node.args[0].func, ast.Name)
                and node.args[0].func.id in self.closed_pixel_factory_names
                and id(node.args[0].func) in self.admitted_pixel_factory_loads
                and not any(node.args[0].func.id in scope for scope in self.scopes[1:])
                and not node.args[0].args and not node.args[0].keywords)
            generated_factory = (len(node.args) == 1 and isinstance(node.args[0], ast.Call)
                and isinstance(node.args[0].func, ast.Name)
                and id(node.args[0].func) in self.admitted_image_resource_factory_loads
                and self.closed_image_resource_factories.get(node.args[0].func.id) == "png"
                and not any(node.args[0].func.id in scope for scope in self.scopes[1:])
                and not node.args[0].args and not node.args[0].keywords)
            if not pixel_source and not pixel_factory and not generated_factory:
                raise _OpaqueSource(node, "unproven_image_source")
            self.admitted_image_loads.add(id(node.func))
        if isinstance(node.func, ast.Name) and node.func.id == "ImagePaint":
            if "ImagePaint" in self.module_bindings or any(
                "ImagePaint" in scope for scope in self.scopes[1:]
            ):
                raise _OpaqueSource(node.func, "unproven_shadowed_image_paint_constructor")
            if (len(node.args) != 1 or node.keywords or not isinstance(node.args[0], ast.Name)
                    or node.args[0].id not in self.image_pixel_sources[-1]):
                raise _OpaqueSource(node, "unproven_image_paint_source")
            self.admitted_image_paint_loads.add(id(node.func))
        if isinstance(node.func, ast.Name) and node.func.id == "MathTex":
            name = node.func.id
            shadowed = any(name in scope for scope in self.scopes[1:])
            if name in self.module_bindings:
                trusted_import = (name in self.controlled_text_imports
                    and self.module_binding_counts.get(name) == 1
                    and isinstance(self.module_binding_locations.get(name), ast.ImportFrom))
                shadowed = shadowed or not trusted_import
            state = self.resource_preparation_scopes[-1] if self.resource_preparation_scopes else None
            keywords = {keyword.arg: keyword.value for keyword in node.keywords}
            if (shadowed or state is None or not state["ready"]
                    or len(node.args) != 1 or not isinstance(node.args[0], ast.Constant)
                    or type(node.args[0].value) is not str or not node.args[0].value
                    or len(node.args[0].value.encode("utf-8")) > 1024 * 1024
                    or any(keyword.arg is None for keyword in node.keywords)
                    or len(keywords) != len(node.keywords)
                    or not set(keywords).issubset({"font_size", "color", "opacity"})):
                raise _OpaqueSource(node, "unproven_text_compiler")
            font_size = keywords.get("font_size")
            if font_size is not None and (
                not isinstance(font_size, ast.Constant)
                or type(font_size.value) not in (int, float)
                or font_size.value <= 0
            ):
                raise _OpaqueSource(font_size, "unproven_text_compiler_option")
            opacity = keywords.get("opacity")
            if opacity is not None and (
                not isinstance(opacity, ast.Constant)
                or type(opacity.value) not in (int, float)
                or not 0 <= opacity.value <= 1
            ):
                raise _OpaqueSource(opacity, "unproven_text_compiler_option")
            color = keywords.get("color")
            if color is not None and (
                not isinstance(color, ast.Name) or color.id not in _SOURCE_CONSTANTS
                or color.id in self.module_bindings
                or any(color.id in scope for scope in self.scopes[1:])
            ):
                raise _OpaqueSource(color, "unproven_text_compiler_option")
            self.admitted_controlled_text_loads.add(id(node.func))
        if isinstance(node.func, ast.Name) and node.func.id == "SVGMobject":
            raise _OpaqueSource(node, "unproven_svg_source")
        if (isinstance(node.func, ast.Attribute) and node.func.attr == "from_string"
                and isinstance(node.func.value, ast.Name) and node.func.value.id == "SVGMobject"):
            if ("SVGMobject" in self.module_bindings
                    or any("SVGMobject" in scope for scope in self.scopes[1:])):
                raise _OpaqueSource(node, "unproven_shadowed_svg_constructor")
            self._prepare_svg_from_string(node)
            self.admitted_svg_from_string_attributes.add(id(node.func))
        if isinstance(node.func, ast.Attribute) and node.func.attr == "set_points":
            receiver = node.func.value
            if (not isinstance(receiver, ast.Call) or not isinstance(receiver.func, ast.Name)
                    or receiver.func.id != "VMobject" or receiver.args or receiver.keywords
                    or len(node.args) != 1 or node.keywords):
                raise _OpaqueSource(node, "unproven_svg_path_write")
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
        if self._spatial_constructor_call(node):
            linear_text = (self.linear_method_depth and isinstance(node.func, ast.Name)
                           and node.func.id == "Typst" and self._trusted_noon_binding("Typst"))
            if not self.spatial_method_depth and not linear_text:
                raise _OpaqueSource(node, "unproven_spatial_constructor")
            if not self._valid_spatial_constructor_call(node):
                raise _OpaqueSource(node, "unproven_spatial_constructor")
            if isinstance(node.func, ast.Name):
                self.admitted_spatial_constructor_loads.add(id(node.func))
            else:
                self.admitted_spatial_constructor_loads.add(id(node.func.value))
        self.generic_visit(node)

    def visit_Await(self, node):
        call = node.value
        if (isinstance(call, ast.Call) and isinstance(call.func, ast.Name)
                and call.func.id in _SOURCE_RESOURCE_PREPARATIONS):
            name = call.func.id
            state = self.resource_preparation_scopes[-1] if self.resource_preparation_scopes else None
            shadowed = any(name in scope for scope in self.scopes[1:])
            if name in self.module_bindings:
                trusted_import = (name in self.resource_preparation_imports
                    and self.module_binding_counts.get(name) == 1
                    and isinstance(self.module_binding_locations.get(name), ast.ImportFrom))
                shadowed = shadowed or not trusted_import
            if (state is None or id(node) not in state["awaits"] or call.args or call.keywords
                    or shadowed or not any(name in scope for scope in self.scopes)):
                raise _OpaqueSource(call, "unproven_resource_preparation")
            self.admitted_resource_preparation_loads.add(id(call.func))
        self.generic_visit(node)

    def _spatial_symbol_shadowed(self, name):
        if any(name in scope for scope in self.scopes[1:]):
            return True
        if name not in self.module_bindings:
            return False
        origin = self.module_binding_locations.get(name)
        return not (origin in self.module_imports and isinstance(origin, ast.ImportFrom)
                    and origin.module == "noon" and self.module_binding_counts.get(name) == 1
                    and any(alias.name == name and alias.asname is None for alias in origin.names))

    @staticmethod
    def _spatial_constructor_call(node):
        return (isinstance(node.func, ast.Name) and node.func.id in _SOURCE_SPATIAL_CONSTRUCTORS
                or isinstance(node.func, ast.Attribute) and isinstance(node.func.value, ast.Name)
                and node.func.value.id == "Mesh3D" and node.func.attr in {"sphere", "parametric"})

    @classmethod
    def _valid_spatial_constructor_call(cls, node):
        if any(isinstance(arg, ast.Starred) for arg in node.args) or any(k.arg is None for k in node.keywords):
            return False
        if isinstance(node.func, ast.Name):
            values = {keyword.arg: keyword.value for keyword in node.keywords}
            if len(values) != len(node.keywords):
                return False
            if node.func.id == "ThreeDAxes":
                return not node.args and not node.keywords
            if node.func.id == "Typst":
                return (len(node.args) == 1 and isinstance(node.args[0], ast.Constant)
                        and isinstance(node.args[0].value, str) and not node.keywords)
            if node.func.id == "Dot3D":
                point = values.get("point")
                return (not node.args and set(values) <= {"point", "radius", "color", "stroke_width", "shade_in_3d"}
                        and isinstance(point, ast.Call) and isinstance(point.func, ast.Attribute)
                        and point.func.attr == "c2p" and isinstance(point.func.value, ast.Name)
                        and len(point.args) == 3 and not point.keywords
                        and all(cls._finite_number(arg) for arg in point.args)
                        and "radius" in values and cls._finite_number(values["radius"])
                        and "color" in values and cls._color_literal(values["color"])
                        and ("stroke_width" not in values or cls._finite_number(values["stroke_width"]))
                        and ("shade_in_3d" not in values or isinstance(values["shade_in_3d"], ast.Constant)
                             and type(values["shade_in_3d"].value) is bool))
            if node.func.id == "WorldTransformTo":
                return (len(node.args) == 1 and isinstance(node.args[0], ast.Name)
                        and set(values) <= {"translation", "rotation", "scale"}
                        and all(cls._finite_tuple(value, 4 if name == "rotation" else 3)
                                for name, value in values.items()))
            return False
        values = {keyword.arg: keyword.value for keyword in node.keywords}
        if len(values) != len(node.keywords):
            return False
        if node.func.attr == "sphere":
            return (not node.args and set(values) <= {"radius", "resolution", "color"}
                    and ("radius" not in values or cls._finite_number(values["radius"]))
                    and ("resolution" not in values or cls._finite_tuple(values["resolution"], 2, integers=True))
                    and ("color" not in values or cls._color_literal(values["color"])))
        if node.func.attr == "parametric":
            function = node.args[0] if node.args else None
            expected = {"u_range", "v_range", "resolution", "color", "point_lit"}
            if (len(node.args) != 1 or not isinstance(function, ast.Lambda) or len(function.args.args) != 2
                    or function.args.posonlyargs or function.args.kwonlyargs or function.args.defaults
                    or function.args.vararg or function.args.kwarg or set(values) != expected
                    or not cls._finite_tuple(values["u_range"], 2)
                    or not cls._finite_tuple(values["v_range"], 2)
                    or not cls._finite_tuple(values["resolution"], 2, integers=True)
                    or not cls._color_literal(values["color"])
                    or not isinstance(values["point_lit"], ast.Constant)
                    or type(values["point_lit"].value) is not bool):
                return False
            parameters = {argument.arg for argument in function.args.args}
            return (isinstance(function.body, ast.Tuple) and len(function.body.elts) == 3
                    and all(cls._numeric_expression(item, parameters) for item in function.body.elts))
        return False

    @staticmethod
    def _finite_number(node, *, integers=False):
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
            return _SceneOnlySource._finite_number(node.operand, integers=integers)
        return (isinstance(node, ast.Constant) and type(node.value) in ((int,) if integers else (int, float))
                and (type(node.value) is not float or math.isfinite(node.value)))

    @classmethod
    def _finite_tuple(cls, node, length, *, integers=False):
        return (isinstance(node, ast.Tuple) and len(node.elts) == length
                and all(cls._finite_number(value, integers=integers) for value in node.elts))

    @classmethod
    def _color_literal(cls, node):
        return (isinstance(node, ast.Name) and node.id in _SOURCE_COLOR_CONSTANTS
                or isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id == "Color" and not node.keywords and len(node.args) in {3, 4}
                and all(cls._finite_number(value) for value in node.args))

    @classmethod
    def _numeric_expression(cls, node, names):
        if cls._finite_number(node) or isinstance(node, ast.Name) and node.id in names:
            return True
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
            return cls._numeric_expression(node.operand, names)
        return (isinstance(node, ast.BinOp) and isinstance(node.op, (ast.Add, ast.Sub, ast.Mult, ast.Div, ast.Pow))
                and cls._numeric_expression(node.left, names) and cls._numeric_expression(node.right, names))
    def _prepare_svg_from_string(self, node):
        if (len(node.args) != 1 or isinstance(node.args[0], ast.Starred)
                or not self._is_module_svg_text(node.args[0])
                or any(keyword.arg is None for keyword in node.keywords)):
            raise _OpaqueSource(node, "unproven_svg_source")
        options = {}
        for keyword in node.keywords:
            if keyword.arg not in {"height", "width", "should_center"} or keyword.arg in options:
                raise _OpaqueSource(keyword, "unproven_svg_option")
            value = keyword.value
            if keyword.arg == "should_center":
                valid = isinstance(value, ast.Constant) and type(value.value) is bool
            else:
                valid = (isinstance(value, ast.Constant) and type(value.value) in (int, float)
                         and _finite_source_number(value.value) and value.value > 0)
            if not valid:
                raise _OpaqueSource(value, "unproven_svg_option")
            options[keyword.arg] = value.value

    def _is_module_svg_text(self, node):
        if isinstance(node, ast.Constant) and type(node.value) is str:
            value = node.value
        elif (isinstance(node, ast.Name) and node.id in self.module_string_literals
                and not any(node.id in scope for scope in self.scopes[1:])):
            value = self.module_string_literals[node.id]
        else:
            return False
        return len(value.encode("utf-8")) <= 1_048_576

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
        if self.admitted_regex_attributes and id(node) in self.admitted_regex_attributes[-1]:
            self.visit(node.value)
            return
        if id(node) in self.admitted_scene_helper_attributes:
            self.visit(node.value)
            return
        if id(node) in self.admitted_scene_owned_attributes:
            self.visit(node.value)
            return
        if (self.admitted_local_group_attributes
                and id(node) in self.admitted_local_group_attributes[-1]):
            self.visit(node.value)
            return
        if id(node) in self.admitted_shader_attributes:
            self.visit(node.value)
            return
        if node.attr == "plot" and id(node) not in self.admitted_plot_loads:
            raise _OpaqueSource(node, "unproven_plot_function")
        numpy_path = _attribute_path(node)
        if numpy_path and numpy_path.startswith("np.") and numpy_path != "np.pi" \
                and id(node) not in self.admitted_numpy_attributes:
            raise _OpaqueSource(node, "unproven_numpy_reference")
        if numpy_path == "np.pi" and isinstance(node.ctx, ast.Load):
            if "np" in self.module_bindings and self.module_binding_counts.get("np") == 1:
                self.visit(node.value)
                return
        if id(node) in getattr(self, "admitted_numpy_attributes", ()):
            self.visit(node.value)
            return
        if node.attr in _NUMPY_ARRAY_ATTRIBUTES and isinstance(node.ctx, ast.Load):
            if _numpy_array_expression(node.value, self.numpy_array_names[-1], self.numpy_array_functions):
                self.visit(node.value)
                return
            raise _OpaqueSource(node, "unproven_numpy_value")
        if not isinstance(node.ctx, ast.Load) or node.attr not in self.members:
            raise _OpaqueSource(node, "unsupported_member" if isinstance(node.ctx, ast.Load) else "unproven_member_write")
        if node.attr == "audio" and id(node) not in self.admitted_scene_audio_calls:
            raise _OpaqueSource(node, "unproven_audio_declaration")
        if node.attr == "small_discovery" and id(node) not in self.admitted_audio_factory_attributes:
            raise _OpaqueSource(node, "unproven_audio_source")
        if node.attr == "from_string" and id(node) not in self.admitted_svg_from_string_attributes:
            raise _OpaqueSource(node, "unproven_svg_source")
        self.visit(node.value)

    def _scalar_plot_lambda(self, node):
        if (not isinstance(node, ast.Lambda) or node.args.posonlyargs
                or len(node.args.args) != 1 or node.args.defaults or node.args.kwonlyargs
                or node.args.vararg or node.args.kwarg):
            return False
        parameter = node.args.args[0].arg

        def scalar(value):
            if isinstance(value, ast.Constant):
                return type(value.value) in (int, float)
            if isinstance(value, ast.Name):
                return value.id == parameter
            if isinstance(value, ast.UnaryOp) and isinstance(value.op, (ast.UAdd, ast.USub)):
                return scalar(value.operand)
            if isinstance(value, ast.BinOp) and isinstance(value.op,
                    (ast.Add, ast.Sub, ast.Mult, ast.Div, ast.FloorDiv, ast.Mod, ast.Pow)):
                return scalar(value.left) and scalar(value.right)
            if isinstance(value, ast.Call) and isinstance(value.func, ast.Name):
                name = value.func.id
                binding = self.module_binding_locations.get(name)
                return (name != parameter and not value.keywords
                    and self.module_binding_counts.get(name) == 1
                    and not any(name in scope for scope in self.scopes[1:])
                    and isinstance(binding, ast.ImportFrom) and binding.module == "math"
                    and any(alias.name == name and alias.asname is None for alias in binding.names)
                    and all(scalar(arg) for arg in value.args))
            return False

        return scalar(node.body)

    def _canonical_numpy_import(self, node):
        return (isinstance(node, ast.Import) and len(node.names) == 1
                and node.names[0].name == "numpy" and node.names[0].asname == "np"
                and self.module_binding_counts.get("np") == 1
                and self.module_binding_locations.get("np") is node)

    def visit_Import(self, node):
        if node not in self.module_imports or len(self.scopes) != 1:
            raise _OpaqueSource(node, "unsupported_import")
        if any(alias.name == "numpy" for alias in node.names):
            if not self._canonical_numpy_import(node):
                raise _OpaqueSource(node, "unsupported_import")
            self.numpy_import = node
        elif any(alias.name not in _SOURCE_MODULES for alias in node.names):
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
        if node.module in {"noon", "manim"} and any(
            alias.name == "ImagePaint" and alias.asname is not None for alias in node.names
        ):
            raise _OpaqueSource(node, "unproven_image_paint_constructor")
        if node.module in {"noon", "manim"} and any(
            alias.name in (_SOURCE_RESOURCE_PREPARATIONS | _SOURCE_CONTROLLED_TEXT_COMPILERS)
            and alias.asname is not None for alias in node.names
        ):
            raise _OpaqueSource(node, "unproven_resource_preparation")
        if node.module in {"noon", "manim"} and any(
            alias.name in _SOURCE_SHADER_CONSTRUCTORS and alias.asname is not None
            for alias in node.names
        ):
            raise _OpaqueSource(node, "unproven_shader_reference")
        if node.module in {"noon", "manim"} and any(
            alias.name in _SOURCE_TYPED_PAINT_CONSTRUCTORS and alias.asname is not None
            for alias in node.names
        ):
            raise _OpaqueSource(node, "unproven_typed_paint")
        noon_names = (_SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS | _SOURCE_CONSTANTS
                      | _SOURCE_RESOURCE_PREPARATIONS | _SOURCE_CONTROLLED_TEXT_COMPILERS
                      | _SOURCE_SHADER_CONSTRUCTORS | _SOURCE_TYPED_PAINT_CONSTRUCTORS)
        if node.module in {"noon", "manim"} and any(
            alias.name in _SOURCE_SPATIAL_CONSTRUCTORS and alias.asname is not None for alias in node.names
        ):
            raise _OpaqueSource(node, "unproven_spatial_constructor")
        allowed = (noon_names | _SOURCE_SPATIAL_CONSTRUCTORS if node.module == "noon"
                   else noon_names if node.module == "manim" else _SOURCE_MODULES.get(node.module, ()))
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

    def _scope(self, body, parameters=(), array_parameters=(), scalar_parameters=(), integer_parameters=()):
        parameters = set(parameters)
        bindings = _SourceBindings()
        for node in body:
            bindings.visit(node)
        if "np" in bindings.names or "np" in parameters:
            raise _OpaqueSource(bindings.locations.get("np"), "unproven_numpy_shadow")
        integer_names = self._guarded_integer_parameters(body, parameters, bindings)
        if integer_names:
            # Only a leading, terminating builtin guard proves parameter type.
            # Re-scan origins with that fact; no source statement is executed.
            bindings = _SourceBindings(integer_names)
            for node in body:
                bindings.visit(node)
        integer_names = _integer_names_in_body(
            body, set(integer_parameters) | integer_names, self.module_numpy_literals)
        for nested in (node for statement in body for node in ast.walk(statement)
                       if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))):
            integer_names.update(name for name in self.numpy_integer_parameters.get(nested.name, ())
                                 if name not in bindings.names and name not in self.module_bindings)
        self.numpy_integer_names.append(integer_names)
        local_group_attributes = self._closed_local_group_attributes(body, bindings, parameters)
        regex_attributes = self._closed_regex_attributes(body, bindings, parameters)
        image_pixel_sources = self._image_pixel_source_names(body, bindings, parameters)
        self.scopes.append(bindings.names | parameters)
        self.shader_values.append({})
        self.image_pixel_sources.append(image_pixel_sources)
        self.image_pixel_numeric_values.append(self.image_numeric_values_by_body.get(id(body), set()))
        # Every lexical binding must have a fresh origin; a parameter, alias,
        # conditional rebind or loop target cannot inherit another value's proof.
        self.owned_containers.append(bindings.container_names - bindings.other_names - parameters)
        self.admitted_local_group_attributes.append(local_group_attributes)
        self.admitted_regex_attributes.append(regex_attributes)
        preparation_awaits = {
            id(node.value)
            for node in body
            if isinstance(node, ast.Expr) and isinstance(node.value, ast.Await)
            and isinstance(node.value.value, ast.Call)
            and isinstance(node.value.value.func, ast.Name)
            and node.value.value.func.id in _SOURCE_RESOURCE_PREPARATIONS
        }
        preparation_scope = {"awaits": preparation_awaits, "ready": False}
        self.resource_preparation_scopes.append(preparation_scope)
        self.numpy_safe_containers.append(_safe_numpy_containers(body))
        self.local_scope_names.append(bindings.names | parameters)
        self.local_function_names.append({node.name for node in body
                                          if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))})
        safe_names = set(_SOURCE_BUILTINS | _SOURCE_CONSTRUCTORS | _SOURCE_FUNCTIONS)
        safe_names.update(self.numpy_module_functions)
        safe_names.update(self.source_callable_imports)
        safe_names.update(self.code_constructors)
        safe_names.update(self.local_function_names[-1])
        aliases = set()
        changed = True
        assignments = [node for node in _walk_scope(body) if isinstance(node, ast.Assign)]
        while changed:
            changed = False
            for assignment in assignments:
                if (len(assignment.targets) == 1 and isinstance(assignment.targets[0], ast.Name)
                        and isinstance(assignment.value, ast.Name)
                        and bindings.binding_counts.get(assignment.targets[0].id) == 1
                        and assignment.targets[0].id not in aliases
                        and assignment.value.id in safe_names | aliases):
                    aliases.add(assignment.targets[0].id)
                    changed = True
        self.safe_callable_aliases.append(aliases)
        array_names = _array_names_in_body(body, array_parameters, self.numpy_array_functions,
                                           self.numpy_function_outputs)
        integer_names.update(_integer_index_container_names(
            body, array_names, integer_names, self.module_numpy_literals))
        if _nested_scope_references(body, array_names, self.numpy_safe_containers[-1],
                                    integer_names, self.module_numpy_literals, self.numpy_integer_parameters):
            raise _OpaqueSource(None, "unproven_numpy_closure")
        self.numpy_array_names.append(array_names)
        self.owned_containers[-1].update(array_names - set(array_parameters))
        scalar_names = _numpy_scalar_names_in_body(
            body, scalar_parameters, array_names, self.numpy_safe_containers[-1], self.module_numpy_literals)
        self.safe_numpy_writes.append(_fresh_numpy_writes(
            body, array_names, self.numpy_array_functions, self.numpy_safe_containers[-1],
            self.module_numpy_literals, scalar_names))
        self.numpy_scalar_names.append(_numpy_scalar_names_in_body(
            body, scalar_parameters, array_names, self.numpy_safe_containers[-1], self.module_numpy_literals))
        discards = set()
        for statement in _walk_scope(body):
            targets = statement.targets if isinstance(statement, ast.Assign) else (
                [statement.target] if isinstance(statement, (ast.For, ast.AsyncFor)) else [])
            for target in targets:
                if isinstance(target, (ast.Tuple, ast.List)):
                    discards.update(item.id for item in ast.walk(target)
                                    if isinstance(item, ast.Name) and item.id.startswith("_"))
        self.local_discard_names.append(discards)
        try:
            for node in body:
                self.visit(node)
                if isinstance(node, ast.Expr) and isinstance(node.value, ast.Await) \
                        and id(node.value) in preparation_awaits:
                    preparation_scope["ready"] = True
        finally:
            self.resource_preparation_scopes.pop()
            self.admitted_local_group_attributes.pop()
            self.admitted_regex_attributes.pop()
            self.image_pixel_sources.pop()
            self.image_pixel_numeric_values.pop()
            self.shader_values.pop()
            self.numpy_array_names.pop()
            self.numpy_integer_names.pop()
            self.safe_numpy_writes.pop()
            self.numpy_scalar_names.pop()
            self.numpy_safe_containers.pop()
            self.local_discard_names.pop()
            self.local_scope_names.pop()
            self.local_function_names.pop()
            self.safe_callable_aliases.pop()
            self.scopes.pop()
            self.owned_containers.pop()

    def _integer_record_list_schemas(self):
        """Summarize closed helpers that return a mapping containing integer rows."""
        parents = {child: parent for parent in ast.walk(self.tree) for child in ast.iter_child_nodes(parent)}
        schemas = {}
        for name, function in self.module_functions.items():
            if (self.module_binding_counts.get(name) != 1 or not isinstance(function, ast.FunctionDef)
                    or self.module_binding_counts.get("dict", 0)
                    or function.decorator_list or function.args.posonlyargs or function.args.kwonlyargs
                    or function.args.vararg or function.args.kwarg
                    or any(value is not None and (not isinstance(value, ast.Constant)
                           or type(value.value) is not int) for value in function.args.defaults)):
                continue
            bindings = _SourceBindings()
            for statement in function.body:
                bindings.visit(statement)
            if "dict" in bindings.names | {arg.arg for arg in function.args.args}:
                continue
            integer_names = _integer_names_in_body(
                function.body, {arg.arg for arg in function.args.args}, self.module_integer_literals)
            dict_names = set()
            for statement in _walk_scope(function.body):
                if not isinstance(statement, ast.Assign) or len(statement.targets) != 1:
                    continue
                targets, values = statement.targets[0], statement.value
                pairs = zip(targets.elts, values.elts) if (
                    isinstance(targets, (ast.Tuple, ast.List)) and isinstance(values, (ast.Tuple, ast.List))
                    and len(targets.elts) == len(values.elts)) else [(targets, values)]
                for target, value in pairs:
                    if (isinstance(target, ast.Name) and isinstance(value, ast.Dict)
                            and not value.keys and bindings.binding_counts.get(target.id) == 1):
                        dict_names.add(target.id)
            empty_lists = set()
            for statement in _walk_scope(function.body):
                if not isinstance(statement, ast.Assign) or len(statement.targets) != 1:
                    continue
                target, value = statement.targets[0], statement.value
                pairs = zip(target.elts, value.elts) if (
                    isinstance(target, (ast.Tuple, ast.List)) and isinstance(value, (ast.Tuple, ast.List))
                    and len(target.elts) == len(value.elts)) else [(target, value)]
                empty_lists.update(item.id for item, origin in pairs
                                   if isinstance(item, ast.Name) and isinstance(origin, ast.List)
                                   and not origin.elts and bindings.binding_counts.get(item.id) == 1)

            safe_dicts = set()
            for dict_name in dict_names:
                writes = [node for node in ast.walk(function) if isinstance(node, ast.Subscript)
                          and isinstance(node.ctx, ast.Store) and isinstance(node.value, ast.Name)
                          and node.value.id == dict_name]
                loads = [node for node in ast.walk(function) if isinstance(node, ast.Name)
                         and node.id == dict_name and isinstance(node.ctx, ast.Load)]
                if (writes and all(_integer_expression(node.slice, integer_names, self.module_integer_literals)
                                   for node in writes)
                        and all(isinstance(parents.get(node), ast.Assign)
                                and _integer_expression(parents[node].value, integer_names,
                                                        self.module_integer_literals)
                                for node in writes)
                        and all(isinstance(parents.get(node), ast.Subscript)
                                and parents[node].value is node for node in loads)
                        and not any(isinstance(child, ast.Call) and isinstance(child.func, ast.Attribute)
                                    and isinstance(child.func.value, ast.Name)
                                    and child.func.value.id == dict_name for child in ast.walk(function))):
                    safe_dicts.add(dict_name)

            def integer_value(expr):
                if _integer_expression(expr, integer_names, self.module_integer_literals):
                    return True
                return (isinstance(expr, ast.Subscript) and isinstance(expr.value, ast.Name)
                        and expr.value.id in safe_dicts
                        and _integer_expression(expr.slice, integer_names, self.module_integer_literals))

            returned = [statement.value for statement in function.body if isinstance(statement, ast.Return)]
            if (len(returned) != 1 or not isinstance(returned[0], ast.Call)
                    or not isinstance(returned[0].func, ast.Name) or returned[0].func.id != "dict"
                    or returned[0].args or any(keyword.arg is None for keyword in returned[0].keywords)):
                continue
            fields = {}
            for keyword in returned[0].keywords:
                if isinstance(keyword.value, ast.Name):
                    candidate = keyword.value.id
                    if (bindings.binding_counts.get(candidate) == 1
                            and candidate in empty_lists):
                        appends = [node for node in ast.walk(function) if isinstance(node, ast.Call)
                                   and isinstance(node.func, ast.Attribute) and node.func.attr == "append"
                                   and isinstance(node.func.value, ast.Name) and node.func.value.id == candidate]
                        loops = [ancestor for ancestor in self._ancestors(appends[0], parents)
                                 if isinstance(ancestor, (ast.For, ast.While))] if appends else []
                        bounded = bool(appends) and all(
                            isinstance(loop, ast.For) and isinstance(loop.iter, ast.Call)
                            and isinstance(loop.iter.func, ast.Name) and loop.iter.func.id == "range"
                            and len(loop.iter.args) == 1 and isinstance(loop.iter.args[0], ast.Constant)
                            and type(loop.iter.args[0].value) is int
                            and 0 < loop.iter.args[0].value <= 4096 and not loop.iter.keywords
                            or isinstance(loop, ast.For) and isinstance(loop.iter, (ast.Tuple, ast.List))
                            for loop in loops)
                        total = 1
                        for loop in loops:
                            if isinstance(loop.iter, ast.Call):
                                total *= loop.iter.args[0].value
                            else:
                                total *= len(loop.iter.elts)
                        if (len(appends) == 1 and bounded and total <= 65_536
                                and len(appends[0].args) == 1
                                and isinstance(appends[0].args[0], (ast.List, ast.Tuple))):
                            row = appends[0].args[0]
                            if all(integer_value(item) for item in row.elts):
                                other_uses = [item for item in ast.walk(function)
                                              if isinstance(item, ast.Name) and item.id == candidate
                                              and isinstance(item.ctx, ast.Load)
                                              and not (isinstance(parents.get(item), ast.Attribute)
                                                       and parents[item].attr == "append"
                                                       or item is keyword.value)]
                                mutated = any(
                                    isinstance(item, ast.Subscript) and isinstance(item.ctx, (ast.Store, ast.Del))
                                    and isinstance(item.value, ast.Name) and item.value.id == candidate
                                    or isinstance(item, ast.Call) and isinstance(item.func, ast.Attribute)
                                    and isinstance(item.func.value, ast.Name) and item.func.value.id == candidate
                                    and item.func.attr != "append"
                                    for item in ast.walk(function))
                                if not other_uses and not mutated:
                                    fields[keyword.arg] = ("integer_rows", len(row.elts))
                                    continue
                fields[keyword.arg] = "integer" if integer_value(keyword.value) else "opaque"
            if not any(isinstance(value, tuple) and value[0] == "integer_rows" for value in fields.values()):
                continue
            call_sites = []
            aliases = False
            for node in ast.walk(self.tree):
                if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == name:
                    caller = parents.get(node)
                    while caller is not None and not isinstance(caller, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        caller = parents.get(caller)
                    if caller is None or len(node.args) < len(function.args.args) - len(function.args.defaults) \
                            or len(node.args) > len(function.args.args) or node.keywords:
                        aliases = True
                        break
                    names = _integer_names_at_node(caller.body, node, set(), self.module_integer_literals)
                    if not all(_integer_expression(arg, names, self.module_integer_literals) for arg in node.args):
                        aliases = True
                        break
                    call_sites.append(node)
                elif isinstance(node, ast.Name) and node.id == name and isinstance(node.ctx, ast.Load):
                    parent = parents.get(node)
                    if not (isinstance(parent, ast.Call) and parent.func is node):
                        aliases = True
            if call_sites and not aliases:
                schemas[name] = fields
        return schemas

    def _integer_row_parameter_types(self):
        """Propagate closed record rows through local scene data and helper calls."""
        row_fields = {name: {key: value for key, value in schema.items()
                             if isinstance(value, tuple) and value[0] == "integer_rows"}
                      for name, schema in self.integer_record_schemas.items()}
        row_fields = {name: fields for name, fields in row_fields.items() if fields}
        methods_by_class = {}
        trusted_scene = any(isinstance(imported, ast.ImportFrom) and imported.module == "noon"
                            and any(alias.name == "*" or alias.name == "Scene" and alias.asname is None
                                    for alias in imported.names)
                            for imported in self.tree.body)
        for cls in (node for node in self.tree.body if isinstance(node, ast.ClassDef)
                    and trusted_scene and len(node.bases) == 1
                    and isinstance(node.bases[0], ast.Name) and node.bases[0].id == "Scene"):
            methods_by_class[cls.name] = {method.name: method for method in cls.body
                                         if isinstance(method, (ast.FunctionDef, ast.AsyncFunctionDef))}
        typed_formals = {}
        call_types = {}
        helper_call_counts = {}
        active_binding_counts = {}
        active_parameters = set()
        for methods in methods_by_class.values():
            for method in methods.values():
                for call in (child for child in ast.walk(method) if isinstance(child, ast.Call)
                             and isinstance(child.func, ast.Attribute)
                             and isinstance(child.func.value, ast.Name)
                             and child.func.value.id == "self"):
                    helper_call_counts[call.func.attr] = helper_call_counts.get(call.func.attr, 0) + 1

        def value(expr, env):
            if isinstance(expr, ast.Name):
                return env.get(expr.id)
            if isinstance(expr, ast.Call) and isinstance(expr.func, ast.Name):
                return ("record", expr.func.id) if expr.func.id in row_fields else None
            if isinstance(expr, ast.ListComp):
                kind = value(expr.elt, env)
                return ("record_list", kind[1]) if isinstance(kind, tuple) and kind[0] == "record" else None
            if isinstance(expr, ast.Subscript) and isinstance(expr.value, ast.Name):
                record = env.get(expr.value.id)
                key = expr.slice.value if isinstance(expr.slice, ast.Constant) else None
                if isinstance(record, tuple) and record[0] == "record_list":
                    return ("record", record[1])
                if isinstance(record, tuple) and record[0] == "record" and key is not None:
                    schema = self.integer_record_schemas[record[1]]
                    field = schema.get(key)
                    return ("integer_rows", field[1]) if isinstance(field, tuple) and field[0] == "integer_rows" else (
                        "integer" if field == "integer" else None)
            if isinstance(expr, (ast.Tuple, ast.List)):
                return ("tuple", tuple(value(item, env) for item in expr.elts))
            return None

        def bind(target, kind, env):
            if isinstance(target, ast.Name):
                # A later unknown write invalidates an earlier type fact.
                env[target.id] = kind
                return
            if isinstance(target, (ast.Tuple, ast.List)) and isinstance(kind, tuple) and kind[0] == "tuple":
                for item, item_kind in zip(target.elts, kind[1]):
                    bind(item, item_kind, env)

        def process_class(methods):
            def block(statements, env):
                for statement in statements:
                    if isinstance(statement, ast.Assign):
                        kind = value(statement.value, env)
                        for target in statement.targets:
                            bind(target, kind, env)
                        for call in (node for node in _walk_scope([statement.value]) if isinstance(node, ast.Call)):
                            record_call(call, env)
                    elif isinstance(statement, (ast.For, ast.AsyncFor)):
                        iterable = value(statement.iter, env)
                        target_kind = ("record", iterable[1]) if (
                            isinstance(iterable, tuple) and iterable[0] == "record_list") else None
                        if (isinstance(statement.iter, ast.Tuple)
                                and isinstance(statement.target, (ast.Tuple, ast.List))
                                and len(statement.target.elts) > 0):
                            rows = [value(item, env) for item in statement.iter.elts]
                            if rows and all(isinstance(row, tuple) and row[0] == "tuple"
                                            and len(row[1]) == len(statement.target.elts) for row in rows):
                                items = tuple(row[1][index] for index in range(len(statement.target.elts))
                                              for row in rows)
                                kinds = []
                                for index in range(len(statement.target.elts)):
                                    candidates = [row[1][index] for row in rows]
                                    kinds.append(candidates[0] if all(item == candidates[0]
                                                                      for item in candidates) else None)
                                target_kind = ("tuple", tuple(kinds))
                        child = env.copy()
                        bind(statement.target, target_kind, child)
                        for call in _walk_scope([statement.iter]):
                            if isinstance(call, ast.Call):
                                record_call(call, env)
                        block(statement.body, child)
                        else_env = child.copy()
                        block(statement.orelse, else_env)
                        before_loop = env.copy()
                        merge_env(env, child, else_env)
                        merge_env(env, env.copy(), before_loop)
                    elif isinstance(statement, ast.If):
                        for call in _walk_scope([statement.test]):
                            if isinstance(call, ast.Call):
                                record_call(call, env)
                        body_env, else_env = env.copy(), env.copy()
                        block(statement.body, body_env)
                        block(statement.orelse, else_env)
                        merge_env(env, body_env, else_env)
                    else:
                        for call in (node for node in _walk_scope([statement]) if isinstance(node, ast.Call)):
                            record_call(call, env)

            def record_call(call, env):
                nonlocal changed
                if (isinstance(call.func, ast.Attribute) and isinstance(call.func.value, ast.Name)
                        and call.func.value.id == "self" and call.func.attr in methods
                        and helper_call_counts.get(call.func.attr) == 1):
                    target = methods[call.func.attr]
                    formals = [arg.arg for arg in [*target.args.posonlyargs, *target.args.args,
                                                    *target.args.kwonlyargs]][1:]
                    for formal, actual in zip(formals, call.args):
                        kind = value(actual, env)
                        if kind is not None and not stable_actual(actual):
                            kind = None
                        if isinstance(kind, tuple) and kind[0] in {"record", "record_list", "integer_rows"}:
                            facts = typed_formals.setdefault(call.func.attr, {})
                            if facts.get(formal) != kind:
                                facts[formal] = kind
                                changed = True
                elif isinstance(call.func, ast.Name) and call.func.id in self.module_functions:
                    formals = [arg.arg for arg in [*self.module_functions[call.func.id].args.posonlyargs,
                                                   *self.module_functions[call.func.id].args.args,
                                                   *self.module_functions[call.func.id].args.kwonlyargs]]
                    kinds = call_types.setdefault(call.func.id, {})
                    kinds[id(call)] = tuple(
                        kind if stable_actual(actual) else None
                        for actual in call.args
                        for kind in (value(actual, env),))

            def stable_actual(actual):
                for name in (node.id for node in ast.walk(actual) if isinstance(node, ast.Name)):
                    if (name in active_parameters and active_binding_counts.get(name, 0) != 0
                            or name in active_binding_counts and active_binding_counts[name] != 1):
                        return False
                return True

            def merge_env(target, left, right):
                target.clear()
                target.update({name: kind for name, kind in left.items()
                               if kind is not None and right.get(name) == kind})

            changed = True
            while changed:
                changed = False
                for method_name, method in methods.items():
                    nonlocal active_binding_counts, active_parameters
                    bindings = _SourceBindings()
                    for statement in method.body:
                        bindings.visit(statement)
                    active_binding_counts = bindings.binding_counts
                    active_parameters = {argument.arg for argument in
                                         [*method.args.posonlyargs, *method.args.args,
                                          *method.args.kwonlyargs]}
                    env = dict(typed_formals.get(method_name, {}))
                    block(method.body, env)

        for methods in methods_by_class.values():
            process_class(methods)
        row_parameters, row_widths = {}, {}
        all_calls = {name: [node for node in ast.walk(self.tree) if isinstance(node, ast.Call)
                            and isinstance(node.func, ast.Name) and node.func.id == name]
                     for name in self.module_functions}
        for name, call_map in call_types.items():
            calls = list(call_map.values())
            sites = all_calls.get(name, [])
            if len(sites) != len(calls) or not sites:
                continue
            formals = [arg.arg for arg in [*self.module_functions[name].args.posonlyargs,
                                           *self.module_functions[name].args.args,
                                           *self.module_functions[name].args.kwonlyargs]]
            for index, formal in enumerate(formals):
                actual_kinds = [site[index] if index < len(site) else None for site in calls]
                if actual_kinds and all(isinstance(kind, tuple) and kind[0] == "integer_rows"
                                        for kind in actual_kinds):
                    row_parameters[name] = row_parameters.get(name, set()) | {formal}
                    row_widths[name, formal] = actual_kinds[0][1]
        return row_parameters, row_widths

    @staticmethod
    def _ancestors(node, parents):
        parent = parents.get(node)
        while parent is not None:
            yield parent
            parent = parents.get(parent)

    def _closed_local_group_attributes(self, body, bindings, parameters):
        """Prove mutations stay inside fresh, bounded local dictionary groups."""
        nodes = [node for statement in body for node in _walk_scope([statement])]
        owners = {}
        for node in nodes:
            if (not isinstance(node, ast.Assign) or len(node.targets) != 1
                    or not isinstance(node.targets[0], ast.Name)
                    or not isinstance(node.value, ast.ListComp)):
                continue
            comp = node.value
            if (not isinstance(comp.elt, ast.Dict) or comp.elt.keys or comp.elt.values
                    or len(comp.generators) != 1):
                continue
            gen = comp.generators[0]
            if (not isinstance(gen.target, ast.Name) or not isinstance(gen.iter, ast.Call)
                    or not isinstance(gen.iter.func, ast.Name) or gen.iter.func.id != "range"
                    or len(gen.iter.args) != 1 or gen.iter.keywords
                    or not isinstance(gen.iter.args[0], ast.Constant)
                    or type(gen.iter.args[0].value) is not int
                    or not 0 < gen.iter.args[0].value <= 1024 or gen.ifs
                    or "range" in bindings.names | set(parameters)
                    or self.module_binding_counts.get("range", 0)
                    or bindings.binding_counts.get(node.targets[0].id) != 1):
                continue
            owners[node.targets[0].id] = gen.iter.args[0].value
        function_name = self.function_context[-1] if self.function_context else None
        typed_rows = self.integer_row_parameters.get(function_name, set()) & set(parameters)
        if any(bindings.binding_counts.get(name, 0) for name in typed_rows):
            return set()
        row_targets = {}
        for loop in nodes:
            if (isinstance(loop, ast.For) and isinstance(loop.iter, ast.Name)
                    and loop.iter.id in typed_rows and isinstance(loop.target, (ast.Tuple, ast.List))
                    and len(loop.target.elts) == self.integer_row_widths.get((function_name, loop.iter.id))):
                row_targets[id(loop)] = {target.id for target in loop.target.elts
                                         if isinstance(target, ast.Name) and target.id != "_"}
        if any(bindings.binding_counts.get(name) != 1
               for names in row_targets.values() for name in names):
            return set()
        parents = {child: parent for parent in nodes for child in ast.iter_child_nodes(parent)}
        admitted = set()
        for node in nodes:
            if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Attribute):
                continue
            attr = node.func
            if attr.attr == "setdefault":
                receiver = attr.value
                if (isinstance(receiver, ast.Subscript) and isinstance(receiver.value, ast.Name)
                        and receiver.value.id in owners and receiver.value.id in bindings.container_names
                        and isinstance(receiver.slice, ast.BinOp) and isinstance(receiver.slice.op, ast.Mod)
                        and isinstance(receiver.slice.left, ast.Name)
                        and isinstance(receiver.slice.right, ast.Constant)
                        and type(receiver.slice.right.value) is int
                        and receiver.slice.right.value == owners[receiver.value.id]
                        and len(node.args) == 2 and isinstance(node.args[0], ast.Name)
                        and isinstance(node.args[1], ast.List) and not node.args[1].elts
                        and not node.keywords
                        and any(receiver.slice.left.id in names and node.args[0].id in names
                                for loop_id, names in row_targets.items()
                                if any(parent is not None and id(parent) == loop_id
                                       for parent in self._ancestors(node, parents)))):
                    admitted.add(id(attr))
            elif attr.attr == "values":
                if (isinstance(attr.value, ast.Name) and not node.args and not node.keywords
                        and "list" not in bindings.names | set(parameters)
                        and self.module_binding_counts.get("list", 0) == 0
                        and any(isinstance(parent, ast.ListComp)
                                and any(isinstance(gen.target, ast.Name)
                                        and gen.target.id == attr.value.id
                                        and isinstance(gen.iter, ast.Name)
                                        and gen.iter.id in owners
                                        for gen in parent.generators)
                                for parent in nodes)):
                    admitted.add(id(attr))
        if any(isinstance(node, ast.Attribute) and node.attr in {"setdefault", "values"}
               and id(node) not in admitted for node in nodes):
            return set()
        for owner in owners:
            if any(isinstance(node, ast.Subscript) and isinstance(node.value, ast.Name)
                   and node.value.id == owner and isinstance(node.ctx, (ast.Store, ast.Del))
                   for node in nodes):
                return set()
            for node in nodes:
                if not (isinstance(node, ast.Name) and node.id == owner
                        and isinstance(node.ctx, ast.Load)):
                    continue
                parent = parents.get(node)
                if (isinstance(parent, ast.Subscript) and parent.value is node
                        or isinstance(parent, ast.comprehension) and parent.iter is node):
                    continue
                return set()
        return admitted

    def _closed_regex_attributes(self, body, bindings, parameters):
        if (self.module_binding_counts.get("re") != 1
                or not isinstance(self.module_binding_locations.get("re"), ast.Import)
                or "re" in bindings.names | parameters):
            return set()
        nodes, pending = [], list(body)
        while pending:
            node = pending.pop()
            nodes.append(node)
            if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Lambda)):
                pending.extend(ast.iter_child_nodes(node))
        parents = {child: parent for parent in nodes for child in ast.iter_child_nodes(parent)
                   if child in nodes}
        literal_strings = {
            statement.targets[0].id
            for statement in nodes
            if isinstance(statement, ast.Assign) and len(statement.targets) == 1
            and isinstance(statement.targets[0], ast.Name)
            and isinstance(statement.value, ast.Constant) and type(statement.value.value) is str
            and bindings.binding_counts.get(statement.targets[0].id) == 1
        }
        match_names = set()
        for node in nodes:
            if (isinstance(node, ast.For) and isinstance(node.target, ast.Name)
                    and isinstance(node.iter, ast.Call)
                    and isinstance(node.iter.func, ast.Attribute) and node.iter.func.attr == "finditer"
                    and isinstance(node.iter.func.value, ast.Name) and node.iter.func.value.id == "re"
                    and len(node.iter.args) == 2 and not node.iter.keywords
                    and not any(isinstance(arg, ast.Starred) for arg in node.iter.args)
                    and (isinstance(node.iter.args[0], ast.Constant)
                         and type(node.iter.args[0].value) is str
                         or isinstance(node.iter.args[0], ast.Name)
                         and node.iter.args[0].id in literal_strings)
                    and bindings.binding_counts.get(node.target.id) == 1):
                match_names.add(node.target.id)
        tokens = set()
        for node in nodes:
            if (isinstance(node, ast.Assign) and len(node.targets) == 1
                    and isinstance(node.targets[0], ast.Name)
                    and isinstance(node.value, ast.Call)
                    and isinstance(node.value.func, ast.Attribute)
                    and node.value.func.attr == "group"
                    and isinstance(node.value.func.value, ast.Name)
                    and node.value.func.value.id in match_names
                    and not node.value.args and not node.value.keywords
                    and bindings.binding_counts.get(node.targets[0].id) == 1):
                tokens.add(node.targets[0].id)
        admitted = set()
        for node in nodes:
            if not isinstance(node, ast.Attribute) or not isinstance(node.value, ast.Name):
                continue
            receiver = node.value.id
            if node.attr not in {"group", "start", "end", "isdigit"}:
                continue
            call = parents.get(node)
            if (not isinstance(call, ast.Call) or call.func is not node
                    or call.args or call.keywords):
                continue
            if (node.attr in {"group", "start", "end"} and receiver in match_names
                    or node.attr == "isdigit" and receiver in tokens):
                admitted.add(id(node))
        return admitted

    def visit_FunctionDef(self, node):
        if node.name.startswith("_") or node.decorator_list or node.returns or getattr(node, "type_params", ()):
            raise _OpaqueSource(node, "unproven_function_contract")
        arguments = [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]
        vararg = node.args.vararg
        kwarg = node.args.kwarg
        if (any(argument.annotation for argument in arguments)
                or vararg and not self._closed_forwarded_vararg(node, vararg.arg)
                or kwarg and not self._closed_forwarded_kwarg(node, kwarg.arg)):
            raise _OpaqueSource(node, "unproven_function_contract")
        for value in [*node.args.defaults, *filter(None, node.args.kw_defaults)]:
            self.visit(value)
        self.function_context.append(node.name)
        try:
            self._scope(node.body, [*(argument.arg for argument in arguments),
                                   *((vararg.arg,) if vararg else ()),
                                   *((kwarg.arg,) if kwarg else ())],
                        self.numpy_array_parameters.get(node.name, ()),
                        self.numpy_scalar_parameters.get(node.name, ()),
                        self.numpy_integer_parameters.get(node.name, ()))
        finally:
            self.function_context.pop()

    visit_AsyncFunctionDef = visit_FunctionDef

    def _closed_forwarded_kwarg(self, node, name):
        loads = [item for item in ast.walk(node)
                 if isinstance(item, ast.Name) and item.id == name and isinstance(item.ctx, ast.Load)]
        return bool(loads) and all(id(item) in self.unreachable_scene_nodes for item in loads)

    def _closed_forwarded_vararg(self, node, name):
        if node.decorator_list or node.args.kwarg:
            return False
        loads = [item for item in ast.walk(node)
                 if isinstance(item, ast.Name) and item.id == name and isinstance(item.ctx, ast.Load)]
        calls = [call for call in ast.walk(node)
                 if isinstance(call, ast.Call) and isinstance(call.func, ast.Attribute)
                 and isinstance(call.func.value, ast.Name) and call.func.value.id == "self"
                 and call.func.attr in self.members | self.scene_helper_names
                 and any(isinstance(arg, ast.Starred) and arg.value is load
                         for arg in call.args for load in loads)]
        return (len(loads) == 1 and len(calls) == 1
                and sum(isinstance(arg, ast.Starred) and arg.value is loads[0]
                        for arg in calls[0].args) == 1)

    def _prepare_optional_scene_fields(self, node):
        if (len(node.bases) != 1 or not isinstance(node.bases[0], ast.Name)
                or node.bases[0].id != "Scene" or not self._trusted_noon_binding("Scene")
                or self.module_binding_counts.get("getattr", 0)):
            return
        methods = [member for member in node.body
                   if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))]
        method_names = {method.name for method in methods}
        if method_names & {"__getattr__", "__getattribute__"}:
            return
        class_bindings = _SourceBindings()
        for statement in node.body:
            class_bindings.visit(statement)
        class_names = class_bindings.names
        writes = {child.attr for method in methods for child in ast.walk(method)
                  if isinstance(child, ast.Attribute) and isinstance(child.ctx, (ast.Store, ast.Del))
                  and isinstance(child.value, ast.Name) and child.value.id == "self"}
        for method in methods:
            nodes = _walk_scope(method.body)
            argument_names = {argument.arg for argument in
                              [*method.args.posonlyargs, *method.args.args, *method.args.kwonlyargs,
                               *([method.args.vararg] if method.args.vararg else []),
                               *([method.args.kwarg] if method.args.kwarg else [])]}
            bindings = _SourceBindings()
            for statement in method.body:
                bindings.visit(statement)
            for call in (child for child in nodes if isinstance(child, ast.Call)):
                if (not isinstance(call.func, ast.Name) or call.func.id != "getattr"
                        or len(call.args) != 3 or call.keywords
                        or not isinstance(call.args[0], ast.Name) or call.args[0].id != "self"
                        or not isinstance(call.args[1], ast.Constant)
                        or type(call.args[1].value) is not str
                        or not call.args[1].value.startswith("_")
                        or call.args[1].value.startswith("__")
                        or call.args[1].value in writes | class_names | method_names | self.members
                        or _reserved_scene_field(call.args[1].value)
                        or not isinstance(call.args[2], ast.Constant) or call.args[2].value is not None):
                    continue
                assign = next((parent for parent in nodes
                               if isinstance(parent, ast.Assign) and parent.value is call), None)
                if (not assign or len(assign.targets) != 1 or not isinstance(assign.targets[0], ast.Name)
                        or bindings.binding_counts.get(assign.targets[0].id) != 1
                        or "getattr" in bindings.names):
                    continue
                optional_name = assign.targets[0].id
                if optional_name in argument_names or assign not in method.body:
                    continue
                assign_index = method.body.index(assign)
                guards = []
                for guard in (child for child in nodes if isinstance(child, ast.If)):
                    test = guard.test
                    if (isinstance(test, ast.Compare) and isinstance(test.left, ast.Name)
                            and test.left.id == optional_name and len(test.ops) == 1
                            and isinstance(test.ops[0], ast.IsNot) and len(test.comparators) == 1
                            and isinstance(test.comparators[0], ast.Constant)
                            and guard in method.body and method.body.index(guard) > assign_index
                            and test.comparators[0].value is None and guard.body):
                        dead = {id(child) for statement in guard.body for child in _walk_scope([statement])}
                        guards.append((guard, dead))
                loads = [child for child in nodes
                         if isinstance(child, ast.Name) and child.id == optional_name
                         and isinstance(child.ctx, ast.Load)]
                tests = {id(guard.test.left) for guard, _ in guards}
                dead = {item for _, nodes in guards for item in nodes}
                if guards and all(id(child) in tests or id(child) in dead for child in loads):
                    self.admitted_optional_scene_getters.add(id(call.func))
                    for guard, nodes in guards:
                        self.unreachable_scene_nodes.add(id(guard))
                        self.unreachable_scene_nodes.update(nodes)

    def visit_ClassDef(self, node):
        self._prepare_optional_scene_fields(node)
        if (node.decorator_list or node.keywords or getattr(node, "type_params", ()) or len(node.bases) != 1
                or not isinstance(node.bases[0], ast.Name)):
            raise _OpaqueSource(node, "unsupported_class_contract")
        spatial = node.bases[0].id == "ThreeDScene"
        moving_camera = node.bases[0].id == "MovingCameraScene"
        linear = node.bases[0].id == "LinearTransformationScene"
        if spatial or linear:
            if not self._trusted_noon_binding(node.bases[0].id):
                raise _OpaqueSource(node, "unsupported_class_contract")
            methods = [member for member in node.body
                       if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))]
            if (any(member.name not in {"__init__", "construct"} for member in methods)
                    or sum(member.name == "construct" for member in methods) > 1
                    or sum(member.name == "__init__" for member in methods) > 1
                    or any(member.name == "construct" and not self._simple_construct(member)
                           for member in methods)):
                raise _OpaqueSource(node, "unsupported_class_contract")
            for member in methods:
                if member.name == "__init__":
                    self._validate_super_init(member, {"leave_ghost_vectors": bool} if linear
                                              else {"near": (int, float), "far": (int, float)})
        elif moving_camera:
            if not self._trusted_noon_binding(node.bases[0].id):
                raise _OpaqueSource(node, "unsupported_class_contract")
            methods = [member for member in node.body
                       if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))]
            if (len(methods) != 1 or methods[0].name != "construct"
                    or not self._simple_construct(methods[0])):
                raise _OpaqueSource(node, "unsupported_class_contract")
        elif node.bases[0].id != "Scene":
            raise _OpaqueSource(node, "unsupported_class_contract")
        self.visit(node.bases[0])
        if any(not isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Expr, ast.Pass))
               or isinstance(member, ast.Expr) and not isinstance(member.value, ast.Constant)
               for member in node.body):
            raise _OpaqueSource(node, "unsupported_class_contract")
        constructs = [member for member in node.body
                      if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))
                      and member.name == "construct"]
        if len(constructs) > 1:
            raise _OpaqueSource(constructs[1], "unproven_audio_declaration")
        if constructs:
            self._prepare_scene_audio_calls(constructs[0])
        # Methods resolve module globals, not names in their class dictionary.
        previous_members = self.members
        previous_module_scope = self.scopes[0]
        previous_helpers = self.scene_helper_names
        declared_helpers = {member.name for member in node.body
                            if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))}
        self.scene_helper_names = declared_helpers
        if len(declared_helpers) != sum(isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))
                                        for member in node.body):
            raise _OpaqueSource(node, "unsupported_class_contract")
        owned_stores = {child.attr for member in node.body
                        if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))
                        for child in ast.walk(member)
                        if isinstance(child, ast.Attribute) and isinstance(child.ctx, ast.Store)
                        and isinstance(child.value, ast.Name) and child.value.id == "self"}
        if (owned_stores & (previous_members | declared_helpers)
                or any(_reserved_scene_field(name) for name in owned_stores)
                or any(name.startswith("__") for name in owned_stores)):
            raise _OpaqueSource(node, "unsupported_class_contract")
        for member in node.body:
            if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)):
                for child in ast.walk(member):
                    if isinstance(child, ast.Name) and child.id == "self" and isinstance(child.ctx, ast.Store):
                        raise _OpaqueSource(child, "unsupported_class_contract")
                    if (isinstance(child, ast.Attribute) and isinstance(child.value, ast.Name)
                            and child.value.id == "self" and child.attr in owned_stores):
                        self.admitted_scene_owned_attributes.add(id(child))
        if declared_helpers & previous_members:
            raise _OpaqueSource(node, "unsupported_class_contract")
        for member in node.body:
            if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)):
                for child in ast.walk(member):
                    if (isinstance(child, ast.Attribute) and isinstance(child.value, ast.Name)
                            and child.value.id == "self" and child.attr in declared_helpers):
                        self.admitted_scene_helper_attributes.add(id(child))
        if spatial:
            self.members = self.members | _SOURCE_SPATIAL_MEMBERS
            self.scopes[0] = self.scopes[0] | _SOURCE_SPATIAL_CONSTRUCTORS
        elif moving_camera:
            self.members = self.members | _SOURCE_MOVING_CAMERA_MEMBERS
        elif linear:
            self.members = self.members | _SOURCE_LINEAR_MEMBERS
            self.scopes[0] = self.scopes[0] | {"Typst"}
        try:
            for member in node.body:
                if (spatial or linear) and isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)) \
                        and member.name == "__init__":
                    continue
                if (spatial or linear) and isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    self.spatial_method_depth += spatial
                    self.linear_method_depth += linear
                    try:
                        self.visit(member)
                    finally:
                        self.spatial_method_depth -= spatial
                        self.linear_method_depth -= linear
                else:
                    self.visit(member)
        finally:
            self.members = previous_members
            self.scopes[0] = previous_module_scope
            self.scene_helper_names = previous_helpers

    def _trusted_noon_binding(self, name):
        if any(name in scope for scope in self.scopes[1:]):
            return False
        origin = self.module_binding_locations.get(name)
        if origin is None:
            return any(isinstance(imported, ast.ImportFrom) and imported.module == "noon"
                       and any(alias.name == "*" for alias in imported.names)
                       for imported in self.module_imports)
        return (origin in self.module_imports and isinstance(origin, ast.ImportFrom)
                and origin.module == "noon"
                and self.module_binding_counts.get(name) == 1
                and any(alias.name == name and alias.asname is None for alias in origin.names))

    @staticmethod
    def _simple_construct(node):
        return (not node.decorator_list and node.returns is None
                and not getattr(node, "type_params", ()) and not node.args.posonlyargs
                and len(node.args.args) == 1 and node.args.args[0].arg == "self"
                and not node.args.args[0].annotation and not node.args.defaults
                and not node.args.kwonlyargs and not node.args.kw_defaults
                and node.args.vararg is None and node.args.kwarg is None)

    @staticmethod
    def _validate_super_init(node, options):
        if not isinstance(node, ast.FunctionDef) or not _SceneOnlySource._simple_construct(node):
            raise _OpaqueSource(node, "unsupported_class_contract")
        body = node.body
        if (body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant)
                and isinstance(body[0].value.value, str)):
            body = body[1:]
        if len(body) != 1 or not isinstance(body[0], ast.Expr) or not isinstance(body[0].value, ast.Call):
            raise _OpaqueSource(node, "unsupported_class_contract")
        call = body[0].value
        target = call.func
        if (not isinstance(target, ast.Attribute) or target.attr != "__init__"
                or not isinstance(target.value, ast.Call) or not isinstance(target.value.func, ast.Name)
                or target.value.func.id != "super" or target.value.args or target.value.keywords
                or call.args or any(keyword.arg not in options for keyword in call.keywords)
                or len({keyword.arg for keyword in call.keywords}) != len(call.keywords)
                or any(not isinstance(keyword.value, ast.Constant)
                       or not isinstance(keyword.value.value, options[keyword.arg])
                       or type(keyword.value.value) is bool and options[keyword.arg] is not bool
                       for keyword in call.keywords)):
            raise _OpaqueSource(node, "unsupported_class_contract")

    def _prepare_scene_audio_calls(self, construct):
        arguments = [*construct.args.posonlyargs, *construct.args.args]
        if not arguments:
            return
        receiver = arguments[0].arg
        bindings = _SourceBindings()
        for statement in construct.body:
            bindings.visit(statement)
        if "AudioSource" in self.module_bindings | bindings.names | {arg.arg for arg in arguments}:
            return

        barrier_seen = False
        for statement in construct.body:
            call = statement.value if isinstance(statement, ast.Expr) else None
            if (isinstance(call, ast.Call) and isinstance(call.func, ast.Attribute)
                    and isinstance(call.func.value, ast.Name) and call.func.value.id == receiver
                    and call.func.attr == "audio"):
                if barrier_seen:
                    raise _OpaqueSource(call, "late_audio_declaration")
                if (len(call.args) != 1 or isinstance(call.args[0], ast.Starred)
                        or any(keyword.arg is None for keyword in call.keywords)):
                    raise _OpaqueSource(call, "unproven_audio_declaration")
                source = call.args[0]
                if (not isinstance(source, ast.Call) or not isinstance(source.func, ast.Attribute)
                        or not isinstance(source.func.value, ast.Name)
                        or source.func.value.id != "AudioSource"
                        or source.func.attr != "small_discovery"
                        or source.args or source.keywords):
                    raise _OpaqueSource(source, "unproven_audio_source")
                options = {}
                allowed = {"start_time", "duration", "loop", "gain", "fade_in", "fade_out"}
                for keyword in call.keywords:
                    if keyword.arg not in allowed or keyword.arg in options:
                        raise _OpaqueSource(keyword, "unproven_audio_option")
                    value = keyword.value
                    if keyword.arg == "loop":
                        valid = isinstance(value, ast.Constant) and type(value.value) is bool
                    else:
                        valid = (isinstance(value, ast.Constant) and type(value.value) in (int, float)
                                 and _finite_source_number(value.value))
                    if not valid:
                        raise _OpaqueSource(value, "unproven_audio_option")
                    options[keyword.arg] = value.value
                if "duration" not in options or options["duration"] <= 0:
                    raise _OpaqueSource(call, "unproven_audio_option")
                self.admitted_scene_audio_calls.add(id(call.func))
                self.admitted_audio_factory_attributes.add(id(source.func))
                continue
            if any(isinstance(child, ast.Call) and isinstance(child.func, ast.Attribute)
                   and isinstance(child.func.value, ast.Name) and child.func.value.id == receiver
                   and child.func.attr in {"play", "wait"} for child in ast.walk(statement)):
                barrier_seen = True

    def visit_Lambda(self, node):
        self.visit_FunctionDef(ast.copy_location(ast.FunctionDef(name="callback", args=node.args,
            body=[ast.Expr(value=node.body)], decorator_list=[], returns=None), node))

    def _comprehension(self, node):
        self.scopes.append(set())
        self.owned_containers.append(set())
        self.numpy_integer_names.append(set(self.numpy_integer_names[-1]))
        self.image_pixel_sources.append(self.image_pixel_sources[-1].copy())
        self.image_pixel_numeric_values.append(self.image_pixel_numeric_values[-1].copy())
        self.shader_values.append({})
        try:
            for generator in node.generators:
                if generator.is_async:
                    raise _OpaqueSource()
                self.visit(generator.iter)
                iterable = self._shader_value(generator.iter)
                bindings = _SourceBindings()
                bindings.visit(generator.target)
                self.scopes[-1].update(bindings.names)
                proven_integer_targets = _integer_iteration_targets(
                    generator.iter, generator.target, self.numpy_integer_names[-1], self.module_numpy_literals)
                target_names = {item.id for item in ast.walk(generator.target) if isinstance(item, ast.Name)}
                self.numpy_integer_names[-1].difference_update(target_names)
                self.numpy_integer_names[-1].update(proven_integer_targets)
                self.image_pixel_sources[-1].difference_update(bindings.names)
                if isinstance(generator.target, (ast.Tuple, ast.List)):
                    values = [self._loop_shader_value(iterable, index)
                              for index in range(len(generator.target.elts))]
                    self._assign_shader_value(generator.target, ("sequence", tuple(values)))
                else:
                    self._assign_shader_value(generator.target, self._loop_shader_value(iterable, 0))
                for condition in generator.ifs:
                    self.visit(condition)
            for value in ([node.key, node.value] if isinstance(node, ast.DictComp) else [node.elt]):
                self.visit(value)
            result = self._shader_value(node.key if isinstance(node, ast.DictComp) else node.elt)
            if result and result[0] == "key":
                result = ("keys", result[1])
            if result and result[0] in {"programs", "keys"}:
                self.shader_expression_values[id(node)] = result
        finally:
            self.shader_values.pop()
            self.image_pixel_sources.pop()
            self.numpy_integer_names.pop()
            self.image_pixel_numeric_values.pop()
            self.scopes.pop()
            self.owned_containers.pop()

    visit_ListComp = visit_SetComp = visit_DictComp = visit_GeneratorExp = _comprehension

    def visit_Subscript(self, node):
        if (isinstance(node.ctx, ast.Load)
                and _numpy_array_expression(node.value, self.numpy_array_names[-1], self.numpy_array_functions)
                and not _numpy_index_expression(node.slice, self.numpy_array_names[-1],
                                                self.module_numpy_literals, self.numpy_integer_names[-1])):
            raise _OpaqueSource(node, "unproven_numpy_index")
        if not isinstance(node.ctx, ast.Load):
            self._reject_shader_collection_mutation(node, node.value)
            name = node.value.id if isinstance(node.value, ast.Name) else None
            if name in self.numpy_array_names[-1] and id(node) not in self.safe_numpy_writes[-1]:
                raise _OpaqueSource(node, "unproven_numpy_write_origin")
            owner = next((owned for scope, owned in zip(reversed(self.scopes), reversed(self.owned_containers))
                if name in scope), ())
            owned_field = (isinstance(node.value, ast.Attribute)
                           and id(node.value) in self.admitted_scene_owned_attributes)
            if name not in owner and not owned_field:
                raise _OpaqueSource(node, "unproven_ownership")
        self.generic_visit(node)

    def visit_Return(self, node):
        if node.value is not None and self._shader_container_reference(node.value):
            raise _OpaqueSource(node, "unproven_shader_collection_escape")
        self.generic_visit(node)

    def visit_AugAssign(self, node):
        if isinstance(node.target, ast.Name) and node.target.id == "_":
            raise _OpaqueSource(node.target, "private_name")
        if isinstance(node.target, ast.Name):
            self._reject_shader_collection_mutation(node, node.target)
        if isinstance(node.target, ast.Name) and self.shader_values:
            self.shader_values[-1][node.target.id] = None
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
    "add_world_mobjects", "add_fixed_in_frame_mobjects", "add_fixed_orientation_mobjects",
})

CONSTRUCT_SCENE_BARRIERS = frozenset({"play", "wait", "move_camera"})
PORTABLE_SPATIAL_METHODS = frozenset({
    "move_camera", "set_camera_orientation", "point_light",
    "add_world_mobjects", "add_fixed_in_frame_mobjects", "add_fixed_orientation_mobjects",
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


@dataclass
class _CooperativeSourceYield:
    callback: object
    last_yield_at: float


_COOPERATIVE_SOURCE_YIELD: ContextVar[_CooperativeSourceYield | None] = ContextVar(
    "noon_cooperative_source_yield", default=None
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


@contextmanager
def cooperative_source_yield_scope(callback=None):
    """Carry an optional host fairness callback across one source execution."""
    state = (_CooperativeSourceYield(callback, perf_counter())
             if callback is not None else None)
    token = _COOPERATIVE_SOURCE_YIELD.set(state)
    try:
        yield
    finally:
        _COOPERATIVE_SOURCE_YIELD.reset(token)


def cooperative_source_yield_active():
    state = _COOPERATIVE_SOURCE_YIELD.get()
    return state is not None and state.callback is not None


async def await_source_checkpoint(value=None, *, condition=False):
    """Yield at a portable source boundary, preserving an optional expression value."""
    result = bool(value) if condition else value
    state = _COOPERATIVE_SOURCE_YIELD.get()
    if state is not None:
        now = perf_counter()
        if now - state.last_yield_at >= _COOPERATIVE_YIELD_QUANTUM_SECONDS:
            pending = state.callback()
            if pending:
                await pending
            state.last_yield_at = perf_counter()
    return result

# Known Scene calls, with suspension handled only at the explicit barriers above.
# Unknown methods (including super().construct()) keep their original host path.
_SCENE_CALLS = frozenset({
    "play", "wait", "audio", "add", "remove", "clear", "on_click", "set_drag_targets",
    "add_foreground_mobject", "add_foreground_mobjects",
    "activate_zooming", "get_zoomed_display_pop_out_animation",
    "bring_to_front", "bring_to_back", "replace", "next_section",
}) | PORTABLE_SPATIAL_METHODS


class _CooperativeSourceCheckpoints(ast.NodeTransformer):
    """Insert awaits only into the already-async portable source copy."""
    _SUITE_FIELDS = frozenset({"body", "orelse", "finalbody"})

    @staticmethod
    def _checkpoint_statement(location):
        call = ast.Call(func=ast.Name(id=COOPERATIVE_CHECKPOINT_GLOBAL, ctx=ast.Load()),
                        args=[], keywords=[])
        return ast.copy_location(ast.Expr(value=ast.Await(value=call)), location)

    @staticmethod
    def _checkpoint_value(value):
        call = ast.Call(func=ast.Name(id=COOPERATIVE_CHECKPOINT_GLOBAL, ctx=ast.Load()),
                        args=[value], keywords=[])
        return ast.copy_location(ast.Await(value=call), value)

    def generic_visit(self, node):
        for field, old_value in ast.iter_fields(node):
            if isinstance(old_value, ast.AST):
                setattr(node, field, self.visit(old_value))
                continue
            if not isinstance(old_value, list):
                continue
            if field in self._SUITE_FIELDS and all(isinstance(item, ast.stmt) for item in old_value):
                new_values = []
                prefix_end = 0
                if isinstance(node, ast.Module):
                    # Keep the module docstring and contiguous future imports
                    # before any injected statement, as required by Python.
                    if (old_value and isinstance(old_value[0], ast.Expr)
                            and isinstance(old_value[0].value, ast.Constant)
                            and isinstance(old_value[0].value.value, str)):
                        prefix_end = 1
                    while (prefix_end < len(old_value)
                           and isinstance(old_value[prefix_end], ast.ImportFrom)
                           and old_value[prefix_end].module == "__future__"):
                        prefix_end += 1
                for index, statement in enumerate(old_value):
                    rewritten = self.visit(statement)
                    if rewritten is None:
                        continue
                    new_values.extend(rewritten if isinstance(rewritten, list) else [rewritten])
                    if index >= prefix_end:
                        new_values.append(self._checkpoint_statement(statement))
            else:
                new_values = [self.visit(item) if isinstance(item, ast.AST) else item
                              for item in old_value]
            setattr(node, field, new_values)
        return node

    def _visit_comprehension(self, node, values):
        node = self.generic_visit(node)
        for generator in node.generators:
            generator.ifs = [self._checkpoint_condition(condition) for condition in generator.ifs]
        for field in values:
            setattr(node, field, self._checkpoint_value(getattr(node, field)))
        return node

    @staticmethod
    def _checkpoint_condition(condition):
        call = ast.Call(func=ast.Name(id=COOPERATIVE_CHECKPOINT_GLOBAL, ctx=ast.Load()),
                        args=[condition], keywords=[ast.keyword(arg="condition", value=ast.Constant(True))])
        return ast.copy_location(ast.Await(value=call), condition)

    def visit_ListComp(self, node):
        return self._visit_comprehension(node, ("elt",))

    def visit_SetComp(self, node):
        return self._visit_comprehension(node, ("elt",))

    def visit_DictComp(self, node):
        # A dict key is evaluated before its value; checkpoint after the value
        # so the transformation preserves that original pair ordering.
        return self._visit_comprehension(node, ("value",))

    def visit_For(self, node):
        node = self.generic_visit(node)
        node.body.insert(0, self._checkpoint_statement(node))
        return node

    visit_AsyncFor = visit_For

    def visit_While(self, node):
        node = self.generic_visit(node)
        node.body.insert(0, self._checkpoint_statement(node))
        return node


class _ConstructBody(_CooperativeSourceCheckpoints):
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
                and call.func.attr in CONSTRUCT_SCENE_BARRIERS
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
            if node.attr in CONSTRUCT_SCENE_BARRIERS:
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
        candidate.body = body.generic_visit(ast.Module(body=candidate.body, type_ignores=[])).body
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


class _ModuleBarriers(_CooperativeSourceCheckpoints):
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
    if not portable or any(name in source for name in (
            BARRIER_GLOBAL, MODULE_BARRIER_GLOBAL,
            COOPERATIVE_CHECKPOINT_GLOBAL)):
        return original, {}
    originals = list(_function_codes(original))
    has_construct = any(
        code.co_name == "construct"
        and not code.co_flags & (inspect.CO_COROUTINE | inspect.CO_GENERATOR)
        and (CONSTRUCT_SCENE_BARRIERS | {"add_effect"}).intersection(code.co_names)
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
    namespace.setdefault(COOPERATIVE_CHECKPOINT_GLOBAL, await_source_checkpoint)
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
    original.__globals__.setdefault(COOPERATIVE_CHECKPOINT_GLOBAL, await_source_checkpoint)
    portable.__kwdefaults__ = original.__kwdefaults__
    return MethodType(portable, method.__self__)


def has_portable_scene_methods(scene: object, **methods: FunctionType | None) -> bool:
    """Admit only ordinary method lookup, without invoking authored descriptors.

    Class-only checks miss instance overrides installed by setup(). Dynamic
    attribute lookup and descriptors retain their original synchronous behavior.
    """
    return (
        type(scene).__getattribute__ is object.__getattribute__
        and all(inspect.getattr_static(scene, name, None) is method
                for name, method in methods.items())
    )
