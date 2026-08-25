from pathlib import Path

path = Path("scripts/manim-differential.py")
source = path.read_text()
anchor = "\n\nFIXTURES = [\n"
if anchor not in source:
    raise SystemExit("fixture anchor not found")

probes = r'''

def _noon_set_xy() -> Any:
    obj = noon.Circle(radius=0.4).set_x(1.25).set_y(-0.75)
    return _object_observation(obj)


def _manim_set_xy() -> Any:
    obj = manim.Circle(radius=0.4).set_x(1.25).set_y(-0.75)
    return _object_observation(obj)


def _noon_next_to_point() -> Any:
    point = noon.RIGHT * 1.5 + noon.UP * 0.2
    obj = noon.Square(side_length=0.6).next_to(point, noon.UP, buff=0.15)
    return _object_observation(obj)


def _manim_next_to_point() -> Any:
    point = manim.RIGHT * 1.5 + manim.UP * 0.2
    obj = manim.Square(side_length=0.6).next_to(point, manim.UP, buff=0.15)
    return _object_observation(obj)


def _noon_vgroup_add_remove() -> Any:
    first = noon.Circle(radius=0.25).shift(noon.LEFT)
    removed = noon.Square(side_length=0.5)
    last = noon.Rectangle(width=0.8, height=0.4).shift(noon.RIGHT)
    group = noon.VGroup(first, removed).add(last).remove(removed)
    return _members_observation(group)


def _manim_vgroup_add_remove() -> Any:
    first = manim.Circle(radius=0.25).shift(manim.LEFT)
    removed = manim.Square(side_length=0.5)
    last = manim.Rectangle(width=0.8, height=0.4).shift(manim.RIGHT)
    group = manim.VGroup(first, removed).add(last).remove(removed)
    return _members_observation(group)


def _noon_vgroup_shift() -> Any:
    group = noon.VGroup(
        noon.Circle(radius=0.25).shift(noon.LEFT),
        noon.Square(side_length=0.5).shift(noon.RIGHT),
    ).shift(noon.UP * 0.75 + noon.LEFT * 0.2)
    return _members_observation(group)


def _manim_vgroup_shift() -> Any:
    group = manim.VGroup(
        manim.Circle(radius=0.25).shift(manim.LEFT),
        manim.Square(side_length=0.5).shift(manim.RIGHT),
    ).shift(manim.UP * 0.75 + manim.LEFT * 0.2)
    return _members_observation(group)


def _noon_mobject_copy_independence() -> Any:
    source = noon.Rectangle(width=1.2, height=0.6).shift(noon.LEFT * 0.8)
    clone = source.copy().shift(noon.RIGHT * 2.0)
    return {"source": _object_observation(source), "clone": _object_observation(clone)}


def _manim_mobject_copy_independence() -> Any:
    source = manim.Rectangle(width=1.2, height=0.6).shift(manim.LEFT * 0.8)
    clone = source.copy().shift(manim.RIGHT * 2.0)
    return {"source": _object_observation(source), "clone": _object_observation(clone)}


def _noon_vgroup_copy_independence() -> Any:
    source = noon.VGroup(
        noon.Circle(radius=0.25),
        noon.Square(side_length=0.5),
    ).arrange(noon.RIGHT, buff=0.3)
    clone = source.copy().shift(noon.UP * 1.1)
    return {"source": _members_observation(source), "clone": _members_observation(clone)}


def _manim_vgroup_copy_independence() -> Any:
    source = manim.VGroup(
        manim.Circle(radius=0.25),
        manim.Square(side_length=0.5),
    ).arrange(manim.RIGHT, buff=0.3)
    clone = source.copy().shift(manim.UP * 1.1)
    return {"source": _members_observation(source), "clone": _members_observation(clone)}


def _noon_vgroup_arrange_grid() -> Any:
    group = noon.VGroup(*(noon.Square(side_length=0.5) for _ in range(4))).arrange_in_grid(
        rows=2, cols=2, buff=0.3
    )
    return _members_observation(group)


def _manim_vgroup_arrange_grid() -> Any:
    group = manim.VGroup(*(manim.Square(side_length=0.5) for _ in range(4))).arrange_in_grid(
        rows=2, cols=2, buff=0.3
    )
    return _members_observation(group)


def _noon_vgroup_scale() -> Any:
    group = noon.VGroup(
        noon.Circle(radius=0.25),
        noon.Circle(radius=0.25),
    ).arrange(noon.RIGHT, buff=0.4).scale(1.5)
    return _members_observation(group)


def _manim_vgroup_scale() -> Any:
    group = manim.VGroup(
        manim.Circle(radius=0.25),
        manim.Circle(radius=0.25),
    ).arrange(manim.RIGHT, buff=0.4).scale(1.5)
    return _members_observation(group)


def _noon_vgroup_rotate() -> Any:
    group = noon.VGroup(
        noon.Rectangle(width=0.8, height=0.4),
        noon.Rectangle(width=0.8, height=0.4),
    ).arrange(noon.RIGHT, buff=0.25).rotate(math.pi / 2.0)
    return _members_observation(group)


def _manim_vgroup_rotate() -> Any:
    group = manim.VGroup(
        manim.Rectangle(width=0.8, height=0.4),
        manim.Rectangle(width=0.8, height=0.4),
    ).arrange(manim.RIGHT, buff=0.25).rotate(math.pi / 2.0)
    return _members_observation(group)
'''
source = source.replace(anchor, probes + anchor, 1)

fixture_anchor = '    Fixture("vgroup_arrange", _noon_arrange, _manim_arrange),\n]'
replacement = '''    Fixture("vgroup_arrange", _noon_arrange, _manim_arrange),
    Fixture("set_xy", _noon_set_xy, _manim_set_xy),
    Fixture("next_to_point", _noon_next_to_point, _manim_next_to_point),
    Fixture("vgroup_add_remove", _noon_vgroup_add_remove, _manim_vgroup_add_remove),
    Fixture("vgroup_shift", _noon_vgroup_shift, _manim_vgroup_shift),
    Fixture(
        "mobject_copy_independence",
        _noon_mobject_copy_independence,
        _manim_mobject_copy_independence,
    ),
    Fixture(
        "vgroup_copy_independence",
        _noon_vgroup_copy_independence,
        _manim_vgroup_copy_independence,
    ),
    Fixture("vgroup_arrange_grid", _noon_vgroup_arrange_grid, _manim_vgroup_arrange_grid),
    Fixture("vgroup_scale", _noon_vgroup_scale, _manim_vgroup_scale),
    Fixture("vgroup_rotate", _noon_vgroup_rotate, _manim_vgroup_rotate),
]'''
if fixture_anchor not in source:
    raise SystemExit("fixture list tail not found")
source = source.replace(fixture_anchor, replacement, 1)

source = source.replace(
    '"family_aliasing": "Noon does not yet retain semantic family/alias relationships (#51)",',
    '"family_aliasing": "Python Group still flattens family identity pending shared semantic handles (#61)",',
    1,
)
unsupported_anchor = '    "stroke_scaling": "semantic stroke-width/scaling mode is being defined in #62",\n'
if unsupported_anchor not in source:
    raise SystemExit("unsupported anchor not found")
source = source.replace(
    unsupported_anchor,
    unsupported_anchor
    + '    "style_channels": "fill/stroke/object opacity integration is being migrated onto the #62 semantic style contract",\n',
    1,
)
path.write_text(source)

Path("scripts/apply-parity-o1-diff.py").unlink()
Path(".github/workflows/apply-parity-o1-diff.yml").unlink()
