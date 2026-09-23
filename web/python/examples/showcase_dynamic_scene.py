"""A dense animated composition, not a benchmark claim or a collection of assertions.

Edit ROWS and COLS to explore load. Object counts are authored counts; actual
performance depends on the browser, backend, device, and viewport. No FPS values
are simulated by the scene. The separate harness records real frame observations.
"""
from math import cos, sin, sqrt
from noon import *

ROWS = 20
COLS = 30
TRACKS = 24
PALETTE = [BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE]


def grid_point(index):
    row, col = divmod(index, COLS)
    return ((col - (COLS - 1) / 2) * 0.35, ((ROWS - 1) / 2 - row) * 0.22, 0)


def spiral_point(index, count):
    fraction = index / max(1, count - 1)
    angle = fraction * 8 * TAU
    radius = 0.25 + 2.0 * sqrt(fraction)
    return (1.9 * radius * cos(angle), radius * sin(angle), 0)


def phase_label(text):
    return Text(text, font_size=22).shift(3.1 * DOWN)


class DynamicScene(Scene):
    def construct(self):
        count = ROWS * COLS
        title = Text("A field in motion", font_size=34).shift(3.25 * UP)
        subtitle = Text(f"{count} shapes + {TRACKS} animated labels", font_size=20, color=GRAY).shift(2.75 * UP)
        caption = phase_label("01 / Assemble the field")
        shapes = []
        for index in range(count):
            color = PALETTE[index % len(PALETTE)]
            shape = Square(side_length=0.14, color=color) if index % 2 else Circle(radius=0.07, color=color)
            shape.set_fill(color, opacity=0.85).set_stroke(width=1).move_to(grid_point(index))
            shapes.append(shape)
        labels = []
        for index in range(TRACKS):
            side = -1 if index < TRACKS // 2 else 1
            slot = index % (TRACKS // 2)
            label = Text(f"TRACK {index + 1:02d}", font="DejaVu Sans Mono", font_size=10, color=PALETTE[index % len(PALETTE)])
            label.move_to((6.1 * side, 2.3 - 0.42 * slot, 0))
            labels.append(label)

        self.play(FadeIn(title), FadeIn(subtitle), FadeIn(caption), run_time=0.8)
        self.play(*[Create(shape) for shape in shapes], *[FadeIn(label) for label in labels], run_time=2.4, rate_func=smooth)
        self.wait(0.4)

        self.play(FadeOut(caption), run_time=0.25)
        caption = phase_label("02 / Morph every object")
        self.play(FadeIn(caption), run_time=0.35)
        targets = []
        for index in range(count):
            color = PALETTE[(index + 3) % len(PALETTE)]
            target = Circle(radius=0.085, color=color) if index % 2 else Square(side_length=0.16, color=color).rotate(PI / 4)
            targets.append(target.set_fill(color, opacity=0.9).set_stroke(width=1).move_to(grid_point(index)))
        self.play(*[Transform(shape, target) for shape, target in zip(shapes, targets)], run_time=2.4, rate_func=smooth)

        self.play(FadeOut(caption), run_time=0.25)
        caption = phase_label("03 / Waves, color, and independent text motion")
        self.play(FadeIn(caption), run_time=0.35)
        for phase in range(3):
            motion = []
            for index, shape in enumerate(shapes):
                x, y, _ = grid_point(index)
                offset = 0.35 * sin(index % COLS * 0.32 + phase * 2.0)
                motion.append(shape.animate.move_to((x, y + offset, 0)).rotate(PI / 6).set_color(PALETTE[(index + phase) % len(PALETTE)]))
            text_motion = [label.animate.shift((0.04 if phase % 2 == 0 else -0.04) * UP).set_opacity(0.65 if (index + phase) % 2 else 1.0) for index, label in enumerate(labels)]
            self.play(*motion, *text_motion, run_time=1.6, rate_func=smooth)

        self.play(FadeOut(caption), run_time=0.25)
        caption = phase_label("04 / Recompose the entire field")
        self.play(FadeIn(caption), run_time=0.35)
        self.play(*[shape.animate.move_to(spiral_point(index, count)).rotate(PI / 3) for index, shape in enumerate(shapes)], run_time=2.8, rate_func=smooth)

        self.play(FadeOut(caption), run_time=0.25)
        caption = phase_label("05 / Exchange one third of the visible field")
        self.play(FadeIn(caption), run_time=0.35)
        leaving = shapes[::3]
        pulses = [Circle(radius=0.035, color=WHITE).set_fill(WHITE, opacity=1).set_stroke(opacity=0).move_to(shape.get_center()) for shape in leaving]
        self.play(*[FadeOut(shape) for shape in leaving], *[FadeIn(pulse) for pulse in pulses], run_time=1.2, rate_func=smooth)
        self.play(*[FadeOut(pulse) for pulse in pulses], *[FadeIn(shape) for shape in leaving], run_time=1.2, rate_func=smooth)

        self.play(FadeOut(caption), run_time=0.25)
        caption = phase_label("06 / Resolve into a new composition")
        self.play(FadeIn(caption), run_time=0.35)
        finale = []
        for index in range(count):
            color = PALETTE[(index // COLS + index % COLS) % len(PALETTE)]
            target = Square(side_length=0.16, color=color).set_fill(color, opacity=0.9).set_stroke(width=1).move_to(grid_point(index))
            finale.append(target)
        self.play(*[Transform(shape, target) for shape, target in zip(shapes, finale)], *[label.animate.set_opacity(1.0) for label in labels], run_time=2.8, rate_func=smooth)
        self.wait(1.6)
