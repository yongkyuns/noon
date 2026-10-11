"""Persistent glow on the shared Scene path; paired with glow_scene_playback.rs.

The finite M1 profile is one glow on a filled, unstroked circle or rectangle.
This uses ordinary .animate/play, not a Python updater or an effect renderer.
"""
from noon import BLUE, RED, WHITE, Circle, Pixels, RIGHT, Scene, linear


class GlowScenePlayback(Scene):
    def construct(self):
        dot = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None)
        dot.set_glow(color=RED, radius=Pixels(3.25), intensity=0.4000000000000123)
        self.add(dot)
        original = dot.get_effect("glow")
        self.play(
            dot.animate.shift(RIGHT * 2).set_glow(
                color=BLUE, radius=Pixels(6.5), intensity=1.4),
            run_time=1.0, rate_func=linear,
        )
        assert dot.get_center() == (2.0, 0.0)
        assert original.authored_definition.intensity == 1.4
        assert original.authored_definition.radius == Pixels(6.5)
        self.play(dot.animate.set_glow(intensity=0.0), run_time=0.5, rate_func=linear)
        assert original.authored_definition.intensity == 0.0
        dot.remove_glow()
        try:
            original.authored_definition
        except ReferenceError:
            pass
        else:
            raise AssertionError("removed attachment handle must stay retired")
        self.wait(0.25)
        assert dot.get_center() == (2.0, 0.0)
