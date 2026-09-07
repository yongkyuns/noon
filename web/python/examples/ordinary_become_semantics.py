"""Pair of the native/direct-WASM shared flagged-become example."""
from noon import Circle, Color, Ellipse, PI, Rectangle, Scene


ELLIPSE_HULL_WIDTH = 3.6630139825174126
ELLIPSE_HULL_HEIGHT = 2.46422807100826


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
        ellipse = Rectangle(
            width=0.4, height=0.4, position=(0.0, -2.5),
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
        ellipse_target = Ellipse(
            width=4.0, height=1.5, position=(0.0, 2.5),
            fill=Color(0.0, 1.0, 1.0), stroke=None,
        ).rotate(PI / 6.0)
        assert abs(ellipse_target.get_center()[0]) < 1e-6
        assert abs(ellipse_target.get_center()[1] - 2.5) < 1e-6
        assert abs(ellipse_target.width - ELLIPSE_HULL_WIDTH) < 1e-5
        assert abs(ellipse_target.height - ELLIPSE_HULL_HEIGHT) < 1e-5
        self.add(fitted, stretched, ellipse)
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
        ellipse.become(ellipse_target, match_center=True)

        assert self.mobjects == [fitted, stretched, ellipse]
        assert fitted_target not in self.mobjects
        assert stretched_target not in self.mobjects
        assert ellipse_target not in self.mobjects
        assert abs(fitted.get_center()[0] + 2.0) < 1e-6
        assert abs(fitted.width - 3.0) < 1e-6
        assert abs(fitted.height - 6.0) < 1e-6
        assert abs(stretched.get_center()[0] - 2.0) < 1e-6
        assert abs(stretched.width - 3.0) < 1e-6
        assert abs(stretched.height - 1.0) < 1e-6
        assert abs(ellipse.get_center()[0]) < 1e-6
        assert abs(ellipse.get_center()[1] + 2.5) < 1e-6
        assert abs(ellipse.width - ELLIPSE_HULL_WIDTH) < 1e-5
        assert abs(ellipse.height - ELLIPSE_HULL_HEIGHT) < 1e-5
        await self.wait(0.25)
