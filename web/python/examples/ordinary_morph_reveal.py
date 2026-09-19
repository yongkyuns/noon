"""Concurrent retained morph and reveal, paired with the direct Rust example."""
from noon import BLUE, PINK, Create, Path, Scene, Transform, Vec2, VectorPath, linear


class MorphReveal(Scene):
    def construct(self):
        source = (
            VectorPath()
            .move_to(Vec2(-2.4, -0.8))
            .cubic_to(Vec2(-2.0, 2.5), Vec2(0.8, -2.4), Vec2(1.0, 0.2))
            .line_to(Vec2(2.4, 1.0))
        )
        target_path = (
            VectorPath()
            .move_to(Vec2(-2.4, -0.8))
            .cubic_to(Vec2(-0.5, -2.6), Vec2(0.4, 2.8), Vec2(1.0, 0.2))
            .line_to(Vec2(2.0, -1.4))
        )
        shape = Path(source, fill=None, stroke=BLUE, stroke_width=9.0)
        target = Path(target_path, fill=None, stroke=PINK, stroke_width=9.0)
        self.play(
            Create(shape, rate_func=linear),
            Transform(shape, target, rate_func=linear),
            run_time=3.0,
        )
