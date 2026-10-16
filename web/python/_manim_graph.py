"""Thin Graph/DiGraph adapters over the shared Rust authoring handle.

Python owns only its arbitrary hashable-key to u32 mapping. Rust owns graph
topology, arrow endpoints, persistent placements, and publication.
"""

from __future__ import annotations

from collections.abc import Hashable, Mapping

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
    fill_color = values.get("fill_color")
    stroke_color = values.get("stroke_color")
    if "color" in values:
        fill_color = stroke_color = values["color"]
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

    Circular layouts accept NetworkX's ``center`` and ``dim=2`` configuration
    through ``layout_config``. Other circular options and dimensions above two
    remain unsupported until their Rust semantics are qualified. Constructor
    ``vertex_config`` and ``edge_config`` accept per-key color, fill, stroke,
    fill-opacity, and stroke-width overrides for vertices and edges present at
    construction time; other per-key constructor options remain unsupported.
    """

    # This typed vector wrapper retains the shared family-paint contract.
    set_opacity = _compat.VGroup.set_opacity
    set_fill = _compat.VGroup.set_fill
    set_stroke = _compat.VGroup.set_stroke

    _directed = False

    def __init__(self, vertices, edges, layout="circular", *, layout_scale=2.0,
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
        circular = isinstance(layout, str) and layout == "circular"
        if explicit is None and not circular:
            raise NotImplementedError("Graph requires a circular layout or a complete position mapping")
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
        center = (_circular_layout_center(layout_config, layout_center)
                  if circular else _point(layout_center))
        scale = _shared._ir._finite_number("layout_scale", layout_scale)
        context = _shared._live_constructor_context("Graph", allow_unstarted=True)
        if context is None:
            raise RuntimeError("Graph construction requires a canonical Scene authoring context")
        options = _graph_style_options(global_vertex_config, global_edge_config)
        ids = _array(self._graph_key_ids.values())
        try:
            if circular:
                self._graph_handle = engine_call(
                    context.liveCreateCircularGraph, bool(self._directed), ids, _array(edge_ids),
                    scale, *center, options, operation="Graph.create",
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

    def change_layout(self, layout="circular", *, scale=2.0, center=(0.0, 0.0),
                      positions=None, layout_config=None):
        if isinstance(layout, Mapping):
            positions, layout = layout, "explicit"
        if layout == "circular":
            center = _circular_layout_center(layout_config, center)
            engine_call(self._context().liveGraphCircularLayout, self._graph_handle,
                        float(scale), *_point(center), operation="Graph.circular_layout")
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
            raise NotImplementedError("Python Graph qualifies only circular and explicit layouts")
        return self


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


class Graph(_GraphBase):
    """Undirected retained Graph with qualified circular/explicit layouts."""


class DiGraph(_GraphBase):
    """Directed retained Graph using Rust's shared Arrow endpoint semantics."""
    _directed = True


__all__ = ["Graph", "DiGraph"]
