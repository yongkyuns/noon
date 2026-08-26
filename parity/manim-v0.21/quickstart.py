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


class DifferentRotations(Scene):
    def construct(self):
        left_square = Square(color=BLUE, fill_opacity=0.7).shift(2 * LEFT)
        right_square = Square(color=GREEN, fill_opacity=0.7).shift(2 * RIGHT)
        self.play(
            left_square.animate.rotate(PI), Rotate(right_square, angle=PI), run_time=2
        )
        self.wait()


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


class CreateTransformedSquare(Scene):
    def construct(self):
        square = Square(
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=8,
        )
        square.scale(1.3).rotate(PI / 6).shift(1.5 * RIGHT + 0.75 * UP)
        self.play(Create(square))


class UncreateTransformedSquare(Scene):
    def construct(self):
        square = Square(
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=8,
        )
        square.scale(1.3).rotate(PI / 6).shift(1.5 * RIGHT + 0.75 * UP)
        self.add(square)
        self.play(Uncreate(square))


class UncreateSquare(Scene):
    def construct(self):
        square = Square()
        self.add(square)
        self.play(Uncreate(square))


class SetColorIndependentOpacity(Scene):
    def construct(self):
        square = Square(
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=8,
        )
        square.set_color(GREEN)
        self.add(square)
        # Keep a real one-second animation interval so the raster oracle samples
        # normal Manim frames instead of save-last-frame collapsing a static wait.
        self.play(square.animate.shift(ORIGIN))


class PaletteSwatches(Scene):
    def construct(self):
        colors = [RED, GREEN, BLUE, YELLOW, PURPLE]
        swatches = []
        for index, color in enumerate(colors):
            swatch = Square(
                side_length=0.9,
                fill_color=color,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            )
            swatch.move_to((index - 2) * 1.2 * RIGHT)
            swatches.append(swatch)
        self.add(*swatches)
        self.play(swatches[-1].animate.shift(ORIGIN))


class TranslucentPainterOrder(Scene):
    def construct(self):
        back = Square(
            side_length=2.4,
            fill_color=RED,
            fill_opacity=0.5,
            stroke_opacity=0.0,
        ).shift(0.4 * LEFT)
        front = Square(
            side_length=2.4,
            fill_color=BLUE,
            fill_opacity=0.5,
            stroke_opacity=0.0,
        ).shift(0.4 * RIGHT)
        self.add(back, front)
        self.play(front.animate.shift(ORIGIN))


class AnimateRotateOffsetSquare(Scene):
    def construct(self):
        square = Square(
            side_length=1.5,
            fill_color=BLUE,
            fill_opacity=1.0,
            stroke_opacity=0.0,
        ).shift(2 * RIGHT)
        self.add(square)
        self.play(square.animate(rate_func=linear).rotate(PI))


class FillOpacityLadder(Scene):
    def construct(self):
        opacities = [0.0, 0.25, 0.5, 0.75, 1.0]
        swatches = []
        for index, opacity in enumerate(opacities):
            swatch = Square(
                side_length=0.9,
                fill_color=PINK,
                fill_opacity=opacity,
                stroke_opacity=0.0,
            )
            swatch.move_to((index - 2) * 1.2 * RIGHT)
            swatches.append(swatch)
        self.add(*swatches)
        self.play(swatches[-1].animate.shift(ORIGIN))


class SetGlobalOpacity(Scene):
    def construct(self):
        square = Square(
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=8,
        )
        square.set_opacity(0.5)
        self.add(square)
        self.play(square.animate.shift(ORIGIN))


class AnimateSetColor(Scene):
    def construct(self):
        square = Square(
            fill_color=BLUE,
            fill_opacity=0.35,
            stroke_color=RED,
            stroke_opacity=0.65,
            stroke_width=8,
        )
        self.add(square)
        self.play(square.animate(rate_func=linear).set_color(GREEN))


class SetColorExplicitEquivalent(Scene):
    def construct(self):
        set_color_square = Square(
            side_length=1.2,
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=4,
        ).shift(1.0 * LEFT)
        set_color_square.set_color(GREEN)

        explicit_square = Square(
            side_length=1.2,
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=4,
        ).shift(1.0 * RIGHT)
        explicit_square.set_fill(color=GREEN)
        explicit_square.set_stroke(color=GREEN)

        self.add(set_color_square, explicit_square)
        self.play(explicit_square.animate.shift(ORIGIN))


class ThreeLayerPainterOrder(Scene):
    def construct(self):
        def layer(color, offset):
            return Square(
                side_length=1.6,
                fill_color=color,
                fill_opacity=0.5,
                stroke_opacity=0.0,
            ).shift(offset)

        left_center = 1.8 * LEFT
        left_red = layer(RED, left_center + 0.3 * LEFT)
        left_green = layer(GREEN, left_center + 0.3 * RIGHT)
        left_blue = layer(BLUE, left_center + 0.3 * UP)

        right_center = 1.8 * RIGHT
        right_red = layer(RED, right_center + 0.3 * LEFT)
        right_green = layer(GREEN, right_center + 0.3 * RIGHT)
        right_blue = layer(BLUE, right_center + 0.3 * UP)

        # Same colors and geometry, opposite painter order. Each cluster contains
        # single-layer, pairwise-overlap, and true three-layer overlap regions.
        self.add(left_red, left_green, left_blue)
        self.add(right_blue, right_green, right_red)
        self.play(right_red.animate.shift(ORIGIN))


class FadeInShiftScaleStyledSquare(Scene):
    def construct(self):
        square = Square(
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=8,
        )
        self.play(FadeIn(square, shift=RIGHT, scale=0.5, rate_func=linear))


class FadeOutShiftScaleStyledSquare(Scene):
    def construct(self):
        square = Square(
            fill_color=PINK,
            fill_opacity=0.35,
            stroke_color=BLUE,
            stroke_opacity=0.65,
            stroke_width=8,
        )
        self.add(square)
        self.play(FadeOut(square, shift=RIGHT, scale=0.5, rate_func=linear))


class CameraUnitOffsets(Scene):
    def construct(self):
        def marker(position):
            return Square(
                side_length=0.4,
                fill_color=WHITE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(position)

        self.add(marker(ORIGIN), marker(RIGHT), marker(UP))
        self.wait(1)
