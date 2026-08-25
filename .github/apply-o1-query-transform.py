from pathlib import Path


def replace_once(text: str, before: str, after: str, label: str) -> str:
    count = text.count(before)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(before, after, 1)


compat = Path("web/python/_manim_compat.py")
text = compat.read_text()
query_block = r'''
def _mobject_get_critical_point(
    self: _BaseMobject, direction: object
) -> _base.Vec2:
    return _critical_for(self, _as_vec2(direction))


def _mobject_get_edge_center(self: _BaseMobject, direction: object) -> _base.Vec2:
    return self.get_critical_point(direction)


def _mobject_get_corner(self: _BaseMobject, direction: object) -> _base.Vec2:
    return self.get_critical_point(direction)


def _mobject_get_left(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.LEFT)


def _mobject_get_right(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.RIGHT)


def _mobject_get_top(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.UP)


def _mobject_get_bottom(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.DOWN)


def _mobject_get_coord(
    self: _BaseMobject, dim: int, direction: object = _base.ORIGIN
) -> float:
    if dim not in (0, 1):
        raise NotImplementedError("Noon currently exposes x/y authoring coordinates only")
    point = self.get_critical_point(direction)
    return float(point[dim])


def _mobject_get_x(self: _BaseMobject, direction: object = _base.ORIGIN) -> float:
    return self.get_coord(0, direction)


def _mobject_get_y(self: _BaseMobject, direction: object = _base.ORIGIN) -> float:
    return self.get_coord(1, direction)


def _mobject_set_coord(
    self: _BaseMobject,
    value: float,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    if dim not in (0, 1):
        raise NotImplementedError("Noon currently exposes x/y authoring coordinates only")
    delta = float(value) - self.get_coord(dim, direction)
    return self.shift(_base.Vec2(delta, 0.0) if dim == 0 else _base.Vec2(0.0, delta))


def _mobject_set_x(
    self: _BaseMobject, x: float, direction: object = _base.ORIGIN
) -> _BaseMobject:
    return self.set_coord(x, 0, direction)


def _mobject_set_y(
    self: _BaseMobject, y: float, direction: object = _base.ORIGIN
) -> _BaseMobject:
    return self.set_coord(y, 1, direction)


def _mobject_rescale_to_fit(
    self: _BaseMobject,
    length: float,
    dim: int,
    stretch: bool = False,
    **kwargs: Any,
) -> _BaseMobject:
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(
            f"rescale_to_fit anchor option(s) are not yet supported: {unsupported}"
        )
    if dim not in (0, 1):
        raise NotImplementedError("Noon currently exposes width/height fitting only")
    old_length = self.width if dim == 0 else self.height
    if old_length == 0.0:
        return self
    factor = float(length) / old_length
    if stretch:
        return self.scale((factor, 1.0) if dim == 0 else (1.0, factor))
    return self.scale(factor)


def _mobject_scale_to_fit_width(self: _BaseMobject, width: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(width, 0, stretch=False, **kwargs)


def _mobject_scale_to_fit_height(self: _BaseMobject, height: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(height, 1, stretch=False, **kwargs)


def _mobject_stretch_to_fit_width(self: _BaseMobject, width: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(width, 0, stretch=True, **kwargs)


def _mobject_stretch_to_fit_height(self: _BaseMobject, height: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(height, 1, stretch=True, **kwargs)


def _mobject_match_dim_size(
    self: _BaseMobject, mobject: _BaseMobject, dim: int, **kwargs: Any
) -> _BaseMobject:
    if not isinstance(mobject, _BaseMobject):
        raise TypeError("dimension match target must be a Mobject")
    if dim == 0:
        length = mobject.width
    elif dim == 1:
        length = mobject.height
    else:
        raise NotImplementedError("Noon currently exposes width/height matching only")
    return self.rescale_to_fit(length, dim, **kwargs)


def _mobject_match_width(
    self: _BaseMobject, mobject: _BaseMobject, **kwargs: Any
) -> _BaseMobject:
    return self.match_dim_size(mobject, 0, **kwargs)


def _mobject_match_height(
    self: _BaseMobject, mobject: _BaseMobject, **kwargs: Any
) -> _BaseMobject:
    return self.match_dim_size(mobject, 1, **kwargs)


def _mobject_match_coord(
    self: _BaseMobject,
    mobject: _BaseMobject,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    if not isinstance(mobject, _BaseMobject):
        raise TypeError("coordinate match target must be a Mobject")
    return self.set_coord(mobject.get_coord(dim, direction), dim, direction)


def _mobject_match_x(
    self: _BaseMobject,
    mobject: _BaseMobject,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    return self.match_coord(mobject, 0, direction)


def _mobject_match_y(
    self: _BaseMobject,
    mobject: _BaseMobject,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    return self.match_coord(mobject, 1, direction)


def _mobject_rotate_about_origin(
    self: _BaseMobject, angle: float, axis: object = None
) -> _BaseMobject:
    if axis is not None:
        try:
            if len(axis) != 3:  # type: ignore[arg-type]
                raise TypeError
            x = float(axis[0])  # type: ignore[index]
            y = float(axis[1])  # type: ignore[index]
            z = float(axis[2])  # type: ignore[index]
        except (TypeError, ValueError, IndexError) as error:
            raise TypeError("rotation axis must be a three-component vector") from error
        if not math.isclose(x, 0.0, abs_tol=1e-12) or not math.isclose(
            y, 0.0, abs_tol=1e-12
        ) or not math.isclose(abs(z), 1.0, abs_tol=1e-12):
            raise NotImplementedError("2D authoring supports rotation about the z axis only")
        if z < 0.0:
            angle = -float(angle)
    angle = float(angle)
    center = self.get_center()
    cosine = math.cos(angle)
    sine = math.sin(angle)
    target = _base.Vec2(
        center.x * cosine - center.y * sine,
        center.x * sine + center.y * cosine,
    )
    self.rotate(angle)
    return self.move_to(target)


def _set_width_property(self: _BaseMobject, width: float) -> None:
    self.scale_to_fit_width(float(width))


def _set_height_property(self: _BaseMobject, height: float) -> None:
    self.scale_to_fit_height(float(height))


'''
text = replace_once(text, "def _state_target(", query_block + "def _state_target(", "query block")
install_anchor = '''    _BaseMobject.animate = property(lambda self: _CompatAnimationBuilder(self))
    _BaseMobject.generate_target = _mobject_generate_target
'''
install_replacement = '''    _BaseMobject.animate = property(lambda self: _CompatAnimationBuilder(self))
    _BaseMobject.get_critical_point = _mobject_get_critical_point
    _BaseMobject.get_edge_center = _mobject_get_edge_center
    _BaseMobject.get_corner = _mobject_get_corner
    _BaseMobject.get_left = _mobject_get_left
    _BaseMobject.get_right = _mobject_get_right
    _BaseMobject.get_top = _mobject_get_top
    _BaseMobject.get_bottom = _mobject_get_bottom
    _BaseMobject.get_coord = _mobject_get_coord
    _BaseMobject.get_x = _mobject_get_x
    _BaseMobject.get_y = _mobject_get_y
    _BaseMobject.set_coord = _mobject_set_coord
    _BaseMobject.set_x = _mobject_set_x
    _BaseMobject.set_y = _mobject_set_y
    _BaseMobject.rescale_to_fit = _mobject_rescale_to_fit
    _BaseMobject.scale_to_fit_width = _mobject_scale_to_fit_width
    _BaseMobject.scale_to_fit_height = _mobject_scale_to_fit_height
    _BaseMobject.stretch_to_fit_width = _mobject_stretch_to_fit_width
    _BaseMobject.stretch_to_fit_height = _mobject_stretch_to_fit_height
    _BaseMobject.match_dim_size = _mobject_match_dim_size
    _BaseMobject.match_width = _mobject_match_width
    _BaseMobject.match_height = _mobject_match_height
    _BaseMobject.match_coord = _mobject_match_coord
    _BaseMobject.match_x = _mobject_match_x
    _BaseMobject.match_y = _mobject_match_y
    _BaseMobject.rotate_about_origin = _mobject_rotate_about_origin
    _BaseMobject.width = property(_BaseMobject.width.fget, _set_width_property)
    _BaseMobject.height = property(_BaseMobject.height.fget, _set_height_property)
    _BaseMobject.generate_target = _mobject_generate_target
'''
text = replace_once(text, install_anchor, install_replacement, "compat install")
compat.write_text(text)


handles = Path("web/python/_manim_semantic_handles.py")
text = handles.read_text()
setter_block = '''\n\ndef _set_width_property(self: _base.Mobject, width: float) -> None:\n    self.scale_to_fit_width(float(width))\n\n\ndef _set_height_property(self: _base.Mobject, height: float) -> None:\n    self.scale_to_fit_height(float(height))\n'''
text = replace_once(
    text,
    '''def _height(self: _base.Mobject) -> float:\n    handle = _handle_for(self)\n    if handle is not None:\n        return float(handle.height)\n    bounds = _base._bounds(self._current_raw())\n    return 0.0 if bounds is None else bounds[1].y - bounds[0].y\n''',
    '''def _height(self: _base.Mobject) -> float:\n    handle = _handle_for(self)\n    if handle is not None:\n        return float(handle.height)\n    bounds = _base._bounds(self._current_raw())\n    return 0.0 if bounds is None else bounds[1].y - bounds[0].y\n''' + setter_block,
    "semantic property setters",
)
text = replace_once(
    text,
    '''    _base.Mobject.width = property(_width)\n    _base.Mobject.height = property(_height)\n''',
    '''    _base.Mobject.width = property(_width, _set_width_property)\n    _base.Mobject.height = property(_height, _set_height_property)\n''',
    "semantic property install",
)
handles.write_text(text)


diff = Path("scripts/manim-differential.py")
text = diff.read_text()
text = replace_once(
    text,
    '''def _members_observation(group: Any) -> dict[str, Any]:\n''',
    '''def _point_observation(point: Any) -> list[float]:\n    return [_round_float(point[0]), _round_float(point[1])]\n\n\ndef _members_observation(group: Any) -> dict[str, Any]:\n''',
    "point observation",
)
fixture_block = r'''
def _noon_critical_points() -> Any:
    obj = noon.Rectangle(width=2.0, height=1.0).shift(noon.RIGHT * 0.7 + noon.UP * 0.3)
    return {
        "left": _point_observation(obj.get_left()),
        "right": _point_observation(obj.get_right()),
        "top": _point_observation(obj.get_top()),
        "bottom": _point_observation(obj.get_bottom()),
        "corner": _point_observation(obj.get_corner(noon.UR)),
        "x_left": _round_float(obj.get_x(noon.LEFT)),
        "y_top": _round_float(obj.get_y(noon.UP)),
    }


def _manim_critical_points() -> Any:
    obj = manim.Rectangle(width=2.0, height=1.0).shift(manim.RIGHT * 0.7 + manim.UP * 0.3)
    return {
        "left": _point_observation(obj.get_left()),
        "right": _point_observation(obj.get_right()),
        "top": _point_observation(obj.get_top()),
        "bottom": _point_observation(obj.get_bottom()),
        "corner": _point_observation(obj.get_corner(manim.UR)),
        "x_left": _round_float(obj.get_x(manim.LEFT)),
        "y_top": _round_float(obj.get_y(manim.UP)),
    }


def _noon_set_coord_direction() -> Any:
    obj = noon.Square(side_length=1.0).shift(noon.RIGHT * 0.25)
    obj.set_coord(-1.5, 0, noon.LEFT).set_coord(1.25, 1, noon.UP)
    return {"object": _object_observation(obj), "left": _point_observation(obj.get_left()), "top": _point_observation(obj.get_top())}


def _manim_set_coord_direction() -> Any:
    obj = manim.Square(side_length=1.0).shift(manim.RIGHT * 0.25)
    obj.set_coord(-1.5, 0, manim.LEFT).set_coord(1.25, 1, manim.UP)
    return {"object": _object_observation(obj), "left": _point_observation(obj.get_left()), "top": _point_observation(obj.get_top())}


def _noon_scale_to_fit_width() -> Any:
    return _object_observation(noon.Rectangle(width=2.0, height=1.0).scale_to_fit_width(3.0))


def _manim_scale_to_fit_width() -> Any:
    return _object_observation(manim.Rectangle(width=2.0, height=1.0).scale_to_fit_width(3.0))


def _noon_stretch_to_fit_height() -> Any:
    return _object_observation(noon.Rectangle(width=2.0, height=1.0).stretch_to_fit_height(2.5))


def _manim_stretch_to_fit_height() -> Any:
    return _object_observation(manim.Rectangle(width=2.0, height=1.0).stretch_to_fit_height(2.5))


def _noon_match_xy() -> Any:
    target = noon.Rectangle(width=1.5, height=0.8).shift(noon.RIGHT * 1.2 + noon.DOWN * 0.6)
    obj = noon.Circle(radius=0.3).match_x(target, noon.RIGHT).match_y(target, noon.DOWN)
    return {"target": _object_observation(target), "object": _object_observation(obj), "right": _point_observation(obj.get_right()), "bottom": _point_observation(obj.get_bottom())}


def _manim_match_xy() -> Any:
    target = manim.Rectangle(width=1.5, height=0.8).shift(manim.RIGHT * 1.2 + manim.DOWN * 0.6)
    obj = manim.Circle(radius=0.3).match_x(target, manim.RIGHT).match_y(target, manim.DOWN)
    return {"target": _object_observation(target), "object": _object_observation(obj), "right": _point_observation(obj.get_right()), "bottom": _point_observation(obj.get_bottom())}


def _noon_match_width() -> Any:
    target = noon.Rectangle(width=2.4, height=0.6)
    return _object_observation(noon.Circle(radius=0.4).match_width(target))


def _manim_match_width() -> Any:
    target = manim.Rectangle(width=2.4, height=0.6)
    return _object_observation(manim.Circle(radius=0.4).match_width(target))


def _noon_match_height_stretch() -> Any:
    target = noon.Rectangle(width=0.5, height=1.8)
    return _object_observation(noon.Rectangle(width=1.4, height=0.7).match_height(target, stretch=True))


def _manim_match_height_stretch() -> Any:
    target = manim.Rectangle(width=0.5, height=1.8)
    return _object_observation(manim.Rectangle(width=1.4, height=0.7).match_height(target, stretch=True))


def _noon_dimension_properties() -> Any:
    obj = noon.Rectangle(width=2.0, height=1.0)
    obj.width = 3.0
    obj.height = 1.5
    return _object_observation(obj)


def _manim_dimension_properties() -> Any:
    obj = manim.Rectangle(width=2.0, height=1.0)
    obj.width = 3.0
    obj.height = 1.5
    return _object_observation(obj)


def _noon_rotate_about_origin() -> Any:
    obj = noon.Rectangle(width=1.2, height=0.6).shift(noon.RIGHT * 1.5 + noon.UP * 0.5)
    obj.rotate_about_origin(math.pi / 2.0)
    return _object_observation(obj)


def _manim_rotate_about_origin() -> Any:
    obj = manim.Rectangle(width=1.2, height=0.6).shift(manim.RIGHT * 1.5 + manim.UP * 0.5)
    obj.rotate_about_origin(math.pi / 2.0)
    return _object_observation(obj)


'''
text = replace_once(text, "FIXTURES = [", fixture_block + "FIXTURES = [", "differential fixture block")
text = replace_once(
    text,
    '''    Fixture("replace_stretch", _noon_replace_stretch, _manim_replace_stretch),\n]''',
    '''    Fixture("replace_stretch", _noon_replace_stretch, _manim_replace_stretch),\n    Fixture("critical_points", _noon_critical_points, _manim_critical_points),\n    Fixture("set_coord_direction", _noon_set_coord_direction, _manim_set_coord_direction),\n    Fixture("scale_to_fit_width", _noon_scale_to_fit_width, _manim_scale_to_fit_width),\n    Fixture("stretch_to_fit_height", _noon_stretch_to_fit_height, _manim_stretch_to_fit_height),\n    Fixture("match_xy", _noon_match_xy, _manim_match_xy),\n    Fixture("match_width", _noon_match_width, _manim_match_width),\n    Fixture("match_height_stretch", _noon_match_height_stretch, _manim_match_height_stretch),\n    Fixture("dimension_properties", _noon_dimension_properties, _manim_dimension_properties),\n    Fixture("rotate_about_origin", _noon_rotate_about_origin, _manim_rotate_about_origin),\n]''',
    "fixture registration",
)
diff.write_text(text)


smoke = Path("scripts/manim-compat-smoke.mjs")
text = smoke.read_text()
source_block = r'''
const queryTransformSource = `
from noon import *

class SharedQueryTransforms(Scene):
    def construct(self):
        box = Rectangle(width=2.0, height=1.0).shift(RIGHT * 0.7 + UP * 0.3)
        assert abs(box.get_left().x + 0.3) < 1e-9
        assert abs(box.get_right().x - 1.7) < 1e-9
        assert abs(box.get_top().y - 0.8) < 1e-9
        assert abs(box.get_x(LEFT) + 0.3) < 1e-9

        box.set_coord(-1.5, 0, LEFT).set_coord(1.25, 1, UP)
        assert abs(box.get_left().x + 1.5) < 1e-9
        assert abs(box.get_top().y - 1.25) < 1e-9
        box.width = 3.0
        box.stretch_to_fit_height(2.0)
        assert abs(box.width - 3.0) < 1e-9
        assert abs(box.height - 2.0) < 1e-9

        target = Circle(radius=0.4).shift(RIGHT * 1.2 + DOWN * 0.4)
        box.match_x(target).match_y(target)
        assert abs(box.get_x() - target.get_x()) < 1e-9
        assert abs(box.get_y() - target.get_y()) < 1e-9

        orbit = Square(side_length=0.5).shift(RIGHT * 1.5 + UP * 0.5)
        orbit.rotate_about_origin(PI / 2)
        assert abs(orbit.get_x() + 0.5) < 1e-9
        assert abs(orbit.get_y() - 1.5) < 1e-9
        self.add(box, target, orbit)
`;

'''
text = replace_once(text, "const rateFunctionSource = `", source_block + "const rateFunctionSource = `", "browser query source")
eval_block = r'''
  const queryTransforms = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    queryTransformSource,
  );
  assert.equal(queryTransforms.kind, "scene_document");
  assert.equal(queryTransforms.document.objects.length, 3);

'''
text = replace_once(
    text,
    '''  const sharedRates = await page.evaluate(\n''',
    eval_block + '''  const sharedRates = await page.evaluate(\n''',
    "browser query evaluation",
)
text = replace_once(
    text,
    '''independent fill/stroke opacity, z=0 vectors, and shared deterministic Manim rate-function lowering.''',
    '''independent fill/stroke opacity, shared detached query/dimension transforms, z=0 vectors, and shared deterministic Manim rate-function lowering.''',
    "browser success message",
)
smoke.write_text(text)
