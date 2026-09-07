"""Pair of the native/direct-WASM typed live geometry construction example."""
from noon import (
    BackgroundRectangle, Color, Group, Line, Path, Rectangle, Scene,
    SurroundingRectangle, VectorPath, linear,
)

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
        line = Line((-1.0, -2.0), (1.0, -2.0), stroke_width=4.0)
        late_path = Path(
            VectorPath().move_to((-0.3, -0.3)).line_to((0.3, -0.3))
            .line_to((0.0, 0.3)).close(),
            position=(0.0, 2.0), fill=Color(0.0, 1.0, 1.0), stroke=None,
        )
        self.add(rectangle, line, late_path)
        await self.play(rectangle.animate.shift((0.0, 1.0, 0.0)), run_time=1.0, rate_func=linear)
