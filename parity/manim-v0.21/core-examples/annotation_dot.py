from manim import *


class AnnotationDotConstructor(Scene):
    def construct(self):
        dots = [
            AnnotationDot(),
            AnnotationDot(radius=0.2),
            AnnotationDot(color=RED),
            AnnotationDot(fill_color=GREEN, stroke_color=RED),
            AnnotationDot(fill_opacity=0.3, stroke_opacity=0.6, stroke_width=7),
        ]
        for index, dot in enumerate(dots):
            dot.shift((index - 2) * RIGHT)
        self.add(*dots)
        self.wait(0.2)
