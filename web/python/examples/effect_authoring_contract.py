"""M0 declaration contract, paired with the native Rust example of the same name.

This checks shared authoring, not glow rendering. Until M1, effect-bearing
execution must reject; cleanup then demonstrates ordinary source continuation.
"""
from noon import Circle, Glow, Pixels, RIGHT, Scene, NoonForeignHandleError, NoonUnsupportedError


class EffectAuthoringContract(Scene):
    def construct(self):
        def rejects(kind, operation):
            try:
                operation()
            except kind as error:
                return error
            raise AssertionError(f"expected {kind.__name__} from effect operation")

        dot = Circle(radius=0.08)
        self.add(dot)
        assert dot.set_glow(intensity=0.25) is dot
        assert dot.add_effect(Glow(radius=Pixels(12)), name="accent") is dot
        original = dot.get_effect("glow")
        # Real Python -> generated JS -> Rust calls, not facade routing spies.
        # None preserves, while zero is an explicit valid write.
        dot.set_glow(intensity=None).set_glow(intensity=0.0)
        assert original.authored_definition.intensity == 0.0
        dot.set_glow(intensity=0.25)
        for invalid in (-1.0, 8.1, float("nan"), float("inf")):
            rejects(ValueError, lambda: dot.set_glow(intensity=invalid))
        rejects(TypeError, lambda: dot.set_glow(intensity=True))
        rejects(ValueError, lambda: dot.set_glow(color="#FF0000", intensity=-1.0))
        rejects(ValueError, lambda: dot.add_effect(Glow(intensity=7), name="glow"))
        assert original.authored_definition.intensity == 0.25
        assert original.authored_definition.color.green == 1.0
        movement = dot.animate(run_time=1.5).shift(RIGHT * 2).set_glow(intensity=1.4)
        movement.set_effect("accent", intensity=0.6)
        wrong_owner = rejects(NoonForeignHandleError,
                              lambda: movement.target.set_effect(original, intensity=3.0))
        assert wrong_owner.code == "effect.foreign_owner"
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
        # Reusing a name must never revive or redirect a retired reference.
        dot.set_glow(intensity=0.9)
        rejects(ReferenceError, lambda: dot.set_effect(original, intensity=3.0))
        rejects(ReferenceError, lambda: dot.remove_effect(original))
        assert dot.get_effect("glow").authored_definition.intensity == 0.9
        dot.remove_glow()
        # The target's independent declaration must outlive removal on its source.
        assert movement.target.get_effect("glow").authored_definition.intensity == 1.4
        dot.remove_effect("accent")
        movement.target.remove_glow().remove_effect("accent")
        self.wait(0.1)
        # After a real completion barrier, setters must use live publication.
        # Rejecting unavailable appearance must leave normal continuation usable.
        rejects(NoonUnsupportedError, lambda: dot.set_glow(intensity=0.5))
        rejects(ValueError, lambda: dot.get_effect("glow"))
        assert dot.get_center() == (0.0, 0.0)
