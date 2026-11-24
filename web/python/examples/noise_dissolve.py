"""Persistent composed masks paired with noon::example_scenes::noise_dissolve.

The left composition supplies ordinary coverage for a numerical mask oracle.
The right applies one seeded field to the composed result of overlapping leaves.
"""
from noon import NoiseDissolve, Rectangle, Scene, VGroup, linear


class ComposedMask(Scene):
    def construct(self):
        background = Rectangle(width=16, height=9, fill="#000000", fill_opacity=1, stroke=None)

        def source(x):
            leaves = []
            for dx in (-0.2, 0.2):
                leaf = Rectangle(width=1.2, height=0.8, fill="#ffffff", fill_opacity=1, stroke=None)
                leaf.set_object_opacity(0.6)
                leaf.shift((x + dx, 0, 0))
                leaves.append(leaf)
            return VGroup(*leaves)

        reference, target = source(-2), source(2)
        target.add_effect(NoiseDissolve(seed=7, softness=0.08), name="mask")
        self.add(background, reference, target)
        self.wait(0.5)
        self.play(target.animate.set_effect("mask", progress=1), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(target.animate.set_effect("mask", progress=0, softness=0.4), run_time=1, rate_func=linear)
        self.wait(0.5)
        target.set_effect("mask", seed=4294967295, progress=0.5)
        self.wait(0.5)
        target.remove_effect("mask")
        self.wait(0.5)
