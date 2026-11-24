"""Ordinary objects with sketch strokes, an optional hatch, and Create."""
from noon import Arrow, BLUE, Circle, Create, HatchPaint, Line, Rectangle, Scene, SketchStroke, TEAL, WHITE, YELLOW, linear


class HandDrawnDiagram(Scene):
    def construct(self):
        panel = Rectangle(width=2.6, height=1.6,
                          stroke=SketchStroke(TEAL, roughness=1.25, seed=8))
        panel.shift((-2.5, 0, 0)).set_fill(
            HatchPaint(TEAL, spacing=0.24, width=0.028), opacity=0.55
        )
        arrow = Arrow((-0.9, 0), (1.15, 0), buff=0,
                      stroke=SketchStroke(YELLOW, roughness=1.8, seed=3987))
        circle = Circle(0.82, stroke=SketchStroke(BLUE, roughness=1.45, seed=20))
        circle.shift((2.25, 0, 0))
        underline = Line((-3.55, -1.12), (-1.45, -1.12), stroke=SketchStroke(TEAL, seed=25))
        frame = Rectangle(8.8, 3.7, color=WHITE, stroke_opacity=0.25)

        self.add(frame)
        self.play(Create(panel), Create(arrow), Create(circle), Create(underline),
                  run_time=2.4, rate_func=linear)
        self.wait(0.6)
