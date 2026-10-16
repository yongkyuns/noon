"""Thin Graph/DiGraph adapters over the shared Rust authoring handle.

Python owns only its arbitrary hashable-key to u32 mapping. Rust owns graph
topology, arrow endpoints, persistent placements, and publication.
"""

from __future__ import annotations

from collections.abc import Hashable, Mapping, Sequence
from numbers import Integral

import _manim_compat as _compat
import _manim_semantic_handles as _shared
import noon as _base
from _noon_errors import engine_call

try:
    from js import noonAuthoringGraphOptions as _graph_options
    from pyodide.ffi import to_js as _to_js
except ImportError:  # pragma: no cover - native import smoke only
    _graph_options = None
    _to_js = None


def _array(values):
    if _to_js is None:
        raise RuntimeError("Graph requires the shared Rust authoring host")
    return _to_js(list(values))


def _point(value):
    point = _base._as_vec2(value)
    return float(point.x), float(point.y)


_VERTEX_CONFIG_KEYS = frozenset(
    {"color", "fill_color", "stroke_color", "stroke_width", "radius", "fill_opacity"}
)
_EDGE_CONFIG_KEYS = frozenset({"color", "stroke_color", "stroke_width"})


def _split_style_config(name, value, supported, entity_keys):
    if value is None:
        return {}, {}
    if not isinstance(value, Mapping):
        raise TypeError(f"{name} must be a mapping")
    global_values, per_key = {}, {}
    for key, option in dict(value).items():
        if key in entity_keys:
            if not isinstance(option, Mapping):
                raise TypeError(f"{name}[{key!r}] must be a mapping of style options")
            per_key[key] = _validated_key_style(name, option)
        elif isinstance(key, str) and key in supported:
            global_values[key] = option
        elif isinstance(key, tuple) and name == "edge_config":
            raise ValueError(f"edge_config references unknown edge {key!r}")
        elif not isinstance(key, str):
            raise NotImplementedError(f"per-key {name} key {key!r} is not present in this graph")
        else:
            raise TypeError(f"unsupported {name} option: {key}")
    return global_values, per_key


def _validated_key_style(name, value):
    supported = {"color", "fill_color", "stroke_color", "fill_opacity", "stroke_width"}
    unknown = [key for key in value if key not in supported]
    if unknown:
        raise NotImplementedError(
            f"unsupported per-key {name} style option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    result = dict(value)
    for key in ("color", "fill_color", "stroke_color"):
        if key in result:
            result[key] = _compat._as_color(f"{name}.{key}", result[key])
    if "fill_opacity" in result:
        result["fill_opacity"] = _compat._opacity(f"{name}.fill_opacity", result["fill_opacity"])
    if "stroke_width" in result:
        _compat._manim_stroke_width(result["stroke_width"])
    return result


def _apply_key_style(target, values):
    fill_color = values.get("fill_color", values.get("color"))
    stroke_color = values.get("stroke_color", values.get("color"))
    target.set_style(
        fill_color=fill_color,
        fill_opacity=values.get("fill_opacity"),
        stroke_color=stroke_color,
        stroke_width=values.get("stroke_width"),
    )


def _set_color(method, name, value):
    color = _compat._as_color(name, value)
    engine_call(method, color.red, color.green, color.blue, color.alpha,
                operation="Graph.style")


def _graph_style_options(vertex_config, edge_config):
    if _graph_options is None:
        raise RuntimeError("Graph requires the shared Rust authoring host")
    vertices = vertex_config
    edges = edge_config
    options = engine_call(_graph_options, operation="Graph.style")
    try:
        if "color" in vertices:
            _set_color(options.setVertexFill, "vertex_config.color", vertices["color"])
            _set_color(options.setVertexStroke, "vertex_config.color", vertices["color"])
        if "fill_color" in vertices:
            _set_color(options.setVertexFill, "vertex_config.fill_color", vertices["fill_color"])
        if "stroke_color" in vertices:
            _set_color(options.setVertexStroke, "vertex_config.stroke_color", vertices["stroke_color"])
        if "radius" in vertices:
            engine_call(options.setVertexRadius, float(vertices["radius"]), operation="Graph.style")
        if "fill_opacity" in vertices:
            engine_call(options.setVertexFillOpacity, float(vertices["fill_opacity"]), operation="Graph.style")
        if "stroke_width" in vertices:
            engine_call(options.setVertexStrokeWidth, float(vertices["stroke_width"]), operation="Graph.style")
        color = edges.get("stroke_color", edges.get("color"))
        if color is not None:
            _set_color(options.setEdgeColor, "edge_config.color", color)
        if "stroke_width" in edges:
            engine_call(options.setEdgeStrokeWidth, float(edges["stroke_width"]), operation="Graph.style")
    except BaseException:
        options.free()
        raise
    return options


class _GraphBase(_compat.Group):
    """Shared retained Graph/DiGraph facade.

    The default spring layout accepts NetworkX's ``seed``, ``pos``, ``fixed``,
    ``k``, ``iterations``, ``threshold``, ``weight``, ``center``, and ``dim=2``
    configuration. NumPy prepares NetworkX-compatible initial positions while
    Rust owns force integration. Circular layouts accept ``center`` and ``dim=2``
    through ``layout_config``; random layouts accept ``seed``, ``center``, and
    ``dim=2``. Shell layouts accept ``nlist``, ``rotate``,
    ``center``, and ``dim=2``; spiral layouts accept ``resolution``,
    ``equidistant``, ``center``, and ``dim=2``; partite layouts accept
    ``partitions``, ``align``, and ``center``. Other options remain unsupported
    until their Rust semantics are qualified. Constructor
    ``vertex_config`` and ``edge_config`` accept per-key color, fill, stroke,
    fill-opacity, and stroke-width overrides for vertices and edges present at
    construction time; other per-key constructor options remain unsupported.
    """

    # This typed vector wrapper retains the shared family-paint contract.
    set_opacity = _compat.VGroup.set_opacity
    set_fill = _compat.VGroup.set_fill
    set_stroke = _compat.VGroup.set_stroke

    _directed = False

    def __init__(self, vertices, edges, layout="spring", *, layout_scale=2.0,
                 layout_center=(0.0, 0.0), layout_config=None, **kwargs):
        vertex_config = kwargs.pop("vertex_config", None)
        edge_config = kwargs.pop("edge_config", None)
        if kwargs:
            raise TypeError("unsupported Graph option(s): " + ", ".join(sorted(kwargs)))
        vertex_mapping = vertices if isinstance(vertices, Mapping) else None
        vertices = tuple(vertices)
        edges = list(edges)
        explicit = layout if isinstance(layout, Mapping) else (
            vertex_mapping if layout in {None, "explicit"} and vertex_mapping is not None else None
        )
        spring = isinstance(layout, str) and layout == "spring"
        random_layout = isinstance(layout, str) and layout == "random"
        circular = isinstance(layout, str) and layout == "circular"
        shell = isinstance(layout, str) and layout == "shell"
        spiral = isinstance(layout, str) and layout == "spiral"
        partite = isinstance(layout, str) and layout == "partite"
        if (explicit is None and not spring and not random_layout and not circular
                and not shell and not spiral and not partite):
            raise NotImplementedError("Graph requires a qualified layout or a complete position mapping")
        self._graph_key_ids = {}
        self._next_graph_id = 0
        positions = []
        for key in vertices:
            if not isinstance(key, Hashable):
                raise TypeError("Graph vertex keys must be hashable")
            if key in self._graph_key_ids:
                raise ValueError("duplicate Graph vertex key")
            self._graph_key_ids[key] = self._next_graph_id
            self._next_graph_id += 1
            if explicit is not None:
                try:
                    positions.extend(_point(explicit[key]))
                except KeyError as error:
                    raise ValueError("Graph layout must cover every vertex exactly once") from error
        if explicit is not None and len(explicit) != len(self._graph_key_ids):
            raise ValueError("Graph layout must cover every vertex exactly once")
        edge_ids = self._edge_ids(edges)
        global_vertex_config, keyed_vertex_config = _split_style_config(
            "vertex_config", vertex_config, _VERTEX_CONFIG_KEYS, set(self._graph_key_ids)
        )
        global_edge_config, keyed_edge_config = _split_style_config(
            "edge_config", edge_config, _EDGE_CONFIG_KEYS, set(edges)
        )
        center = (0.0, 0.0)
        if circular:
            center = _circular_layout_center(layout_config, layout_center)
        shell_groups, rotate = ([], None)
        resolution, equidistant = (0.35, False)
        partitions, align_horizontal = ([], False)
        spring_positions, fixed_indices, spring_k = ([], [], None)
        random_positions = []
        spring_iterations, spring_threshold, spring_rescale = (50, 1e-4, True)
        spring_fixed_is_set = False
        if spring:
            (center, spring_positions, fixed_indices, spring_k,
             spring_iterations, spring_threshold, spring_rescale,
             scale, spring_fixed_is_set) = _spring_layout_config(
                layout_config, layout_center, self._graph_key_ids, layout_scale
            )
        elif random_layout:
            center, random_positions, scale = _random_layout_config(
                layout_config, layout_center, self._graph_key_ids, layout_scale
            )
        elif shell:
            center, shell_groups, rotate = _shell_layout_config(
                layout_config, layout_center, self._graph_key_ids
            )
        elif spiral:
            center, resolution, equidistant = _spiral_layout_config(
                layout_config, layout_center, len(self._graph_key_ids)
            )
        elif partite:
            center, partitions, align_horizontal = _partite_layout_config(
                layout_config, layout_center, self._graph_key_ids
            )
        if not spring and not random_layout:
            scale = _shared._ir._finite_number("layout_scale", layout_scale)
        context = _shared._live_constructor_context("Graph", allow_unstarted=True)
        if context is None:
            raise RuntimeError("Graph construction requires a canonical Scene authoring context")
        options = _graph_style_options(global_vertex_config, global_edge_config)
        ids = _array(self._graph_key_ids.values())
        try:
            if spring:
                self._graph_handle = engine_call(
                    context.liveCreateSpringGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(value for point in spring_positions for value in point),
                    _array(fixed_indices), spring_fixed_is_set,
                    scale, *center, spring_k, spring_iterations, spring_threshold,
                    spring_rescale, options, operation="Graph.create",
                )
            elif random_layout:
                self._graph_handle = engine_call(
                    context.liveCreateRandomGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(value for point in random_positions for value in point),
                    scale, options, operation="Graph.create",
                )
            elif circular:
                self._graph_handle = engine_call(
                    context.liveCreateCircularGraph, bool(self._directed), ids, _array(edge_ids),
                    scale, *center, options, operation="Graph.create",
                )
            elif shell:
                shell_indices, shell_sizes = _flatten_groups(shell_groups)
                self._graph_handle = engine_call(
                    context.liveCreateShellGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(shell_indices), _array(shell_sizes), scale, *center, rotate,
                    options, operation="Graph.create",
                )
            elif spiral:
                self._graph_handle = engine_call(
                    context.liveCreateSpiralGraph,
                    bool(self._directed), ids, _array(edge_ids), scale, *center,
                    resolution, equidistant, options, operation="Graph.create",
                )
            elif partite:
                partition_indices, partition_sizes = _flatten_groups(partitions)
                self._graph_handle = engine_call(
                    context.liveCreatePartiteGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(partition_indices), _array(partition_sizes),
                    scale, *center, align_horizontal, options,
                    operation="Graph.create",
                )
            else:
                self._graph_handle = engine_call(
                    context.liveCreateGraph, bool(self._directed), ids, _array(positions),
                    _array(edge_ids), options, operation="Graph.create",
                )
        except BaseException:
            options.free()
            raise
        _shared._attach_shared_family(self, engine_call(self._graph_handle.family), context)
        self._canonical_live_target_context = context
        for key, style in keyed_vertex_config.items():
            _apply_key_style(self[key], style)
        for edge, style in keyed_edge_config.items():
            _apply_key_style(self._edge_style_target(edge), style)

    def _edge_ids(self, edges):
        values = []
        for start, end in edges:
            try:
                values.extend((self._graph_key_ids[start], self._graph_key_ids[end]))
            except KeyError as error:
                raise ValueError("Graph edge references an unknown vertex key") from error
        return values

    def _edge_style_target(self, edge):
        if len(edge) != 2:
            raise ValueError("edge_config keys must be two-vertex tuples")
        try:
            start, end = (self._graph_key_ids[key] for key in edge)
        except KeyError as error:
            raise ValueError(f"edge_config references unknown vertex {error.args[0]!r}") from error
        family = engine_call(self._graph_handle.edge, start, end, operation="Graph.edge")
        if family is None:
            raise ValueError(f"edge_config references unknown edge {edge!r}")
        target = object.__new__(_compat.Group)
        _shared._attach_shared_family(target, family, self._context())
        target._canonical_live_target_context = self._context()
        return target

    def _refresh_semantic_members(self):
        _shared._attach_shared_family(self, self._semantic_family_handle, self._context())

    def _context(self):
        return self._canonical_live_target_context

    def __getitem__(self, key):
        """Resolve a vertex key to its existing shared semantic Mobject."""
        handle = engine_call(self._graph_handle.vertex, self._graph_key_ids[key])
        if handle is None:
            raise KeyError(key)
        identity = f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"
        member = self._semantic_member_wrappers.get(identity)
        if member is None:
            member = object.__new__(_compat.VMobject)
            _shared._attach_shared_handle(member, handle)
            member._canonical_live_target_context = self._context()
            self._semantic_member_wrappers[identity] = member
        return member

    @property
    def vertices(self):
        return {key: self[key] for key in self._graph_key_ids}

    def add_vertices(self, vertices):
        if not isinstance(vertices, Mapping):
            raise TypeError("Graph vertices must be a mapping of hashable keys to positions")
        pending, ids, positions = [], [], []
        next_id = self._next_graph_id
        for key, position in vertices.items():
            if not isinstance(key, Hashable):
                raise TypeError("Graph vertex keys must be hashable")
            if key in self._graph_key_ids or any(key == item[0] for item in pending):
                raise ValueError("duplicate Graph vertex key")
            x, y = _point(position)
            pending.append((key, x, y))
            ids.append(next_id)
            next_id += 1
            positions.extend((x, y))
        engine_call(self._context().liveGraphAddVertices, self._graph_handle,
                    _array(ids), _array(positions), operation="Graph.add_vertices")
        self._graph_key_ids.update((key, vertex_id) for (key, _, _), vertex_id in zip(pending, ids))
        self._next_graph_id = next_id
        return self

    def add_edges(self, edges):
        """Add only edges whose endpoints already exist.

        Manim's implicit missing-vertex creation needs a shared compound
        publication with default placement; it is deliberately unsupported
        here so Python never splits that operation into transactions.
        """
        engine_call(self._context().liveGraphAddEdges, self._graph_handle,
                    _array(self._edge_ids(edges)), operation="Graph.add_edges")
        return self

    def remove_vertices(self, vertices):
        keys = list(vertices)
        try:
            ids = [self._graph_key_ids[key] for key in keys]
        except KeyError as error:
            raise ValueError("unknown Graph vertex key") from error
        engine_call(self._context().liveGraphRemoveVertices, self._graph_handle,
                    _array(ids), operation="Graph.remove_vertices")
        for key in keys:
            del self._graph_key_ids[key]
        return self

    def remove_edges(self, edges):
        engine_call(self._context().liveGraphRemoveEdges, self._graph_handle,
                    _array(self._edge_ids(edges)), operation="Graph.remove_edges")
        return self

    def copy(self):
        """Copy the retained family, including appearance and endpoint bindings."""
        clone = object.__new__(type(self))
        clone._directed = self._directed
        clone._graph_key_ids = dict(self._graph_key_ids)
        clone._next_graph_id = self._next_graph_id
        clone._canonical_live_target_context = self._context()
        clone._graph_handle = engine_call(
            clone._canonical_live_target_context.liveGraphCopy,
            self._graph_handle,
            operation="Graph.copy",
        )
        _shared._attach_shared_family(clone, engine_call(clone._graph_handle.family), self._context())
        return clone

    def change_layout(self, layout="spring", *, scale=2.0, center=(0.0, 0.0),
                      positions=None, layout_config=None):
        if isinstance(layout, Mapping):
            positions, layout = layout, "explicit"
        if layout == "spring":
            (center, initial_positions, fixed_indices, spring_k,
             iterations, threshold, rescale, scale, fixed_is_set) = _spring_layout_config(
                layout_config, center, self._graph_key_ids, scale
            )
            engine_call(
                self._context().liveGraphSpringLayout,
                self._graph_handle,
                _array(value for point in initial_positions for value in point),
                _array(fixed_indices), fixed_is_set,
                scale, *center, spring_k, iterations, threshold, rescale,
                operation="Graph.spring_layout",
            )
        elif layout == "random":
            _, random_positions, scale = _random_layout_config(
                layout_config, center, self._graph_key_ids, scale
            )
            engine_call(
                self._context().liveGraphRandomLayout,
                self._graph_handle,
                _array(value for point in random_positions for value in point),
                scale,
                operation="Graph.random_layout",
            )
        elif layout == "circular":
            center = _circular_layout_center(layout_config, center)
            engine_call(self._context().liveGraphCircularLayout, self._graph_handle,
                        float(scale), *_point(center), operation="Graph.circular_layout")
        elif layout == "shell":
            vertex_order = {key: index for index, key in enumerate(self._graph_key_ids)}
            center, groups, rotate = _shell_layout_config(
                layout_config, center, vertex_order
            )
            shell_indices, shell_sizes = _flatten_groups(groups)
            engine_call(
                self._context().liveGraphShellLayout,
                self._graph_handle,
                _array(shell_indices), _array(shell_sizes),
                _shared._ir._finite_number("scale", scale), *center, rotate,
                operation="Graph.shell_layout",
            )
        elif layout == "spiral":
            center, resolution, equidistant = _spiral_layout_config(
                layout_config, center, len(self._graph_key_ids)
            )
            engine_call(
                self._context().liveGraphSpiralLayout,
                self._graph_handle,
                _shared._ir._finite_number("scale", scale), *center,
                resolution, equidistant,
                operation="Graph.spiral_layout",
            )
        elif layout == "partite":
            center, partitions, align_horizontal = _partite_layout_config(
                layout_config, center, self._graph_key_ids
            )
            partition_indices, partition_sizes = _flatten_groups(partitions)
            engine_call(
                self._context().liveGraphPartiteLayout,
                self._graph_handle,
                _array(partition_indices), _array(partition_sizes),
                _shared._ir._finite_number("scale", scale), *center,
                align_horizontal,
                operation="Graph.partite_layout",
            )
        elif layout == "explicit":
            if not isinstance(positions, Mapping):
                raise TypeError("explicit Graph layout requires a key-to-position mapping")
            try:
                ordered = [_point(positions[key]) for key in self._graph_key_ids]
            except KeyError as error:
                raise ValueError("explicit Graph layout must cover every current vertex") from error
            if len(positions) != len(ordered):
                raise ValueError("explicit Graph layout must cover every current vertex exactly once")
            engine_call(self._context().liveGraphExplicitLayout, self._graph_handle,
                        _array(value for point in ordered for value in point),
                        operation="Graph.explicit_layout")
        else:
            raise NotImplementedError(
                "Python Graph qualifies only spring, random, circular, shell, spiral, partite, and explicit layouts"
            )
        return self


def _spring_layout_config(layout_config, fallback_center, vertex_ids, layout_scale):
    """Prepare NetworkX-compatible initial positions for Rust force integration."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"center", "dim", "seed", "pos", "fixed", "k", "iterations", "threshold", "weight"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported spring layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )

    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, Integral):
        raise TypeError("spring layout_config dim must be an integer")
    if dimension != 2:
        raise NotImplementedError("Graph currently supports only two-dimensional spring layouts")
    raw_center = options.get("center", fallback_center)
    center = _point((0.0, 0.0) if raw_center is None else raw_center)
    count = len(vertex_ids)
    fixed_is_set = "fixed" in options and options["fixed"] is not None

    pos = options.get("pos")
    if pos is not None and not isinstance(pos, Mapping):
        raise TypeError("spring layout_config pos must map vertex keys to 2D positions")
    fixed = options.get("fixed")
    if fixed_is_set:
        if pos is None:
            raise ValueError("spring fixed vertices require a pos mapping")
        if isinstance(fixed, (str, bytes)):
            raise TypeError("spring layout_config fixed must be a sequence of vertex keys")
        try:
            fixed = list(fixed)
        except TypeError as error:
            raise TypeError("spring layout_config fixed must be a sequence of vertex keys") from error
        missing_positions = [key for key in fixed if key not in pos]
        if missing_positions:
            raise ValueError(f"spring fixed vertex {missing_positions[0]!r} is missing from pos")
    else:
        fixed = []

    raw_k = options.get("k")
    spring_k = None if raw_k is None else _shared._ir._finite_number("layout_config.k", raw_k)
    if spring_k == 0:
        raise ValueError("spring layout_config k must be non-zero")
    iterations = options.get("iterations", 50)
    if isinstance(iterations, bool) or not isinstance(iterations, Integral):
        raise TypeError("spring layout_config iterations must be an integer")
    if iterations < 0:
        raise ValueError("spring layout_config iterations must be non-negative")
    threshold = _shared._ir._finite_number(
        "layout_config.threshold", options.get("threshold", 1e-4)
    )
    if threshold < 0:
        raise ValueError("spring layout_config threshold must be non-negative")
    weight = options.get("weight", "weight")
    if weight is not None and not isinstance(weight, str):
        raise TypeError("spring layout_config weight must be a string or None")
    if layout_scale is None:
        scale, rescale = 1.0, False
    else:
        scale = _shared._ir._finite_number("layout_scale", layout_scale)
        rescale = not fixed_is_set

    np, rng = _numpy_random_source(options.get("seed"), "spring")

    def random_array(shape):
        return _numpy_random_array(rng, shape)

    provided = {}
    if pos is not None:
        try:
            domain_size = max(float(coordinate) for value in pos.values() for coordinate in value)
        except ValueError as error:
            raise ValueError("spring layout_config pos must contain at least one 2D position") from error
        if domain_size == 0:
            domain_size = 1.0
        for key in vertex_ids:
            if key in pos:
                point = _point(pos[key])
                provided[key] = point
        initial = random_array((count, 2)) * domain_size + np.asarray(center)
        for key, point in provided.items():
            initial[vertex_ids[key], :] = point
    else:
        domain_size = 1.0
        if count <= 1:
            initial = np.asarray([center] if count == 1 else [], dtype=float).reshape((count, 2))
        else:
            initial = random_array((count, 2))

    fixed_indices = [vertex_ids[key] for key in fixed if key in vertex_ids]

    if spring_k is None and fixed_is_set and count > 1:
        spring_k = domain_size / (count ** 0.5)

    initial_positions = [tuple(map(float, point)) for point in initial]
    return (
        center, initial_positions, fixed_indices, spring_k, int(iterations), threshold,
        rescale, scale, fixed_is_set,
    )


def _random_layout_config(layout_config, fallback_center, vertex_ids, layout_scale):
    """Prepare Manim's scaled NetworkX random-layout coordinates."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"center", "dim", "seed"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported random layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )

    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, Integral):
        raise TypeError("random layout_config dim must be an integer")
    if dimension != 2:
        raise NotImplementedError("Graph currently supports only two-dimensional random layouts")
    raw_center = options.get("center", fallback_center)
    center = _point((0.0, 0.0) if raw_center is None else raw_center)
    scale = _shared._ir._finite_number("layout_scale", layout_scale)

    np, rng = _numpy_random_source(options.get("seed"), "random")
    positions = (_numpy_random_array(rng, (len(vertex_ids), 2)) + np.asarray(center)).astype(np.float32)
    prepared = [tuple(map(float, point)) for point in positions]
    return center, prepared, scale


def _numpy_random_source(seed, layout_name):
    try:
        import numpy as np
    except ImportError as error:  # pragma: no cover - only available in the browser runtime
        raise RuntimeError(f"{layout_name} Graph layout requires NumPy in the Manim runtime") from error
    if seed is None:
        rng = np.random
    elif seed is np.random:
        rng = seed
    elif isinstance(seed, Integral):
        rng = np.random.RandomState(int(seed))
    elif isinstance(seed, np.random.RandomState):
        rng = seed
    elif hasattr(np.random, "Generator") and isinstance(seed, np.random.Generator):
        rng = seed
    else:
        raise TypeError(
            f"{layout_name} layout_config seed must be an integer, RandomState, Generator, or None"
        )
    return np, rng


def _numpy_random_array(rng, shape):
    return rng.rand(*shape) if hasattr(rng, "rand") else rng.random(shape)


def _circular_layout_center(layout_config, fallback_center):
    """Resolve the supported NetworkX circular-layout configuration subset."""
    if layout_config is None:
        return _point(fallback_center)
    if not isinstance(layout_config, Mapping):
        raise TypeError("layout_config must be a mapping")
    options = dict(layout_config)
    unknown = [key for key in options if key not in {"center", "dim"}]
    if unknown:
        raise NotImplementedError(
            "unsupported circular layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, int):
        raise TypeError("circular layout_config dim must be an integer")
    if dimension < 2:
        raise ValueError("circular layout_config dim must be at least 2")
    if dimension > 2:
        raise NotImplementedError("Graph currently supports only two-dimensional layouts")
    return _point(options.get("center", fallback_center))


def _shell_layout_config(layout_config, fallback_center, vertex_ids):
    """Resolve the qualified 2D NetworkX shell-layout option subset."""
    if layout_config is None:
        return _point(fallback_center), [], None
    if not isinstance(layout_config, Mapping):
        raise TypeError("layout_config must be a mapping")
    options = dict(layout_config)
    unknown = [key for key in options if key not in {"center", "dim", "nlist", "rotate"}]
    if unknown:
        raise NotImplementedError(
            "unsupported shell layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, int):
        raise TypeError("shell layout_config dim must be an integer")
    if dimension != 2:
        raise ValueError("shell layout currently supports only dim=2")
    center = _point(options.get("center", fallback_center))
    # NetworkX returns the center directly for empty and singleton graphs,
    # before inspecting shell membership or rotation.
    if len(vertex_ids) <= 1:
        return center, [], None
    rotate = options.get("rotate")
    if rotate is not None:
        rotate = _shared._ir._finite_number("layout_config.rotate", rotate)
    nlist = options.get("nlist")
    groups = []
    if nlist is not None:
        if isinstance(nlist, (str, bytes)) or not isinstance(nlist, Sequence):
            raise TypeError("shell layout_config nlist must be a sequence of vertex sequences")
        if len(nlist) == 0 and vertex_ids:
            raise ValueError("shell layout_config nlist must not be empty")
        seen = set()
        for shell in nlist:
            if isinstance(shell, (str, bytes)) or not isinstance(shell, Sequence):
                raise TypeError("each shell in layout_config nlist must be a vertex sequence")
            group = []
            for key in shell:
                try:
                    vertex_id = vertex_ids[key]
                except (KeyError, TypeError) as error:
                    raise ValueError(f"shell layout references unknown vertex {key!r}") from error
                if vertex_id in seen:
                    raise ValueError("shell layout must assign each graph vertex exactly once")
                seen.add(vertex_id)
                group.append(vertex_id)
            groups.append(group)
        if len(seen) != len(vertex_ids):
            raise ValueError("shell layout must assign each graph vertex exactly once")
    return center, groups, rotate


def _flatten_groups(groups):
    return [index for group in groups for index in group], [len(group) for group in groups]


def _spiral_layout_config(layout_config, fallback_center, vertex_count):
    """Resolve the qualified 2D NetworkX spiral-layout option subset."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    unknown = [key for key in options if key not in {"center", "dim", "resolution", "equidistant"}]
    if unknown:
        raise NotImplementedError(
            "unsupported spiral layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, int):
        raise TypeError("spiral layout_config dim must be an integer")
    if dimension != 2:
        raise ValueError("spiral layout currently supports only dim=2")
    center = _point(options.get("center", fallback_center))
    # NetworkX returns immediately for empty and singleton graphs, so layout
    # tuning values are intentionally not inspected in those cases.
    if vertex_count <= 1:
        return center, 0.35, False
    resolution = _shared._ir._finite_number(
        "layout_config.resolution", options.get("resolution", 0.35)
    )
    equidistant = options.get("equidistant", False)
    if not isinstance(equidistant, bool):
        raise TypeError("spiral layout_config equidistant must be a bool")
    return center, resolution, equidistant


def _partite_layout_config(layout_config, fallback_center, vertex_ids):
    """Resolve Manim's partition assignment and qualified NetworkX align options."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    unknown = [key for key in options if key not in {"partitions", "align", "center"}]
    if unknown:
        raise NotImplementedError(
            "unsupported partite layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    center = _point(options.get("center", fallback_center))
    align = options.get("align", "vertical")
    if align not in ("vertical", "horizontal"):
        raise ValueError("partite layout_config align must be 'vertical' or 'horizontal'")
    specifications = options.get("partitions")
    if specifications is None or isinstance(specifications, (str, bytes)) or not isinstance(specifications, Sequence):
        raise ValueError("partite layout requires a non-empty partitions sequence")
    if not specifications:
        raise ValueError("partite layout requires a non-empty partitions sequence")

    position_by_key = {key: index for index, key in enumerate(vertex_ids)}
    layer_by_position = {}
    for layer, partition in enumerate(specifications):
        if isinstance(partition, (str, bytes)) or not isinstance(partition, Sequence):
            raise TypeError("each partite partition must be a sequence of vertex keys")
        for key in partition:
            try:
                position = position_by_key[key]
            except (KeyError, TypeError) as error:
                raise ValueError(f"partite layout references unknown vertex {key!r}") from error
            # Manim assigns each partition in order, so a repeated key receives
            # the subset number from its last occurrence.
            layer_by_position[position] = layer
    for position in range(len(position_by_key)):
        layer_by_position.setdefault(position, len(specifications))

    ordered_layers = sorted(set(layer_by_position.values()))
    groups = [
        [position for position in range(len(position_by_key))
         if layer_by_position[position] == layer]
        for layer in ordered_layers
    ]
    return center, groups, align == "horizontal"


class Graph(_GraphBase):
    """Undirected retained Graph with qualified spring, random, circular, shell, spiral, partite, and explicit layouts."""


class DiGraph(_GraphBase):
    """Directed retained Graph with shared Rust spring/random layouts and Arrow semantics."""
    _directed = True


__all__ = ["Graph", "DiGraph"]
