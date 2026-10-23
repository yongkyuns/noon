"""A finite 30-second lesson with the original score 'A Small Discovery'.

Paired with noon::example_scenes::background_music_lesson. Every point and music
clip is declared before execution; Rust owns interpolation, seeking and audio.
"""
from noon import *


def tint(rgb):
    return Color((rgb >> 16 & 255) / 255, (rgb >> 8 & 255) / 255, (rgb & 255) / 255)


INK = tint(0xECF2F5)
MUTED = tint(0x9CAFB9)
GUIDE = tint(0x607884)
PANEL = tint(0x13232D)
INPUT_BLUE = tint(0x58C4DD)
OUTPUT_GOLD = tint(0xF2C66D)
ORIGIN_X, INPUT_Y, OUTPUT_Y = 1.0, 0.75, -1.1

result = Scene()


def label(source, size, x, y, color=INK):
    result.add(Text(source, font="DejaVu Sans Mono", font_size=size, color=color).move_to((x, y, 0)))


label("Double the input", 42, 0, 3.05)
label("One rule connects two moving points:  y = 2x", 23, 0, 2.45, MUTED)
for name, symbol, y, color in [("INPUT", "x", INPUT_Y, INPUT_BLUE), ("OUTPUT", "2x", OUTPUT_Y, OUTPUT_GOLD)]:
    panel = Rectangle(width=12.5, height=1.6, stroke_width=0).set_fill(PANEL, opacity=1)
    result.add(panel.move_to((0, y, 0)))
    label(name, 18, -4.8, y + 0.22, color)
    label(symbol, 32, -4.8, y - 0.22)
    result.add(Line((-3, y, 0), (5, y, 0), color=GUIDE, stroke_width=2.5))
    for value in range(-4, 5):
        x = ORIGIN_X + value
        tick_color = INK if value == 0 else MUTED
        result.add(Line((x, y - 0.1, 0), (x, y + 0.1, 0), color=tick_color, stroke_width=2))
        label(str(value), 18, x, y - 0.43, tick_color)

label("Same sign. Twice the distance from zero.", 24, 0, -2.25)
for x, heading, explanation in [
    (-4.3, "01  POSITIVE", "2 becomes 4"),
    (0, "02  NEGATIVE", "-2 becomes -4"),
    (4.3, "03  ZERO", "0 stays 0"),
]:
    label(heading, 18, x, -2.95, INPUT_BLUE)
    label(explanation, 18, x, -3.35, MUTED)

input_point = Circle(radius=0.18, stroke_width=0).set_fill(INPUT_BLUE, opacity=1)
output_point = Circle(radius=0.18, stroke_width=0).set_fill(OUTPUT_GOLD, opacity=1)
result.add(input_point.move_to((ORIGIN_X, INPUT_Y, 0)), output_point.move_to((ORIGIN_X, OUTPUT_Y, 0)))

x = result.value_tracker(0)
result.bind_position(input_point, x, direction=RIGHT, offset=(ORIGIN_X, INPUT_Y, 0))
result.bind_position(output_point, x, direction=2 * RIGHT, offset=(ORIGIN_X, OUTPUT_Y, 0))

result.next_section("Double a positive input")
result.declare_wait(1)
result.declare_value(x, 2, run_time=8, rate_func=linear)
result.declare_wait(1)

result.next_section("Keep the sign")
result.declare_wait(1)
result.declare_value(x, -2, run_time=8, rate_func=linear)
result.declare_wait(1)

result.next_section("Return to zero")
result.declare_wait(1)
result.declare_value(x, 0, run_time=8, rate_func=linear)
result.declare_wait(1)

result.audio(AudioSource.small_discovery(), duration=30, gain=0.5, fade_in=1.5, fade_out=2.5)
