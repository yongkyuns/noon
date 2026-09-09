"""Reveal endpoints, paired with the direct native Rust create_shapes example."""
from noon import BLUE, PINK, WHITE, Circle, Create, Line, Path, Scene, Square, VectorPath


class CreateShapes(Scene):
    def construct(self):
        circle = Circle(0.9).set_fill(BLUE, opacity=1).set_stroke(WHITE, width=5.5).move_to((-3, 1))
        square = Square(1.7).set_fill(PINK, opacity=1).set_stroke(WHITE, width=5.5).move_to((0, 1))
        line = Line((1.75, 1), (4.25, 1)).set_stroke(BLUE, width=5.5)
        wave = Path(VectorPath().move_to((-2.4, -1.6)).cubic_to(
            (-1.2, -2.6), (1.2, -0.6), (2.4, -1.6),
        ), fill=None, stroke=PINK, stroke_width=5.0)
        self.play(Create(circle), Create(square), Create(line), Create(wave),
                  run_time=3.2, easing="ease_in_out_cubic")
