"""Pair of the native/direct-WASM shared flagged-become example."""
from noon import Circle, Color, Rectangle, Scene


class OrdinaryBecomeSemantics(Scene):
    async def construct(self):
        fitted = Rectangle(
            width=3.0, height=1.0, position=(-2.0, 0.0),
            fill=Color(0.0, 0.0, 1.0), stroke=None,
        )
        stretched = Rectangle(
            width=3.0, height=1.0, position=(2.0, 0.0),
            fill=Color(0.0, 0.0, 1.0), stroke=None,
        )
        fitted_target = Rectangle(
            width=1.0, height=2.0, position=(3.5, 0.0),
            fill=Color(1.0, 1.0, 0.0), stroke=None,
        )
        stretched_target = Circle(
            radius=0.5, position=(-3.5, 0.0),
            fill=Color(1.0, 0.0, 1.0), stroke=None,
        )
        self.add(fitted, stretched)
        await self.wait(0.5)

        fitted.become(
            fitted_target,
            match_height=True,
            match_width=True,
            match_center=True,
        )
        stretched.become(
            stretched_target,
            match_height=True,
            match_width=True,
            match_center=True,
            stretch=True,
        )

        assert self.mobjects == [fitted, stretched]
        assert fitted_target not in self.mobjects
        assert stretched_target not in self.mobjects
        assert abs(fitted.get_center()[0] + 2.0) < 1e-6
        assert abs(fitted.width - 3.0) < 1e-6
        assert abs(fitted.height - 6.0) < 1e-6
        assert abs(stretched.get_center()[0] - 2.0) < 1e-6
        assert abs(stretched.width - 3.0) < 1e-6
        assert abs(stretched.height - 1.0) < 1e-6
        await self.wait(0.25)
