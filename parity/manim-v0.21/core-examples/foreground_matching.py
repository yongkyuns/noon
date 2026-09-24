"""Pinned overlap oracle; assertions belong to qualification, not a gallery demo."""
from manim import *


def triangle(x, color):
    return VMobject(fill_color=color, fill_opacity=1, stroke_width=0).set_points_as_corners(
        [(-0.5, -0.8, 0), (0.5, -0.8, 0), (0, 0.8, 0), (-0.5, -0.8, 0)]
    ).shift(x * RIGHT)


def diamond(x, layer):
    return VMobject(fill_color="#00FF00", fill_opacity=1, stroke_width=0).set_points_as_corners(
        [(0, -0.8, 0), (0.5, 0, 0), (0, 0.8, 0), (-0.5, 0, 0), (0, -0.8, 0)]
    ).shift(x * RIGHT).set_z_index(layer)


def assert_membership(scene, display, foreground):
    # Cairo may admit an inert plain Mobject for Wait. Exclude only that exact
    # reference-only placeholder; extra drawable or family roots still fail.
    actual = [root for root in scene.mobjects if not (
        type(root) is Mobject and root.get_num_points() == 0 and not root.submobjects
    )]
    assert len(actual) == len(display) and all(a is b for a, b in zip(actual, display))
    actual_foreground = scene.foreground_mobjects
    assert len(actual_foreground) == len(foreground)
    assert all(a is b for a, b in zip(actual_foreground, foreground))


def exercise(scene, source_is_foreground=False, target_layer=0):
    source = VGroup(triangle(-2.4, "#0000FF"), triangle(-0.8, "#0000FF"))
    target = VGroup(
        triangle(-2.4, "#00FF00"), triangle(-0.8, "#00FF00"),
        triangle(0.8, "#00FF00"), diamond(2.4, target_layer),
    )
    source_members = tuple(source.submobjects)
    target_members = tuple(target.submobjects)
    left = Rectangle(width=5.4, height=0.3, fill_color="#FFFFFF", fill_opacity=1,
                     stroke_width=0).shift(0.8 * LEFT)
    right = Rectangle(width=1.2, height=0.3, fill_color="#FFFFFF", fill_opacity=1,
                      stroke_width=0).shift(2.4 * RIGHT).set_z_index(target_layer)
    later = Rectangle(width=6.2, height=1, fill_color="#FF0000", fill_opacity=1,
                      stroke_width=0)
    scene.add(source)
    if source_is_foreground:
        # The matched source is above left but below right in declaration order.
        scene.add_foreground_mobjects(left, source, right)
        assert_membership(scene, [left, source, right], [left, source, right])
    else:
        scene.add_foreground_mobjects(left, right)
        assert_membership(scene, [source, left, right], [left, right])

    scene.play(TransformMatchingShapes(source, target), run_time=2, rate_func=linear)
    assert_membership(scene, [target, left, right], [left, right])
    assert tuple(source.submobjects) == source_members
    assert tuple(target.submobjects) == target_members
    # Matching cleanup restores authored source geometry instead of making its
    # temporary/padded correspondence structure the persistent source family.
    for member, x in zip(source_members, (-2.4, -0.8)):
        assert abs(member.get_center()[0] - x) < 1e-6
        assert abs(member.get_center()[1]) < 1e-6
    scene.wait(0.2)
    assert_membership(scene, [target, left, right], [left, right])
    scene.add(later)
    assert_membership(scene, [target, later, left, right], [left, right])
    scene.wait(0.2)
    assert_membership(scene, [target, later, left, right], [left, right])
    assert tuple(source.submobjects) == source_members
    assert tuple(target.submobjects) == target_members


class MatchingOrdinaryForeground(Scene):
    def construct(self):
        exercise(self)


class MatchingForegroundSource(Scene):
    def construct(self):
        exercise(self, source_is_foreground=True, target_layer=2)


class MatchingForegroundOnlyLayer(Scene):
    def construct(self):
        exercise(self, target_layer=2)
