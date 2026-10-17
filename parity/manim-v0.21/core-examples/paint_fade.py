from manim import *


class PaintFade(Scene):
    def construct(self):
        a = Square(0.6).shift(LEFT)
        b = Square(0.6).shift(RIGHT)
        family = VGroup(VGroup(a, b), a)
        family.set_fill("#FF0000", 0.8)
        family.set_stroke("#0000FF", width=4, opacity=1)
        a.fade()
        assert abs(a.get_fill_opacity() - 0.4) < 1e-6
        self.add(family)
        self.wait(0.1)
        family.set_color("#3399FF")
        family.set_opacity(0.5)
        a.fade(0.5, family=False)
        family.fade(0.5)
        assert abs(a.get_fill_opacity() - 0.0625) < 1e-6
        assert abs(a.get_stroke_opacity() - 0.0625) < 1e-6
        assert abs(b.get_fill_opacity() - 0.25) < 1e-6
        assert abs(b.get_stroke_opacity() - 0.25) < 1e-6
        family.fade_to("#FF0000", 0.5)
        assert abs(a.get_fill_opacity() - 0.0625) < 1e-6
        assert abs(a.get_stroke_opacity() - 0.0625) < 1e-6
        assert abs(b.get_fill_opacity() - 0.25) < 1e-6
        assert abs(b.get_stroke_opacity() - 0.25) < 1e-6
        for mob, expected in ((a, (0.8, 0.15, 0.25)), (b, (0.6, 0.3, 0.5))):
            for paint in (mob.get_fill_color(), mob.get_stroke_color()):
                # Normalize the observable color representation only.
                channels = paint.to_rgb() if hasattr(paint, "to_rgb") else (paint.red, paint.green, paint.blue)
                assert all(abs(float(channel) - value) < 1e-6 for channel, value in zip(channels, expected))
        self.wait(0.1)
