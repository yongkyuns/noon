"""Two bounded always_redraw targets rebuilt from authored values."""
from noon import *


class AlwaysRedrawShowcase(Scene):
    async def construct(self):
        circle_x = ValueTracker(-2.2)
        circle_radius = ValueTracker(0.48)
        rectangle_x = ValueTracker(2.2)
        rectangle_angle = ValueTracker(0.0)
        visibility = ValueTracker(0.0)

        # Put paint and placement options on each newly constructed shape.
        circle = always_redraw(lambda: Circle(
            radius=circle_radius.get_value(), color=BLUE,
            fill=BLUE, fill_opacity=0.84, opacity=visibility.get_value(),
            position=(circle_x.get_value(), 0.15),
        ))
        rectangle = always_redraw(lambda: Rectangle(
            width=1.15, height=0.78, color=TEAL,
            fill=TEAL, fill_opacity=0.84, opacity=visibility.get_value(),
            rotation=rectangle_angle.get_value(),
            position=(rectangle_x.get_value(), 0.15),
        ))

        title = Text("Redraw from animated values", font_size=32).shift(3.25 * UP)
        subtitle = Text("Each producer returns one bounded shape.", font_size=20, color=GRAY).shift(2.65 * UP)
        labels = [
            Text("Circle · position + radius", font_size=18, color=BLUE).move_to((-2.2, 1.65, 0)),
            Text("Rectangle · position + angle", font_size=18, color=TEAL).move_to((2.2, 1.65, 0)),
        ]
        resolved = Text("Both shapes redraw as their values change.", font_size=21).shift(2.65 * UP)

        self.add(circle, rectangle)
        await self.play(
            FadeIn(title), FadeIn(subtitle), *[FadeIn(label) for label in labels],
            visibility.animate.set_value(1), run_time=0.8, rate_func=smooth,
        )
        await self.wait(0.4)
        await self.play(
            circle_x.animate.set_value(-1.15), circle_radius.animate.set_value(0.82),
            rectangle_x.animate.set_value(1.15), rectangle_angle.animate.set_value(0.65),
            run_time=2.4, rate_func=smooth,
        )
        await self.wait(0.5)
        await self.play(
            circle_x.animate.set_value(-2.2), circle_radius.animate.set_value(0.48),
            rectangle_x.animate.set_value(2.2), rectangle_angle.animate.set_value(-0.45),
            run_time=2.4, rate_func=smooth,
        )
        await self.play(FadeOut(subtitle), FadeIn(resolved), run_time=0.5)
        await self.wait(1.2)
