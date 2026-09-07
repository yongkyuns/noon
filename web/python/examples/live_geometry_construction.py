"""Pair of the native/direct-WASM typed live geometry construction example."""
from noon import (
    Annulus, BackgroundRectangle, Color, Dot, Group, Line, Path, Rectangle, Scene,
    SurroundingRectangle, Underline, VectorPath, linear, PI,
)

LINE_START = (-1.111538105676658, -3.0747595264191645)
LINE_END = (1.111538105676658, -0.9252404735808355)
LINE_COLOR = Color(0.2, 0.4, 0.8, 0.35)


def assert_point(actual, expected):
    assert abs(actual[0] - expected[0]) < 1e-6
    assert abs(actual[1] - expected[1]) < 1e-6


def assert_color(actual, expected):
    assert abs(actual.red - expected.red) < 1e-6
    assert abs(actual.green - expected.green) < 1e-6
    assert abs(actual.blue - expected.blue) < 1e-6
    assert abs(actual.alpha - expected.alpha) < 1e-6

class LiveGeometryConstruction(Scene):
    async def construct(self):
        path = Path(
            VectorPath().move_to((-0.6, -0.5)).line_to((0.6, -0.5))
            .quadratic_to((0.8, 0.0), (0.6, 0.5))
            .cubic_to((0.2, 0.7), (-0.2, 0.7), (-0.6, 0.5)).close(),
            position=(-2.0, 0.0), fill=Color(0.0, 0.0, 1.0), stroke=None,
        )
        outline = SurroundingRectangle(path, buff=0.15, corner_radius=0.1)
        background = BackgroundRectangle(Group(path), buff=0.25, corner_radius=0.1, fill_opacity=0.5)
        self.add(background, path, outline)
        await self.wait(1.0)
        rectangle = Rectangle(width=1.2, height=0.8, position=(2.0, 0.0),
                              fill=Color(0.0, 1.0, 0.0), stroke=None)
        line = (
            Line(
                (-1.0, -0.5), (1.0, 0.5),
                fill=Color(1.0, 1.0, 0.0, 0.7),
                stroke=Color(0.2, 0.4, 0.8),
                stroke_opacity=LINE_COLOR.alpha,
                stroke_width=4.0,
            )
            .scale((1.5, 0.75))
            .rotate(PI / 6.0)
            .shift((0.0, -2.0, 0.0))
        )
        assert_point(line.get_start(), LINE_START)
        assert_point(line.get_end(), LINE_END)
        assert_color(line.get_color(), LINE_COLOR)
        late_path = Path(
            VectorPath().move_to((-0.3, -0.3)).line_to((0.3, -0.3))
            .line_to((0.0, 0.3)).close(),
            position=(0.0, 2.0), fill=Color(0.0, 1.0, 1.0), stroke=None,
        )
        self.add(rectangle, line, late_path)
        await self.play(
            rectangle.animate.shift((0.0, 1.0, 0.0)),
            line.animate.shift((0.0, 1.0, 0.0)),
            run_time=1.0,
            rate_func=linear,
        )
        assert_point(line.get_start(), (LINE_START[0], LINE_START[1] + 1.0))
        assert_point(line.get_end(), (LINE_END[0], LINE_END[1] + 1.0))
        assert_color(line.get_color(), LINE_COLOR)
        dot = Dot((-4.0, -1.5, 0.0), radius=0.25, color=Color(1.0, 0.0, 0.0))
        annulus = Annulus(inner_radius=0.25, outer_radius=0.5, arc_center=(4.0, -1.5, 0.0),
                          color=Color(1.0, 1.0, 0.0))
        underline = Underline(rectangle, buff=0.15, stroke_width=8.0)
        self.add(dot, annulus, underline)
