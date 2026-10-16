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


def _layout_scale_pair(value):
    """Return independent x/y scale values accepted by Manim's 2D layouts."""
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes)):
        if len(value) != 2:
            raise ValueError("2D layout_scale tuples must contain x and y values")
        return (
            _shared._ir._finite_number("layout_scale[0]", value[0]),
            _shared._ir._finite_number("layout_scale[1]", value[1]),
        )
    return _shared._ir._finite_number("layout_scale", value), None


_VERTEX_CONFIG_KEYS = frozenset(
    {"color", "fill_color", "stroke_color", "stroke_width", "radius", "fill_opacity"}
)
_EDGE_CONFIG_KEYS = frozenset({"color", "stroke_color", "stroke_width"})


def _split_style_config(name, value, supported, entity_keys, per_key_supported=None):
    if value is None:
        return {}, {}
    if not isinstance(value, Mapping):
        raise TypeError(f"{name} must be a mapping")
    global_values, per_key = {}, {}
    for key, option in dict(value).items():
        if key in entity_keys:
            if not isinstance(option, Mapping):
                raise TypeError(f"{name}[{key!r}] must be a mapping of style options")
            per_key[key] = _validated_key_style(name, option, per_key_supported)
        elif isinstance(key, str) and key in supported:
            global_values[key] = option
        elif isinstance(key, tuple) and name == "edge_config":
            raise ValueError(f"edge_config references unknown edge {key!r}")
        elif not isinstance(key, str):
            raise NotImplementedError(f"per-key {name} key {key!r} is not present in this graph")
        else:
            raise TypeError(f"unsupported {name} option: {key}")
    return global_values, per_key


def _validated_key_style(name, value, supported=None):
    if supported is None:
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
    if "radius" in result:
        result["radius"] = _shared._ir._finite_number(f"{name}.radius", result["radius"])
        if result["radius"] <= 0:
            raise ValueError(f"{name}.radius must be positive")
    return result


def _apply_key_style(target, values):
    fill_color = values.get("fill_color", values.get("color"))
    stroke_color = values.get("stroke_color", values.get("color"))
    fill_opacity = values.get("fill_opacity")
    stroke_width = values.get("stroke_width")
    if fill_color is None and stroke_color is None and fill_opacity is None and stroke_width is None:
        return
    target.set_style(
        fill_color=fill_color,
        fill_opacity=fill_opacity,
        stroke_color=stroke_color,
        stroke_width=stroke_width,
    )


def _set_color(method, name, value):
    color = _compat._as_color(name, value)
    engine_call(method, color.red, color.green, color.blue, color.alpha,
                operation="Graph.style")


def _color_components(name, value):
    color = _compat._as_color(name, value)
    return color.red, color.green, color.blue, color.alpha


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


def _vertex_style_override_rows(vertices, keyed_config):
    missing = float("nan")
    rows = []
    for key in vertices:
        style = keyed_config.get(key, {})
        fill = style.get("fill_color", style.get("color"))
        stroke = style.get("stroke_color", style.get("color"))
        fill_components = (
            _color_components("vertex_config.fill_color", fill)
            if fill is not None else (missing,) * 4
        )
        stroke_components = (
            _color_components("vertex_config.stroke_color", stroke)
            if stroke is not None else (missing,) * 4
        )
        rows.extend((
            style.get("radius", missing),
            *fill_components,
            style.get("fill_opacity", missing),
            *stroke_components,
            style.get("stroke_width", missing),
        ))
    return rows


def _edge_style_override_rows(edges, keyed_config):
    missing = float("nan")
    rows = []
    for edge in edges:
        style = keyed_config.get(edge, {})
        color = style.get("stroke_color", style.get("color"))
        color_components = (
            _color_components("edge_config.color", color)
            if color is not None else (missing,) * 4
        )
        rows.extend((*color_components, style.get("stroke_width", missing)))
    return rows


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
    ``partitions``, ``align``, and ``center``; tree layouts accept
    ``root_vertex``, ``orientation``, and ``vertex_spacing``. Other options
    remain unsupported until their Rust semantics are qualified. Two-axis
    ``layout_scale`` tuples are supported for spring, random, circular, spiral,
    partite, and tree layouts; shell layout follows NetworkX's scalar-only
    scale contract. Partite layouts accept ``partitions`` directly or in
    ``layout_config``; the latter takes precedence. Constructor
    ``vertex_config`` and ``edge_config`` accept per-key color, fill, stroke,
    fill-opacity, and stroke-width overrides for vertices and edges present at
    construction time. ``add_vertices`` inherits constructor-wide vertex
    styles and accepts the same per-key style subset plus radius in its atomic
    mutation. ``add_edges`` inherits constructor-wide edge styles and accepts
    per-edge color, stroke-color, and stroke-width overrides. Constructor
    ``vertex_config`` also accepts per-key radius. Custom mobject types remain
    unsupported.
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
        root_vertex = kwargs.pop("root_vertex", None)
        partitions = kwargs.pop("partitions", None)
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
        tree = isinstance(layout, str) and layout == "tree"
        circular = isinstance(layout, str) and layout == "circular"
        shell = isinstance(layout, str) and layout == "shell"
        spiral = isinstance(layout, str) and layout == "spiral"
        partite = isinstance(layout, str) and layout == "partite"
        if root_vertex is not None and not tree:
            raise NotImplementedError("root_vertex is supported only by the tree layout")
        if partitions is not None and not partite:
            raise NotImplementedError("partitions are supported only by the partite layout")
        if (explicit is None and not spring and not random_layout and not tree
                and not circular and not shell and not spiral and not partite):
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
        self._graph_edges = [tuple(edge) for edge in edges]
        edge_ids = self._edge_ids(edges)
        global_vertex_config, keyed_vertex_config = _split_style_config(
            "vertex_config", vertex_config, _VERTEX_CONFIG_KEYS, set(self._graph_key_ids),
            per_key_supported=_VERTEX_CONFIG_KEYS,
        )
        self._default_vertex_config = dict(global_vertex_config)
        global_edge_config, keyed_edge_config = _split_style_config(
            "edge_config", edge_config, _EDGE_CONFIG_KEYS, set(edges)
        )
        self._default_edge_config = dict(global_edge_config)
        center = (0.0, 0.0)
        if circular:
            center = _circular_layout_center(layout_config, layout_center)
        shell_groups, rotate = ([], None)
        resolution, equidistant = (0.35, False)
        partite_groups, align_horizontal = ([], False)
        spring_positions, fixed_indices, spring_k = ([], [], None)
        random_positions = []
        tree_root, tree_orientation_up, tree_spacing = (None, False, None)
        scale_y = None
        spring_iterations, spring_threshold, spring_rescale = (50, 1e-4, True)
        spring_fixed_is_set = False
        if spring:
            (center, spring_positions, fixed_indices, spring_k,
             spring_iterations, spring_threshold, spring_rescale,
             scale, scale_y, spring_fixed_is_set) = _spring_layout_config(
                layout_config, layout_center, self._graph_key_ids, layout_scale
            )
        elif random_layout:
            center, random_positions, scale, scale_y = _random_layout_config(
                layout_config, layout_center, self._graph_key_ids, layout_scale
            )
        elif tree:
            tree_root, tree_orientation_up, tree_spacing, scale, scale_y = _tree_layout_config(
                layout_config, self._graph_key_ids, layout_scale, self._directed,
                root_vertex=root_vertex,
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
            center, partite_groups, align_horizontal = _partite_layout_config(
                layout_config, layout_center, self._graph_key_ids,
                partitions=partitions,
            )
        if not spring and not random_layout and not tree:
            if shell:
                if isinstance(layout_scale, Sequence) and not isinstance(layout_scale, (str, bytes)):
                    raise NotImplementedError("shell layout_scale must be a scalar")
                scale = _shared._ir._finite_number("layout_scale", layout_scale)
            else:
                scale, scale_y = _layout_scale_pair(layout_scale)
        context = _shared._live_constructor_context("Graph", allow_unstarted=True)
        if context is None:
            raise RuntimeError("Graph construction requires a canonical Scene authoring context")
        options = _graph_style_options(global_vertex_config, global_edge_config)
        ids = _array(self._graph_key_ids.values())
        try:
            for key, style in keyed_vertex_config.items():
                if "radius" in style:
                    engine_call(
                        options.setVertexRadiusFor,
                        self._graph_key_ids[key],
                        float(style["radius"]),
                        operation="Graph.style",
                    )
            if spring:
                self._graph_handle = engine_call(
                    context.liveCreateSpringGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(value for point in spring_positions for value in point),
                    _array(fixed_indices), spring_fixed_is_set,
                    scale, scale_y, *center, spring_k, spring_iterations, spring_threshold,
                    spring_rescale, options, operation="Graph.create",
                )
            elif random_layout:
                self._graph_handle = engine_call(
                    context.liveCreateRandomGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(value for point in random_positions for value in point),
                    scale, scale_y, options, operation="Graph.create",
                )
            elif tree:
                spacing_x, spacing_y = (None, None) if tree_spacing is None else tree_spacing
                self._graph_handle = engine_call(
                    context.liveCreateTreeGraph,
                    bool(self._directed), ids, _array(edge_ids), tree_root,
                    tree_orientation_up, spacing_x, spacing_y, scale, scale_y, options,
                    operation="Graph.create",
                )
            elif circular:
                self._graph_handle = engine_call(
                    context.liveCreateCircularGraph, bool(self._directed), ids, _array(edge_ids),
                    scale, scale_y, *center, options, operation="Graph.create",
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
                    bool(self._directed), ids, _array(edge_ids), scale, scale_y, *center,
                    resolution, equidistant, options, operation="Graph.create",
                )
            elif partite:
                partition_indices, partition_sizes = _flatten_groups(partite_groups)
                self._graph_handle = engine_call(
                    context.liveCreatePartiteGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(partition_indices), _array(partition_sizes),
                    scale, scale_y, *center, align_horizontal, options,
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
        self._edge_wrappers = {}
        self._edge_mapping = None
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
        cache_key = tuple(edge)
        if not self._directed and start > end:
            cache_key = (edge[1], edge[0])
        cached = self._edge_wrappers.get(cache_key)
        if cached is not None:
            return cached
        family = engine_call(self._graph_handle.edge, start, end, operation="Graph.edge")
        if family is None:
            raise ValueError(f"edge_config references unknown edge {edge!r}")
        target = object.__new__(_compat.Group)
        _shared._attach_shared_family(target, family, self._context())
        target._canonical_live_target_context = self._context()
        self._edge_wrappers[cache_key] = target
        return target

    def _refresh_semantic_members(self):
        _shared._attach_shared_family(self, self._semantic_family_handle, self._context())

    def _context(self):
        return self._canonical_live_target_context

    def __getitem__(self, key):
        """Resolve a vertex key or edge tuple to its retained semantic Mobject."""
        try:
            vertex_id = self._graph_key_ids.get(key)
        except TypeError:
            vertex_id = None
        if vertex_id is not None:
            handle = engine_call(self._graph_handle.vertex, vertex_id)
            if handle is None:
                raise ValueError(f"Could not find {key} in vertices or edges")
            identity = f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"
            member = self._semantic_member_wrappers.get(identity)
            if member is None:
                member = object.__new__(_compat.VMobject)
                _shared._attach_shared_handle(member, handle)
                member._canonical_live_target_context = self._context()
                self._semantic_member_wrappers[identity] = member
            return member
        if isinstance(key, tuple) and len(key) == 2:
            try:
                return self._edge_style_target(key)
            except ValueError:
                pass
        raise ValueError(f"Could not find {key} in vertices or edges")

    @property
    def vertices(self):
        return {key: self[key] for key in self._graph_key_ids}

    @property
    def edges(self):
        """Resolve graph edge keys to wrappers around their retained families."""
        if self._edge_mapping is not None:
            return self._edge_mapping
        adjacency = {key: [] for key in self._graph_key_ids}
        for start, end in self._graph_edges:
            adjacency[start].append(end)
            if not self._directed:
                adjacency[end].append(start)
        ordered = []
        seen_undirected = set()
        for start, neighbors in adjacency.items():
            for end in neighbors:
                if not self._directed:
                    undirected_key = frozenset((start, end))
                    if undirected_key in seen_undirected:
                        continue
                    seen_undirected.add(undirected_key)
                ordered.append((start, end))
        self._edge_mapping = {
            edge: self._edge_style_target(edge) for edge in ordered
        }
        return self._edge_mapping

    def add_vertices(self, *vertices, positions=None, vertex_config=None):
        """Append vertices, optionally supplying positions and per-key styles.

        ``vertex_config`` uses the constructor's global style keys and accepts
        per-key radius, color, fill-opacity, and stroke-width overrides. Global
        values extend constructor defaults; per-key values override those
        globals in the same graph publication. A mapping passed as the sole
        positional argument remains a shorthand for ``positions``.
        """
        if len(vertices) == 1 and isinstance(vertices[0], Mapping):
            if positions is not None:
                raise TypeError("positions cannot be combined with a vertex-to-position mapping")
            positions = vertices[0]
            vertices = tuple(positions)
        if positions is not None and not isinstance(positions, Mapping):
            raise TypeError("positions must be a mapping of vertex keys to positions")
        if not vertices:
            return []
        positions_by_key = positions if isinstance(positions, Mapping) else {}
        pending, ids, flat_positions = [], [], []
        next_id = self._next_graph_id
        center = _point(self.get_center())
        for key in vertices:
            if not isinstance(key, Hashable):
                raise TypeError("Graph vertex keys must be hashable")
            if key in self._graph_key_ids or any(key == item[0] for item in pending):
                raise ValueError("duplicate Graph vertex key")
            x, y = _point(positions_by_key.get(key, center))
            pending.append((key, x, y))
            ids.append(next_id)
            next_id += 1
            flat_positions.extend((x, y))
        pending_keys = {key for key, _, _ in pending}
        global_config, keyed_config = _split_style_config(
            "vertex_config", vertex_config, _VERTEX_CONFIG_KEYS, pending_keys,
            per_key_supported={
                "color", "fill_color", "stroke_color", "fill_opacity", "stroke_width", "radius",
            },
        )
        base_config = {**self._default_vertex_config, **global_config}
        options = _graph_style_options(base_config, {})
        try:
            style_overrides = _vertex_style_override_rows(
                (key for key, _, _ in pending), keyed_config
            )
            engine_call(
                self._context().liveGraphAddVerticesWithOptions,
                self._graph_handle,
                _array(ids),
                _array(flat_positions),
                _array(style_overrides),
                options,
                operation="Graph.add_vertices",
            )
        except BaseException:
            options.free()
            raise
        self._graph_key_ids.update((key, vertex_id) for (key, _, _), vertex_id in zip(pending, ids))
        self._next_graph_id = next_id
        return [self[key] for key, _, _ in pending]

    def add_edges(self, edges, *, edge_config=None, positions=None, vertex_config=None):
        """Add edges and create any missing endpoint vertices atomically.

        Missing vertices default to the current graph center. ``positions`` and
        ``vertex_config`` customize those vertices; edge styles continue to
        inherit graph defaults and accept per-edge overrides.
        """
        edges = [tuple(edge) for edge in edges]
        if any(len(edge) != 2 for edge in edges):
            raise ValueError("Graph edges must contain two vertex keys")
        if positions is not None and not isinstance(positions, Mapping):
            raise TypeError("positions must be a mapping of vertex keys to positions")
        new_vertices, seen_new_vertices = [], set()
        try:
            for key in (key for edge in edges for key in edge):
                if key not in self._graph_key_ids and key not in seen_new_vertices:
                    seen_new_vertices.add(key)
                    new_vertices.append(key)
        except TypeError as error:
            raise TypeError("Graph vertex keys must be hashable") from error
        next_id = self._next_graph_id
        vertex_ids = list(range(next_id, next_id + len(new_vertices)))
        ids_by_key = dict(self._graph_key_ids)
        ids_by_key.update(zip(new_vertices, vertex_ids))
        edge_ids = [
            vertex_id
            for start, end in edges
            for vertex_id in (ids_by_key[start], ids_by_key[end])
        ]

        if new_vertices:
            center = _point(self.get_center())
            vertex_positions = [
                _point(positions.get(key, center) if positions is not None else center)
                for key in new_vertices
            ]
        else:
            vertex_positions = []

        global_vertex_config, keyed_vertex_config = _split_style_config(
            "vertex_config", vertex_config, _VERTEX_CONFIG_KEYS, set(new_vertices),
            per_key_supported={
                "color", "fill_color", "stroke_color", "fill_opacity", "stroke_width", "radius",
            },
        )
        base_vertex_config = {**self._default_vertex_config, **global_vertex_config}
        vertex_style_overrides = _vertex_style_override_rows(
            new_vertices, keyed_vertex_config
        )

        global_edge_config, keyed_edge_config = _split_style_config(
            "edge_config", edge_config, _EDGE_CONFIG_KEYS, set(edges),
            per_key_supported=_EDGE_CONFIG_KEYS,
        )
        default_edge_config = _validated_key_style(
            "edge_config",
            {**self._default_edge_config, **global_edge_config},
            _EDGE_CONFIG_KEYS,
        )
        edge_style_overrides = _edge_style_override_rows(edges, keyed_edge_config)

        vertex_options = (
            _graph_style_options(base_vertex_config, {}) if new_vertices else None
        )
        try:
            edge_options = _graph_style_options(self._default_vertex_config, default_edge_config)
        except BaseException:
            if vertex_options is not None:
                vertex_options.free()
            raise

        try:
            if new_vertices:
                engine_call(
                    self._context().liveGraphAddVerticesAndEdgesWithOptions,
                    self._graph_handle,
                    _array(vertex_ids),
                    _array(value for point in vertex_positions for value in point),
                    _array(vertex_style_overrides),
                    vertex_options,
                    _array(edge_ids),
                    _array(edge_style_overrides),
                    edge_options,
                    operation="Graph.add_edges",
                )
            else:
                engine_call(
                    self._context().liveGraphAddEdgesWithOptions,
                    self._graph_handle,
                    _array(edge_ids),
                    _array(edge_style_overrides),
                    edge_options,
                    operation="Graph.add_edges",
                )
        except BaseException:
            if vertex_options is not None:
                vertex_options.free()
            edge_options.free()
            raise
        self._graph_key_ids.update(zip(new_vertices, vertex_ids))
        self._next_graph_id = next_id + len(new_vertices)
        self._graph_edges.extend(edges)
        self._edge_mapping = None
        return self

    def remove_vertices(self, vertices):
        keys = list(vertices)
        try:
            ids = [self._graph_key_ids[key] for key in keys]
        except KeyError as error:
            raise ValueError("unknown Graph vertex key") from error
        engine_call(self._context().liveGraphRemoveVertices, self._graph_handle,
                    _array(ids), operation="Graph.remove_vertices")
        removed = set(keys)
        for key in keys:
            del self._graph_key_ids[key]
        self._graph_edges = [
            edge for edge in self._graph_edges
            if edge[0] not in removed and edge[1] not in removed
        ]
        self._edge_wrappers = {
            edge: wrapper for edge, wrapper in self._edge_wrappers.items()
            if edge[0] not in removed and edge[1] not in removed
        }
        self._edge_mapping = None
        return self

    def remove_edges(self, edges):
        edges = [tuple(edge) for edge in edges]
        engine_call(self._context().liveGraphRemoveEdges, self._graph_handle,
                    _array(self._edge_ids(edges)), operation="Graph.remove_edges")
        removed = set(edges)
        if not self._directed:
            removed.update((end, start) for start, end in edges)
        self._graph_edges = [edge for edge in self._graph_edges if edge not in removed]
        for start, end in edges:
            cache_key = (start, end)
            if not self._directed and self._graph_key_ids[start] > self._graph_key_ids[end]:
                cache_key = (end, start)
            self._edge_wrappers.pop(cache_key, None)
        self._edge_mapping = None
        return self

    def copy(self):
        """Copy the retained family, including appearance and endpoint bindings."""
        clone = object.__new__(type(self))
        clone._directed = self._directed
        clone._graph_key_ids = dict(self._graph_key_ids)
        clone._graph_edges = list(self._graph_edges)
        clone._edge_wrappers = {}
        clone._edge_mapping = None
        clone._next_graph_id = self._next_graph_id
        clone._default_vertex_config = dict(self._default_vertex_config)
        clone._default_edge_config = dict(self._default_edge_config)
        clone._canonical_live_target_context = self._context()
        clone._graph_handle = engine_call(
            clone._canonical_live_target_context.liveGraphCopy,
            self._graph_handle,
            operation="Graph.copy",
        )
        _shared._attach_shared_family(clone, engine_call(clone._graph_handle.family), self._context())
        return clone

    def change_layout(self, layout="spring", *, scale=2.0, center=(0.0, 0.0),
                      positions=None, layout_config=None, root_vertex=None,
                      partitions=None):
        if isinstance(layout, Mapping):
            positions, layout = layout, "explicit"
        if root_vertex is not None and layout != "tree":
            raise NotImplementedError("root_vertex is supported only by the tree layout")
        if partitions is not None and layout != "partite":
            raise NotImplementedError("partitions are supported only by the partite layout")
        if layout == "spring":
            (center, initial_positions, fixed_indices, spring_k,
             iterations, threshold, rescale, scale, scale_y, fixed_is_set) = _spring_layout_config(
                layout_config, center, self._graph_key_ids, scale
            )
            engine_call(
                self._context().liveGraphSpringLayout,
                self._graph_handle,
                _array(value for point in initial_positions for value in point),
                _array(fixed_indices), fixed_is_set,
                scale, scale_y, *center, spring_k, iterations, threshold, rescale,
                operation="Graph.spring_layout",
            )
        elif layout == "random":
            _, random_positions, scale, scale_y = _random_layout_config(
                layout_config, center, self._graph_key_ids, scale
            )
            engine_call(
                self._context().liveGraphRandomLayout,
                self._graph_handle,
                _array(value for point in random_positions for value in point),
                scale, scale_y,
                operation="Graph.random_layout",
            )
        elif layout == "tree":
            vertex_order = {key: index for index, key in enumerate(self._graph_key_ids)}
            root_index, orientation_up, spacing, scale, scale_y = _tree_layout_config(
                layout_config, vertex_order, scale, self._directed,
                root_vertex=root_vertex,
            )
            spacing_x, spacing_y = (None, None) if spacing is None else spacing
            engine_call(
                self._context().liveGraphTreeLayout,
                self._graph_handle,
                root_index, orientation_up, spacing_x, spacing_y, scale, scale_y,
                operation="Graph.tree_layout",
            )
        elif layout == "circular":
            center = _circular_layout_center(layout_config, center)
            scale_x, scale_y = _layout_scale_pair(scale)
            engine_call(self._context().liveGraphCircularLayout, self._graph_handle,
                        scale_x, scale_y, *_point(center), operation="Graph.circular_layout")
        elif layout == "shell":
            vertex_order = {key: index for index, key in enumerate(self._graph_key_ids)}
            center, groups, rotate = _shell_layout_config(
                layout_config, center, vertex_order
            )
            if isinstance(scale, Sequence) and not isinstance(scale, (str, bytes)):
                raise NotImplementedError("shell layout_scale must be a scalar")
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
            scale_x, scale_y = _layout_scale_pair(scale)
            engine_call(
                self._context().liveGraphSpiralLayout,
                self._graph_handle,
                scale_x, scale_y, *center,
                resolution, equidistant,
                operation="Graph.spiral_layout",
            )
        elif layout == "partite":
            center, partitions, align_horizontal = _partite_layout_config(
                layout_config, center, self._graph_key_ids,
                partitions=partitions,
            )
            partition_indices, partition_sizes = _flatten_groups(partitions)
            scale_x, scale_y = _layout_scale_pair(scale)
            engine_call(
                self._context().liveGraphPartiteLayout,
                self._graph_handle,
                _array(partition_indices), _array(partition_sizes),
                scale_x, scale_y, *center,
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
                "Python Graph qualifies only spring, random, tree, circular, shell, spiral, partite, and explicit layouts"
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
        scale, scale_y, rescale = 1.0, None, False
    else:
        scale, scale_y = _layout_scale_pair(layout_scale)
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
        rescale, scale, scale_y, fixed_is_set,
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
    scale, scale_y = _layout_scale_pair(layout_scale)

    np, rng = _numpy_random_source(options.get("seed"), "random")
    positions = (_numpy_random_array(rng, (len(vertex_ids), 2)) + np.asarray(center)).astype(np.float32)
    prepared = [tuple(map(float, point)) for point in positions]
    return center, prepared, scale, scale_y


def _tree_layout_config(layout_config, vertex_ids, layout_scale, directed, *, root_vertex=None):
    """Resolve Manim's tree-layout root, direction, and spacing options."""
    if directed:
        raise NotImplementedError("ManimCE tree layout requires an undirected Graph")
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"root_vertex", "orientation", "vertex_spacing"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported tree layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    if root_vertex is not None and "root_vertex" not in options:
        options["root_vertex"] = root_vertex
    root = options.get("root_vertex")
    if root is None:
        raise ValueError("tree layout requires root_vertex")
    try:
        root_index = vertex_ids[root]
    except (KeyError, TypeError) as error:
        raise ValueError("tree root_vertex must be an existing graph vertex") from error
    orientation = options.get("orientation", "down")
    if not isinstance(orientation, str):
        raise TypeError("tree layout_config orientation must be 'down' or 'up'")
    if orientation not in {"down", "up"}:
        raise ValueError("tree layout_config orientation must be 'down' or 'up'")
    spacing = options.get("vertex_spacing")
    if spacing is not None:
        if isinstance(spacing, (str, bytes)) or not isinstance(spacing, Sequence) or len(spacing) != 2:
            raise TypeError("tree layout_config vertex_spacing must contain x and y values")
        spacing = (
            _shared._ir._finite_number("layout_config.vertex_spacing[0]", spacing[0]),
            _shared._ir._finite_number("layout_config.vertex_spacing[1]", spacing[1]),
        )
    scale, scale_y = _layout_scale_pair(layout_scale)
    return root_index, orientation == "up", spacing, scale, scale_y


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


def _partite_layout_config(layout_config, fallback_center, vertex_ids, *, partitions=None):
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
    specifications = options.get("partitions", partitions)
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
    """Undirected retained Graph with qualified spring, random, tree, circular, shell, spiral, partite, and explicit layouts."""


class DiGraph(_GraphBase):
    """Directed retained Graph with shared Rust spring/random layouts and Arrow semantics."""
    _directed = True


__all__ = ["Graph", "DiGraph"]
