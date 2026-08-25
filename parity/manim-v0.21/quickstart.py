# Canonical ManimCE v0.21.0 quickstart parity scenes.
#
# These scene bodies follow the Manim Community quickstart examples and are
# intentionally not tuned for Noon.  The parity harness changes only the import
# line when executing them through Noon's compatibility frontend.
#
# Upstream: https://docs.manim.community/en/v0.21.0/tutorials/quickstart.html
# Manim Community is MIT licensed.

from manim import *


class CreateCircle(Scene):
    def construct(self):
        circle = Circle()
        circle.set_fill(PINK, opacity=0.5)
        self.play(Create(circle))


class SquareToCircle(Scene):
    def construct(self):
        circle = Circle()
        circle.set_fill(PINK, opacity=0.5)

        square = Square()
        square.rotate(PI / 4)

        self.play(Create(square))
        self.play(Transform(square, circle))
        self.play(FadeOut(square))


class SquareAndCircle(Scene):
    def construct(self):
        circle = Circle()
        circle.set_fill(PINK, opacity=0.5)

        square = Square()
        square.set_fill(BLUE, opacity=0.5)

        square.next_to(circle, RIGHT, buff=0.5)
        self.play(Create(circle), Create(square))


class AnimatedSquareToCircle(Scene):
    def construct(self):
        circle = Circle()
        square = Square()

        self.play(Create(square))
        self.play(square.animate.rotate(PI / 4))
        self.play(Transform(square, circle))
        self.play(square.animate.set_fill(PINK, opacity=0.5))


# Supplemental source-equivalent parity probes. These are deliberately ordinary
# Manim source (not Noon-specific adaptations) and exercise reveal/lifecycle
# contracts that are not covered directly by the quickstart examples above.
class CreateLine(Scene):
    def construct(self):
        line = Line(LEFT, RIGHT)
        self.play(Create(line))


class UncreateLine(Scene):
    def construct(self):
        line = Line(LEFT, RIGHT)
        self.add(line)
        self.play(Uncreate(line))


class CreateStyledSquare(Scene):
    def construct(self):
        square = Square()
        square.set_fill(PINK, opacity=0.35)
        square.set_stroke(BLUE, width=8, opacity=0.65)
        self.play(Create(square))


class UncreateStyledSquare(Scene):
    def construct(self):
        square = Square()
        square.set_fill(PINK, opacity=0.35)
        square.set_stroke(BLUE, width=8, opacity=0.65)
        self.add(square)
        self.play(Uncreate(square))


class UncreateSquare(Scene):
    def construct(self):
        square = Square()
        self.add(square)
        self.play(Uncreate(square))
