"""A hand-drawn diagram using ordinary Create and a retained hatch fill."""
from noon import BLUE, TEAL, WHITE, YELLOW, Create, HandDrawn, HatchPaint, Rectangle, Scene, linear


class HandDrawnDiagram(Scene):
    def construct(self):
        panel = HandDrawn.rectangle(2.6, 1.6, roughness=1.25, seed=8, color=TEAL)
        panel.shift((-2.5, 0, 0)).set_fill(
            HatchPaint(TEAL, spacing=0.24, width=0.028), opacity=0.55
        )
        arrow = HandDrawn.arrow((-0.9, 0), (1.15, 0), roughness=1.1, seed=12, color=YELLOW)
        circle = HandDrawn.circle(0.82, roughness=1.45, seed=20, color=BLUE)
        circle.shift((2.25, 0, 0))
        underline = HandDrawn.line((-3.55, -1.12), (-1.45, -1.12), seed=25, color=TEAL)
        frame = Rectangle(8.8, 3.7, color=WHITE, stroke_opacity=0.25)

        self.add(frame)
        self.play(Create(panel), Create(arrow), Create(circle), Create(underline),
                  run_time=2.4, rate_func=linear)
        self.wait(0.6)
