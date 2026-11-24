"""Sketch an open Bézier and a filled compound path."""
from noon import Create, Path, Scene, SketchStroke, TEAL, VMobject, VectorPath, YELLOW, linear


class HandDrawnPaths(Scene):
    def construct(self):
        bezier = VectorPath().move_to((-3.7, -0.7)).cubic_to((-3.4, 2), (-1.35, -2), (-0.65, 0.75))
        curve = Path(bezier, stroke=SketchStroke(YELLOW, roughness=1.3, seed=17),
                     stroke_width=5, fill_opacity=0)

        ring = VMobject(stroke=SketchStroke(TEAL, seed=8), stroke_width=4)
        for start, *points in (
            ((0.55, -1.2), (3.6, -1.2), (3.6, 1.2), (0.55, 1.2)),
            ((1.35, -0.53), (1.35, 0.53), (2.8, 0.53), (2.8, -0.53)),
        ):
            ring.start_new_path(start)
            for point in points:
                ring.add_line_to(point)
            ring.close_path()
        ring.set_fill(TEAL, opacity=0.38)

        self.play(Create(curve), Create(ring), run_time=2.4, rate_func=linear)
        self.wait(0.6)
