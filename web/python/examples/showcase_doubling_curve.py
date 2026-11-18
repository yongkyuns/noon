"""Watch equal growth ratios become equal distances on a log axis."""
from noon import *

GROWTH_FACTOR = 2.0  # Try 1.5 or 3. A constant factor still makes a straight log plot.
STEPS = 4
X0, Y0 = -4.4, -2.05
WIDTH, HEIGHT = 8.1, 4.3
NIGHT = color_from_hex("#101d29")
GRID = color_from_hex("#385564")
MUTED = color_from_hex("#b5cad0")
INK = color_from_hex("#fff8e8")
DATA = color_from_hex("#71ead1")
STEP = color_from_hex("#ffc874")


def text(value, size, x, y, color=INK):
    return Text(value, font_size=size, color=color).move_to((x, y, 0))


def position(step, log=False):
    level = step / STEPS if log else (GROWTH_FACTOR ** step - 1) / (GROWTH_FACTOR ** STEPS - 1)
    return (X0 + WIDTH * step / STEPS, Y0 + HEIGHT * level, 0)


def curve(log=False, start=0, end=STEPS):
    graph = VMobject(stroke_color=DATA, stroke_width=4, fill_opacity=0)
    graph.set_points_as_corners([position(start + (end - start) * i / 64, log) for i in range(65)])
    return graph


def step_bracket(index, log=False):
    _, low, _ = position(index - 1, log)
    x, high, _ = position(index, log)
    x += 0.28
    return VGroup(
        Line((x, low, 0), (x, high, 0), color=STEP, stroke_width=2),
        Line((x - 0.09, low, 0), (x + 0.09, low, 0), color=STEP, stroke_width=2),
        Line((x - 0.09, high, 0), (x + 0.09, high, 0), color=STEP, stroke_width=2),
    )


class DoublingCurve(MovingCameraScene):
    def construct(self):
        if not 1.2 <= GROWTH_FACTOR <= 3:
            raise ValueError("Choose a growth factor between 1.2 and 3.")
        multiplier = f"×{GROWTH_FACTOR:g}"
        frame = self.camera.frame
        frame.save_state()
        background = Rectangle(width=28, height=20, fill=NIGHT, fill_opacity=1, stroke=None)
        title = text("Why does this growth curve bend?", 42, 0, 3.13)
        scale = text("LINEAR SCALE", 25, 5.0, 2.2, MUTED)
        caption = text(f"Every period multiplies the value by {GROWTH_FACTOR:g}", 30, 0, -3.46)
        periods = text("PERIOD", 23, 4.84, -2.49, MUTED)
        lines, values, samples = [], [], []
        for i in range(STEPS + 1):
            x, y, _ = position(i)
            lines.append(Line((X0, y, 0), (X0 + WIDTH, y, 0), color=GRID, stroke_width=1.4))
            values.append(text(f"{GROWTH_FACTOR ** i:.3g}", 24, X0 - 0.58, y, MUTED))
            samples.append(Dot((x, y, 0), radius=0.085, color=DATA))
        ticks = [text(str(i), 24, position(i)[0], -2.49, MUTED) for i in range(STEPS + 1)]
        axis = Line((X0, Y0, 0), (X0, Y0 + HEIGHT, 0), color=MUTED, stroke_width=2)
        graph = curve()
        self.add(background, *lines, *values, *ticks, periods, title, scale)
        self.next_section("The curve")
        self.play(Create(axis), Create(graph), run_time=1.2)
        self.play(*(FadeIn(dot) for dot in samples), FadeIn(caption), run_time=0.5)
        self.wait(2.2)

        self.next_section("Follow the same ratio")
        marker = Dot(position(0), radius=0.12, color=STEP)
        self.play(FadeIn(marker), FadeOut(title), FadeOut(caption), FadeOut(scale),
                  frame.animate.scale(0.53).move_to((-2.7, -1.66, 0)), run_time=1)
        notes = []
        for i in range(1, STEPS):
            x, y, _ = position(i)
            middle = (y + position(i - 1)[1]) / 2
            note = text(multiplier, 25, x + 0.77, middle, STEP)
            bracket = step_bracket(i)
            segment = curve(start=i - 1, end=i).set_stroke(opacity=0)
            self.add(segment)
            self.play(MoveAlongPath(marker, segment),
                      frame.animate.move_to((x - 0.9, y + 0.12, 0)), run_time=0.7)
            self.remove(segment)
            self.play(Create(bracket), Write(note), run_time=0.4)
            notes.extend([bracket, note])
            self.wait(0.5)

        summary = text(f"Always {multiplier} · each jump gets taller", 30, 0, -3.46)
        last_note = text(multiplier, 25, position(STEPS)[0] + 0.77, position(STEPS)[1] - 0.45, STEP)
        last_bracket = step_bracket(STEPS)
        segment = curve(start=STEPS - 1).set_stroke(opacity=0)
        self.add(segment)
        self.play(Restore(frame), MoveAlongPath(marker, segment),
                  FadeIn(title), FadeIn(scale), run_time=1.1)
        self.remove(segment)
        self.play(Create(last_bracket), Write(last_note), FadeIn(summary), run_time=0.5)
        self.wait(2)
        self.play(FadeOut(title), FadeOut(summary), run_time=0.3)
        puzzle = text(f"What if {multiplier} always took the same height?", 37, 0, 3.13)
        self.play(Write(puzzle), run_time=1.0)
        self.wait(1.8)

        self.next_section("Change the scale")
        self.play(*(FadeOut(item) for item in [scale, *notes, last_bracket, last_note]), run_time=0.5)
        # The curve and scale markers retain their horizontal coordinates.
        log_scale = text("LOG SCALE", 25, 5.0, 2.2, DATA)
        motions = []
        for i, (line, value, sample) in enumerate(zip(lines, values, samples)):
            x, y, _ = position(i, log=True)
            motions.extend([
                line.animate.move_to((X0 + WIDTH / 2, y, 0)),
                value.animate.move_to((X0 - 0.58, y, 0)),
                sample.animate.move_to((x, y, 0)),
            ])
        self.play(Transform(graph, curve(log=True)), *motions, run_time=2.4, rate_func=smooth)
        self.play(Write(log_scale), run_time=0.6)
        self.wait(2)

        self.next_section("Read the straight line")
        self.play(FadeOut(puzzle), run_time=0.3)
        result = text("The same data draws a straight line", 42, 0, 3.13)
        takeaway = text("Equal ratios become equal vertical steps", 30, 0, -3.46)
        self.play(Write(result), run_time=0.9)
        for i in range(1, STEPS + 1):
            x, y, _ = position(i, log=True)
            ratio = text(multiplier, 25, x + 0.77, y - HEIGHT / (2 * STEPS), STEP)
            self.play(Create(step_bracket(i, log=True)), Write(ratio), run_time=0.4)
        self.play(FadeIn(takeaway), run_time=0.5)
        self.wait(3.7)
