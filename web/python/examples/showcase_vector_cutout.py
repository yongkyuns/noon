"""An original gasket example demonstrating Manim Community's BooleanOperations.

https://docs.manim.community/en/stable/examples.html#booleanoperations
The gasket design and teaching sequence were created for Noon.
"""
from noon import *


def label(text, x, y, size=24, color=WHITE):
    return Text(text, font_size=size, color=color).move_to((x, y, 0))


def circle(x, y, radius, color, opacity):
    return Circle(radius=radius, color=color, fill_opacity=opacity,
                  stroke_width=2.5).move_to((x, y, 0))


class VectorCutout(Scene):
    def construct(self):
        self.next_section("Place the cutters")
        heading = label("How does a vector editor cut a hole?", 0, 3.18, 41)
        body = RoundedRectangle(width=5.5, height=3.55, corner_radius=0.6,
                                color=TEAL, fill_opacity=0.37, stroke_width=3)
        left = circle(-0.65, 0, 1.10, GOLD, 0.26)
        right = circle(0.65, 0, 1.10, GOLD, 0.26)
        holes = [circle(x, y, 0.22, GOLD, 0.35)
                 for x in (-2.20, 2.20) for y in (-1.22, 1.22)]
        stock_label = label("Start with the solid outline.", 0, -2.90, 26, TEAL_A)
        cutter_label = label("Lay two overlapping cutters over the stock.", 0, -2.90, 26, GOLD)
        self.play(Write(heading), Create(body), run_time=1.25)
        self.play(FadeIn(stock_label), run_time=0.5)
        self.wait(0.65)
        self.play(FadeOut(stock_label), FadeIn(cutter_label),
                  LaggedStart(Create(left), Create(right), lag_ratio=0.4), run_time=1.3)
        self.wait(0.85)

        self.next_section("Unite the cutters")
        overlap = Intersection(left, right, color=ORANGE, fill_opacity=0.88, stroke_width=2)
        overlap_label = label("The orange region belongs to both cutters.", 0, -2.90, 26, GOLD)
        self.play(FadeOut(cutter_label), FadeIn(overlap_label), FadeIn(overlap), run_time=0.65)
        self.wait(1.2)
        center = Union(left, right, color=GOLD, fill_opacity=0.60, stroke_width=3)
        union_label = label("Union treats both cutters as one region.", 0, -2.90, 26, GOLD)
        self.play(FadeOut(left), FadeOut(right), FadeOut(overlap), FadeOut(overlap_label),
                  FadeIn(center), FadeIn(union_label), run_time=0.85)
        self.wait(1.0)

        self.next_section("Take the difference")
        difference_label = label("Difference keeps the stock outside every cutter.", 0, -2.90, 26, TEAL_A)
        self.play(*(FadeIn(hole) for hole in holes), run_time=0.7)
        self.wait(0.55)
        material = Difference(body, Union(center, *holes), color=TEAL,
                              fill_opacity=1, stroke_width=3)
        mat_label = label("Stock − cutters", 0, 2.22, 27, TEAL_A)
        self.play(FadeOut(body), FadeOut(center), *(FadeOut(hole) for hole in holes),
                  FadeOut(union_label), FadeIn(difference_label), FadeIn(material),
                  Write(mat_label), run_time=1.25)
        self.wait(0.9)

        # Pass a guide behind the compound path so the actual openings show through.
        guide = Line((-4.25, 0), (4.25, 0), color=GOLD, stroke_width=3).set_z_index(-1)
        self.play(Create(guide), run_time=1.2)
        self.wait(1.9)
