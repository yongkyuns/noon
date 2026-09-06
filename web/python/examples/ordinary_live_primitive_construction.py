"""Construct Circle and Square after a shared ordinary wait.

The constructor candidate is typed Rust state until the retained live session
publishes it. Python does not allocate a second scene identity or apply style
mutations after publication.
"""

from noon import Circle, Color, Scene, Square


class OrdinaryLivePrimitiveConstruction(Scene):
    async def construct(self):
        anchor = Circle(radius=0.2, color=Color(1.0, 1.0, 1.0)).set_fill(
            Color(1.0, 1.0, 1.0), opacity=1.0
        )
        self.add(anchor)

        await self.wait(1.0)
        assert self.time == 1.0

        circle = Circle(
            radius=0.3,
            position=(2.0, -1.0),
            scale=(1.5, 1.5),
            fill=Color(0.0, 0.4, 1.0, 0.6),
            stroke_width=8.0,
            stroke_opacity=0.9,
        )
        square = Square(
            side_length=0.5,
            position=(-2.0, 1.0),
            rotation=0.25,
            color=Color(0.2, 0.9, 0.3),
            fill_opacity=0.7,
            opacity=0.75,
        )
        self.add(circle, square)

        assert circle.get_center() == (2.0, -1.0)
        assert square.get_center() == (-2.0, 1.0)
