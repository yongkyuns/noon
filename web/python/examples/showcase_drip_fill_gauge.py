# Copyright (c) 2026 Trimora Inc.
# SPDX-License-Identifier: MIT
# Original: https://rendercomp.com/free/
# https://github.com/RenderComp/free-remotion-templates/blob/f648980e528defb7594e24c6447055f0540f340b/src/components/pourover-drip-fill-gauge/PouroverDripFillGauge.tsx
# Bitmap glyphs, particles, and CRT treatment from src/pixel-kit at the same revision.
# See THIRD_PARTY_NOTICES.md.
"""RenderComp's amber pour-over gauge, driven by one authored progress value."""

from math import exp, floor, sin

from noon import *


FRAME_COUNT = 180
FPS = 30
RISE_FRAMES = 140  # The upstream default is round(180 * 0.78).
AMBER_BACKGROUND, AMBER_DARK, AMBER_MID, AMBER_LIGHT = "#1c1206", "#6b4410", "#c88a1e", "#f6d27a"
PIXELS_PER_UNIT = 33.75
CARAFE_TOP, CARAFE_BOTTOM = 122, 214
GAUGE_X, GAUGE_Y = 300, 236

# Only the glyphs present in the original scene.
GLYPHS = {
    "0": ("01110", "10011", "10101", "10101", "11001", "10001", "01110"),
    "1": ("00100", "01100", "00100", "00100", "00100", "00100", "01110"),
    "2": ("01110", "10001", "00001", "00110", "01000", "10000", "11111"),
    "3": ("11110", "00001", "00001", "01110", "00001", "00001", "11110"),
    "4": ("00010", "00110", "01010", "10010", "11111", "00010", "00010"),
    "5": ("11111", "10000", "11110", "00001", "00001", "10001", "01110"),
    "6": ("00110", "01000", "10000", "11110", "10001", "10001", "01110"),
    "7": ("11111", "00001", "00010", "00100", "01000", "01000", "01000"),
    "8": ("01110", "10001", "10001", "01110", "10001", "10001", "01110"),
    "9": ("01110", "10001", "10001", "01111", "00001", "00010", "01100"),
    "%": ("11001", "11010", "00010", "00100", "01000", "01011", "10011"),
    "E": ("11111", "10000", "10000", "11110", "10000", "10000", "11111"),
    "F": ("11111", "10000", "10000", "11110", "10000", "10000", "10000"),
    "L": ("10000", "10000", "10000", "10000", "10000", "10000", "11111"),
    "M": ("10001", "11011", "10101", "10101", "10001", "10001", "10001"),
    "O": ("01110", "10001", "10001", "10001", "10001", "10001", "01110"),
    "U": ("10001", "10001", "10001", "10001", "10001", "10001", "01110"),
    "V": ("10001", "10001", "10001", "10001", "10001", "01010", "00100"),
}

CRT = """
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let uv = vec2<f32>(0.5 + input.position.x * 9.0 / 128.0, 0.5 - input.position.y / 8.0);
    let xy = vec2<u32>(floor(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.99999)) * vec2<f32>(480.0, 270.0)));
    let row = xy.y % 4u;
    let column = xy.x % 4u;
    let dot = (row == 0u && column % 2u == 0u)
           || (row == 1u && column == 1u)
           || (row == 2u && column % 2u == 0u)
           || (row == 3u && column == 3u);
    let beam = (1.0 - uv.y) * 1080.0;
    let scan = select(1.0, 1.0 - 0.42 * 0.32, beam - 5.0 * floor(beam / 5.0) < 2.0);
    let distance = length((uv - vec2<f32>(0.5, 0.48)) / vec2<f32>(0.78, 0.72));
    let vignette = 1.0 - 0.75 * clamp((distance - 0.52) / 0.48, 0.0, 1.0);
    let darken = select(1.0, 0.92, dot) * scan;
    // A frame-seeded grain at the original opacity. The SVG turbulence field
    // is represented by a deterministic shader rather than a browser filter.
    let f = floor(min(179.0, p.frame));
    let noise = fract(sin(dot_product(floor(uv * vec2<f32>(1920.0, 1080.0)), f)) * 43758.5453);
    let grain = (0.25 + noise * 0.5) * 0.035 * 0.35;
    let alpha = 1.0 - darken * vignette * (1.0 - grain);
    let rgb = vec3<f32>(0.6, 0.9, 1.0) * (grain * vignette / max(alpha, 0.00001));
    return vec4<f32>(rgb, alpha);
}
fn dot_product(pixel: vec2<f32>, frame: f32) -> f32 {
    return dot(pixel, vec2<f32>(12.9898, 78.233)) + frame * 0.823;
}
"""


def snap(value):
    return floor(value + 0.5)


def progress(frame):
    t = min(1, frame / RISE_FRAMES)
    return t * t * (3 - 2 * t)


def pulse(frame):
    beat_length = 60 / 96 * FPS
    phase = frame % beat_length
    envelope = (phase + 1) / 3 if phase < 2 else exp(-(phase-2)/(beat_length * 0.45))
    return min(1, envelope * 0.85 + 0.08 * (0.5 + 0.5 * sin(frame * 0.3)))


def carafe_half_width(y):
    return 38 + 16 * (y - CARAFE_TOP) / (CARAFE_BOTTOM - CARAFE_TOP)


def fraction(value):
    return value - floor(value)


def drop(index, frame):
    x_seed = fraction(sin((index * 2.3 + 4) * 127.1 + 311.7) * 43758.5453)
    y_seed = fraction(sin((index * 1.7 + 15) * 91.3 + 47.1) * 31873.7)
    x = snap(x_seed * 14 + sin(frame * 0.1 + x_seed * 9))
    y = snap((60 * y_seed + frame * 3.2 * (0.6 + y_seed * 0.8)) % 60 - 10)
    return (232 + x, 30 + y, snap(1 + x_seed) + 1)


def pixel(x, y):
    return ((x - 240) / PIXELS_PER_UNIT, (135 - y) / PIXELS_PER_UNIT, 0)


def box(x, y, w, h, color, opacity=1):
    return Rectangle(width=w / PIXELS_PER_UNIT, height=h / PIXELS_PER_UNIT,
                     fill=color, fill_opacity=opacity, stroke=None).move_to(pixel(x + w/2, y + h/2))


def boxes(rows, color, opacity=1):
    shape = VMobject(fill_color=color, fill_opacity=opacity, stroke_width=0)
    for x, y, w, h in rows:
        shape.start_new_path(pixel(x, y))
        shape.add_line_to(pixel(x + w, y))
        shape.add_line_to(pixel(x + w, y + h))
        shape.add_line_to(pixel(x, y + h))
        shape.close_path()
    return shape


def glyph_cells(text, x, y, cell):
    return [(x + (6 * index + column) * cell, y + row * cell, cell, cell)
            for index, char in enumerate(text)
            for row, bits in enumerate(GLYPHS[char])
            for column, bit in enumerate(bits) if bit == "1"]


def percent_text(value):
    text = str(snap(value * 100)) + "%"
    return text + " " * (4 - len(text))


class DripFillGauge(Scene):
    def moving_box(self, clock, frames, color, opacity=1):
        x, y, w, h = frames[0]
        plate = box(x, y, max(w, 1), h, color, opacity)
        self.add(plate)
        self.bind_sampled(plate, clock, "position",
                          [(i, (x + w/2 - 240) / PIXELS_PER_UNIT, (135 - y - h/2) / PIXELS_PER_UNIT)
                           for i, (x, y, w, h) in enumerate(frames)], linear=False)
        self.bind_sampled(plate, clock, "scale",
                          [(i, max(0.0001, width/max(w, 1)), height/h)
                           for i, (_, _, width, height) in enumerate(frames)], linear=False)
        return plate

    def readout(self, clock, values, pulses):
        for position in range(4):
            for char in sorted(set(text[position] for text in values)):
                if char == " ":
                    continue
                cells = glyph_cells(char, 18 + 24 * position, 230, 4)
                base = boxes(cells, AMBER_LIGHT)
                highlight = boxes(cells, WHITE)
                self.add(base, highlight)
                self.bind_sampled(base, clock, "opacity",
                                  [(i, int(text[position] == char)) for i, text in enumerate(values)],
                                  linear=False)
                self.bind_sampled(highlight, clock, "opacity",
                                  [(i, pulses[i] * 0.5 * int(text[position] == char))
                                   for i, text in enumerate(values)], linear=False)

    def construct(self):
        clock = self.value_tracker(0)
        progresses = [progress(frame) for frame in range(FRAME_COUNT)]
        pulses = [pulse(frame) for frame in range(FRAME_COUNT)]
        surfaces = [snap(CARAFE_BOTTOM - (CARAFE_BOTTOM-CARAFE_TOP) * value)
                    for value in progresses]

        self.add(box(0, 0, 480, 270, AMBER_BACKGROUND), box(0, 240, 480, 30, "#38240a"),
                 box(170, 216, 140, 5, "#110b04", 0.6),
                 boxes([(224, 12, 32, 16), (256, 16, 8, 4)], AMBER_DARK),
                 boxes([(224, 12, 32, 3), (236, 28, 8, 4)], AMBER_MID))
        for index in range(7):
            poses = [drop(index, frame) for frame in range(FRAME_COUNT)]
            self.moving_box(clock, [(x, y, 1, h) for x, y, h in poses], AMBER_MID, 0.85)

        cone_light, cone_dark, cone_inside = [], [], []
        for index in range(23):
            y = 64 + 2 * index
            width = snap(56 * (1 - (y - 64)/46) + 6 * (y - 64)/46)
            walls = cone_light if index % 3 == 0 else cone_dark
            walls.extend(((240-width, y, 2, 2), (240+width-2, y, 2, 2)))
            cone_inside.append((242-width, y, 2 * width-4, 2))
        self.add(boxes(cone_light, AMBER_MID), boxes(cone_dark, AMBER_DARK),
                 boxes(cone_inside, "#472e0c", 0.5), box(184, 62, 112, 2, AMBER_LIGHT))
        stream = box(239, 110, 2, 14, AMBER_MID)
        self.add(stream)
        self.bind_sampled(stream, clock, "opacity",
                          [(i, 0.7 + alpha * 0.3) for i, alpha in enumerate(pulses)], linear=False)
        self.add(boxes([(225, 113, 30, 2), (225, 121, 30, 2),
                        (225, 115, 2, 6), (253, 115, 2, 6)], AMBER_DARK))

        # For the default amber palette the two source band colors quantize
        # to the same mid tone. Preserve the alternating two-pixel row phase.
        for y in range(CARAFE_TOP, CARAFE_BOTTOM):
            width = snap(carafe_half_width(y)) - 4
            row = box(240-width, y, 2 * width, 2, AMBER_MID)
            self.add(row)
            self.bind_sampled(row, clock, "opacity",
                              [(i, int(y >= surface and (y-surface) % 2 == 0))
                               for i, surface in enumerate(surfaces)], linear=False)
        meniscus_poses = []
        for i, surface in enumerate(surfaces):
            width = snap(carafe_half_width(surface)) - 3
            slosh = snap(sin(i * 0.18) * (0.6 + pulses[i]))
            meniscus_poses.append((240-width, surface + slosh, 2 * width, 2))
        meniscus = self.moving_box(clock, meniscus_poses, AMBER_LIGHT)
        self.bind_sampled(meniscus, clock, "opacity",
                          [(i, 0.9 * int(value > 0.01)) for i, value in enumerate(progresses)],
                          linear=False)
        walls, highlights = [], []
        for y in range(CARAFE_TOP, CARAFE_BOTTOM + 1, 2):
            width = snap(carafe_half_width(y))
            walls.extend(((240-width, y, 2, 2), (238+width, y, 2, 2)))
            if ((y-CARAFE_TOP)//2) % 3 == 0:
                highlights.append((243-width, y, 1, 2))
        self.add(boxes(walls, AMBER_DARK), boxes(highlights, AMBER_MID, 0.4), box(186, 214, 108, 3, AMBER_DARK))

        self.readout(clock, [percent_text(value) for value in progresses], pulses)
        self.add(boxes(glyph_cells("VOLUME", 18, 218, 1), AMBER_MID),
                 boxes(glyph_cells("FULL", 427, GAUGE_Y-11, 1), AMBER_MID),
                 boxes([(299.5, 235.5, 151, 1), (299.5, 243.5, 151, 1),
                        (299.5, 236.5, 1, 7), (449.5, 236.5, 1, 7)], AMBER_DARK))
        fill_widths = [max(0, snap(150 * value) - 2) for value in progresses]
        fill = self.moving_box(clock, [(GAUGE_X + 1, GAUGE_Y + 1, width, 6)
                                      for width in fill_widths], AMBER_MID)
        self.bind_sampled(fill, clock, "opacity",
                          [(i, int(width > 0)) for i, width in enumerate(fill_widths)], linear=False)
        self.add(boxes([(GAUGE_X + snap(150 * fraction), GAUGE_Y - 2, 1, 2)
                        for fraction in (0.25, 0.5, 0.75)], AMBER_LIGHT, 0.7))

        glass = box(0, 0, 480, 270, WHITE)
        program = WgslEffectProgram(CRT, coordinates="scene", parameters=[
            ShaderParameter.scalar("frame", default=0, minimum=0, maximum=180),
        ])
        frame = program.parameter("frame")
        glass.add_effect(ShaderEffect(program), name="crt")
        self.add(glass)
        self.play(clock.animate.set_value(RISE_FRAMES),
                  glass.animate.set_effect("crt", parameters={frame: 140}),
                  run_time=RISE_FRAMES / FPS, rate_func=linear)
        self.play(clock.animate.set_value(FRAME_COUNT),
                  glass.animate.set_effect("crt", parameters={frame: 180}),
                  run_time=(FRAME_COUNT - RISE_FRAMES) / FPS, rate_func=linear)
