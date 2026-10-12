# Source-equivalent ManimCE v0.21.0 ApplyMatrix parity candidate.
# Paired with Rust `example_scenes::apply_matrix`.
from noon import *


class ApplyMatrixShear(Scene):
    def construct(self):
        square = Square(side_length=2.0).set_fill(BLUE, opacity=0.6)
        self.add(square)
        deferred = ApplyMatrix([[1.0, 1.0], [0.0, 1.0]], square)
        self.play(square.animate.shift((1.0, 1.0)), run_time=0.25, rate_func=linear)
        self.play(deferred)
        center = square.get_center()
        assert abs(center.x - 2.0) < 1e-6 and abs(center.y - 1.0) < 1e-6
        assert abs(square.width - 4.0) < 1e-6 and abs(square.height - 2.0) < 1e-6
