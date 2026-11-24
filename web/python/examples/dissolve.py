"""Visibility lifecycle paired with noon::example_scenes::dissolve."""
from noon import Dissolve, Glow, Pixels, Rectangle, Scene, VGroup, linear


class VisibilityLifecycle(Scene):
    def construct(self):
        background = Rectangle(width=16, height=9, fill="#000000", fill_opacity=1, stroke=None)

        def source(x):
            leaves = []
            for dx in (-0.2, 0.2):
                leaf = Rectangle(width=1.2, height=0.8, fill="#ffffff", fill_opacity=1, stroke=None)
                leaf.set_object_opacity(0.6)
                leaf.shift((x + dx, 0, 0))
                leaves.append(leaf)
            family = VGroup(*leaves)
            family.add_effect(Glow(radius=Pixels(2), source="silhouette"), name="halo", scope="composed")
            return family

        reference, target = source(-2), source(2)
        self.add(background, reference, target)
        self.wait(0.5)
        self.play(Dissolve(target, seed=7, softness=0.2, remover=False), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(Dissolve(target, direction="in", seed=7, softness=0.2), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(Dissolve(target, seed=7, softness=0.2), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(Dissolve(target, direction="in", seed=7, softness=0.2), run_time=1, rate_func=linear)
        self.wait(0.5)
