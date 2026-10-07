"""M0 source review, NOT an enabled/shipping example.

The compiled Rust declaration subset is effect_authoring_contract.rs. This file
reviews the complete intended grammar; M1/M2 add execution and M3/M4 add the
explicit group/view/custom calls. Do not replace missing operations with mocks.
The matching Rust review is authoring_review.rs. These files are syntax-checked,
not cross-language execution evidence or a second implementation of the effects.
"""
import numpy as np
from noon import (
    AnimationGroup, Bloom, Circle, Glow, GlowPulse, Group, ImageMobject, MovingCameraScene,
    Pixels, RIGHT, Scene, Succession, Text, UP, VMobject, linear,
)
from effect_fixture import ScanBand


class LuminousExplanation(Scene):
    def construct(self):
        dot = Circle(radius=0.08).set_fill("#FFFFFF", opacity=1).set_stroke(width=0)
        dot.set_glow(radius=0.15, intensity=0.35)
        self.add(dot)
        self.play(dot.animate.shift(RIGHT * 2).set_glow(intensity=1.2),
                  run_time=1.5, rate_func=linear)
        self.play(GlowPulse(dot, intensity=2.0), run_time=0.6)
        self.wait(0.5)
        self.play(dot.animate.set_glow(intensity=0.0), run_time=0.4, rate_func=linear)
        dot.remove_glow()


class MixedEffectsReview(MovingCameraScene):
    def construct(self):
        title = Text("Signal", font="DejaVu Sans Mono", font_size=48).shift(UP * 2)
        path = VMobject().set_points_as_corners([(-2, -1, 0), (0, 1, 0), (2, -1, 0)])
        path.set_fill(opacity=0).set_stroke("#FFFFFF", width=2)
        image = ImageMobject(np.array([
            [[255, 0, 0, 255], [0, 0, 255, 0]],
            [[0, 255, 0, 128], [255, 255, 255, 255]],
        ], dtype=np.uint8)).set_height(1)
        dot = Circle(radius=0.08).set_fill("#FFFFFF", opacity=1).set_stroke(width=0)
        inner = Group(path, image)
        group = Group(inner, dot)
        aliases = Group(inner, dot, path)  # A query/copy family; never isolated/displayed here.
        self.add(group, title)
        dot.set_glow(intensity=0.35)
        title.add_effect(Glow(radius=Pixels(12)), name="accent")
        title.add_effect(ScanBand(width=0.2, phase=0.0), name="scan")
        group.add_effect(Glow(radius=Pixels(12)), name="group-halo", scope="composed")
        self.camera.frame.add_effect(Bloom(intensity=0.5), name="bloom", scope="view")
        self.on_click(dot, GlowPulse(dot, intensity=2.0, run_time=0.6))

        self.play(AnimationGroup(
            dot.animate(run_time=1.5, rate_func=linear).shift(RIGHT * 2).set_glow(intensity=1.2),
            title.animate(run_time=1.5, rate_func=linear).set_effect("accent", intensity=1.4),
            rate_func=linear,
        ))
        self.play(GlowPulse(dot, intensity=2.0), run_time=0.6)
        self.play(Succession(
            title.animate(run_time=0.5, rate_func=linear).set_effect("scan", phase=1.0),
            dot.animate(run_time=0.5, rate_func=linear).set_glow(intensity=0.5),
            rate_func=linear,
        ))
        # Persistent edits after a completion barrier use the same live scene.
        group.set_effect("group-halo", intensity=0.25)
        self.wait(0.5)
        self.play(self.camera.frame.animate.set_effect("bloom", intensity=0.0),
                  run_time=0.3, rate_func=linear)
        self.camera.frame.remove_effect("bloom")
        group.remove_effect("group-halo")
        title.remove_effect("scan")
        self.on_click(dot, None)
        del aliases  # Dropping a language wrapper is not an attachment removal operation.

# Host review: sample the same authored times forward and after rewind, comparing
# parameters/painter order/identities. Direct seek is host playback control, not
# a new Scene.seek or an effects replay engine. Interaction occurs on its own
# existing clock: during the authored intensity write it is suppressed; during
# the wait at authored t=3.1 it captures .5, peaks at 2 and restores .5.
