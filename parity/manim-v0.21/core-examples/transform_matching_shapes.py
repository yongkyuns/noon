from manim import *


def matching_triangle(x, y=0.0, color=BLUE):
    return Polygon(
        [-0.6, -0.5, 0.0], [0.6, -0.2, 0.0], [-0.15, 0.7, 0.0],
        fill_color=color, fill_opacity=1.0, stroke_width=0.0,
    ).shift([x, y, 0.0])


def matching_kite(x, y=0.0, color=GREEN):
    return Polygon(
        [0.0, -0.6, 0.0], [0.9, 0.0, 0.0],
        [0.0, 0.6, 0.0], [-0.3, 0.0, 0.0],
        fill_color=color, fill_opacity=1.0, stroke_width=0.0,
    ).shift([x, y, 0.0])


def roots_after_wait(scene):
    # ManimCE 0.21 inserts a point-free base Mobject for Wait. Ignore only
    # that inert placeholder, not unexpected drawable roots or family copies.
    return [mob for mob in scene.mobjects if not (
        type(mob) is Mobject and mob.get_num_points() == 0 and not mob.submobjects
    )]


class MatchingShapesReordered(Scene):
    def construct(self):
        # Nesting and reversed target order distinguish shape matching from zip.
        source = VGroup(VGroup(matching_triangle(-2.0)), matching_kite(2.0))
        target = VGroup(matching_kite(-2.0, color=YELLOW), VGroup(matching_triangle(2.0, color=RED)))
        before = Rectangle(width=8.0, height=0.3, fill_color=PURPLE, fill_opacity=1.0, stroke_width=0.0)
        after = Rectangle(width=0.3, height=2.0, fill_color=WHITE, fill_opacity=1.0, stroke_width=0.0).shift(2.0 * LEFT)
        self.add(before, source, after)
        assert self.mobjects == [before, source, after]
        self.play(TransformMatchingShapes(source, target), run_time=2.0, rate_func=linear)
        # The original target is appended; it is not a copy or the old source.
        assert self.mobjects == [before, after, target]
        self.wait(0.1)
        assert roots_after_wait(self) == [before, after, target]
        self.wait(0.1)


class MatchingShapesDefaultMismatches(Scene):
    def construct(self):
        source = VGroup(
            matching_triangle(-2.0),
            Square(side_length=1.0, fill_color=YELLOW, fill_opacity=1.0, stroke_width=0.0).shift(UP),
        )
        target = VGroup(matching_kite(0.0, -1.0), matching_triangle(2.0, color=RED))
        self.add(source)
        self.play(TransformMatchingShapes(source, target), run_time=2.0, rate_func=linear)
        assert self.mobjects == [target]
        self.wait(0.1)
        assert roots_after_wait(self) == [target]
        self.wait(0.1)
