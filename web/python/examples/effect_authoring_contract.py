"""M0 declaration contract, paired with the native Rust example of the same name.

This checks shared authoring, not glow rendering. Until M1, effect-bearing
execution must reject; cleanup then demonstrates ordinary source continuation.
"""
from noon import Circle, Glow, Pixels, RIGHT, Scene


class EffectAuthoringContract(Scene):
    def construct(self):
        dot = Circle(radius=0.08)
        self.add(dot)
        assert dot.set_glow(intensity=0.25) is dot
        assert dot.add_effect(Glow(radius=Pixels(12)), name="accent") is dot
        original = dot.get_effect("glow")
        movement = dot.animate(run_time=1.5).shift(RIGHT * 2).set_glow(intensity=1.4)
        movement.set_effect("accent", intensity=0.6)
        assert original.authored_definition.intensity == 0.25
        assert movement.target.get_effect("glow").authored_definition.intensity == 1.4
        assert dot.get_effect("accent").authored_definition.radius == Pixels(12)
        assert movement.target.get_effect("accent").authored_definition.intensity == 0.6

        # Exercise actual Rust admission, not a Python-only skip/fake renderer.
        try:
            self.live_execution()
        except Exception as error:
            assert "effect declarations cannot execute" in str(error), str(error)
        else:
            raise AssertionError("effect-bearing execution was silently accepted")

        assert dot.remove_effect(original) is dot
        try:
            original.authored_definition
        except ReferenceError:
            pass
        else:
            raise AssertionError("retired effect handle remained usable")
        # The target's independent declaration must outlive removal on its source.
        assert movement.target.get_effect("glow").authored_definition.intensity == 1.4
        dot.remove_effect("accent")
        movement.target.remove_glow().remove_effect("accent")
        self.wait(0.1)
