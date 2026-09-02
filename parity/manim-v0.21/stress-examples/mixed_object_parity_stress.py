from manim import *


class MixedObjectParityStress(Scene):
    def construct(self):
        # Keep every animation on Manim's 30 FPS frame grid so the logical
        # duration remains the five seconds declared by the parity manifest.
        frame = 1 / 30
        rows = 20
        cols = 30
        palette = [BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE]
        font = "DejaVu Sans Mono"

        shape_count = rows * cols
        title = Text(
            "NOON DYNAMIC LOAD",
            font=font,
            font_size=30,
            color=WHITE,
        ).shift(3.45 * UP)
        subtitle = Text(
            f"{shape_count} GEOMETRY  ·  24 TEXT STREAMS  ·  STAGGERED TRANSFORMS",
            font=font,
            font_size=13,
            color=GRAY,
        ).shift(3.02 * UP)

        labels = []
        for index in range(24):
            slot = index % 12
            side = -1 if index < 12 else 1
            label = Text(
                f"S{index:02d} {((index * 73 + 19) % 997):03d}",
                font=font,
                font_size=9,
                color=palette[(index * 3 + 2) % len(palette)],
            )
            label.shift(
                (6.32 * side) * RIGHT
                + (2.58 - slot * 0.47) * UP
            )
            labels.append(label)

        shapes = []
        targets_a = []
        targets_b = []
        x_spacing = 0.38
        y_spacing = 0.255
        x_origin = -0.5 * (cols - 1) * x_spacing
        y_origin = 2.40

        for row in range(rows):
            for col in range(cols):
                index = row * cols + col
                color = palette[index % len(palette)]
                color_a = palette[(index + 3) % len(palette)]
                color_b = palette[(index + 6) % len(palette)]
                point = (x_origin + col * x_spacing) * RIGHT + (
                    y_origin - row * y_spacing
                ) * UP

                if (row + col) % 2 == 0:
                    shape = Square(
                        side_length=0.18,
                        fill_color=color,
                        fill_opacity=0.58,
                        stroke_color=color,
                        stroke_width=1,
                    )
                    target_a = Circle(
                        radius=0.105,
                        fill_color=color_a,
                        fill_opacity=0.90,
                        stroke_color=color_a,
                        stroke_width=1,
                    )
                    target_b = Square(
                        side_length=0.20,
                        fill_color=color_b,
                        fill_opacity=0.72,
                        stroke_color=color_b,
                        stroke_width=1,
                    ).rotate(PI / 4)
                else:
                    shape = Circle(
                        radius=0.09,
                        fill_color=color,
                        fill_opacity=0.58,
                        stroke_color=color,
                        stroke_width=1,
                    )
                    target_a = Square(
                        side_length=0.20,
                        fill_color=color_a,
                        fill_opacity=0.90,
                        stroke_color=color_a,
                        stroke_width=1,
                    ).rotate(PI / 4)
                    target_b = Circle(
                        radius=0.105,
                        fill_color=color_b,
                        fill_opacity=0.72,
                        stroke_color=color_b,
                        stroke_width=1,
                    )

                offset = 0.025 if row % 2 == 0 else -0.025
                shape.move_to(point)
                target_a.move_to(point + offset * RIGHT)
                target_b.move_to(point - offset * RIGHT)
                shapes.append(shape)
                targets_a.append(target_a)
                targets_b.append(target_b)

        self.play(
            FadeIn(title),
            FadeIn(subtitle),
            *[FadeIn(label) for label in labels],
            run_time=11 * frame,
        )
        self.play(*[Create(shape) for shape in shapes], run_time=17 * frame)

        self.play(
            *[
                Transform(shape, target)
                for shape, target in zip(shapes, targets_a)
            ],
            run_time=16 * frame,
        )

        for wave in range(6):
            wave_motion = []
            for index, shape in enumerate(shapes):
                row = index // cols
                col = index % cols
                bucket = (index * 37 + row * 11 + col * 7) % 6
                if bucket != wave:
                    continue
                scale_factor = 0.84 + 0.04 * ((index * 13 + row) % 9)
                angle = (((index * 19 + col) % 11) - 5) * (PI / 36)
                dx = (((index * 17 + row) % 9) - 4) * 0.027
                dy = (((index * 29 + col) % 7) - 3) * 0.023
                color = palette[(index + wave + 3) % len(palette)]
                wave_motion.append(
                    shape.animate.scale(scale_factor)
                    .rotate(angle)
                    .shift(dx * RIGHT + dy * UP)
                    .set_color(color)
                )
            if wave_motion:
                self.play(*wave_motion, run_time=2 * frame)

        label_specs = []
        for index in range(len(labels)):
            factor = 0.90 + 0.04 * ((index * 5 + 1) % 6)
            angle = (((index * 7) % 9) - 4) * (PI / 72)
            dx = (((index * 11) % 5) - 2) * 0.045
            dy = (((index * 13) % 5) - 2) * 0.025
            opacity = 0.62 + 0.09 * (index % 5)
            label_specs.append((factor, angle, dx, dy, opacity))

        for wave in range(4):
            text_motion = []
            for index, label in enumerate(labels):
                if index % 4 != wave:
                    continue
                factor, angle, dx, dy, opacity = label_specs[index]
                text_motion.append(
                    label.animate.scale(factor)
                    .rotate(angle)
                    .shift(dx * RIGHT + dy * UP)
                    .set_opacity(opacity)
                )
            self.play(*text_motion, run_time=2 * frame)

        self.play(
            *[
                Transform(shape, target)
                for shape, target in zip(shapes, targets_b)
            ],
            run_time=16 * frame,
        )

        turbulence = []
        for index, shape in enumerate(shapes):
            row = index // cols
            col = index % cols
            factor = 0.92 + 0.02 * ((index * 7 + col) % 9)
            angle = (PI / 3) if (index + row) % 2 == 0 else (-PI / 3)
            dx = (((index * 23 + col) % 7) - 3) * 0.022
            dy = (((index * 31 + row) % 7) - 3) * 0.020
            color = palette[(index * 5 + 1) % len(palette)]
            turbulence.append(
                shape.animate.scale(factor)
                .rotate(angle)
                .shift(dx * RIGHT + dy * UP)
                .set_color(color)
            )
        self.play(*turbulence, run_time=13 * frame)

        for wave in range(4):
            text_motion = []
            for index, label in enumerate(labels):
                if index % 4 != wave:
                    continue
                factor, angle, dx, dy, _ = label_specs[index]
                text_motion.append(
                    label.animate.scale(1.0 / factor)
                    .rotate(-angle)
                    .shift(-dx * RIGHT - dy * UP)
                    .set_opacity(1.0)
                )
            self.play(*text_motion, run_time=2 * frame)

        leaving = shapes[::3]
        pulses = [
            Circle(
                radius=0.045,
                fill_color=WHITE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(shape.get_center())
            for shape in leaving
        ]
        blinking_labels = labels[::3]

        self.play(
            FadeOut(subtitle),
            *[FadeOut(label) for label in blinking_labels],
            run_time=0.10,
        )
        self.play(
            *[FadeOut(shape, scale=0.25) for shape in leaving],
            *[FadeIn(pulse, scale=0.15) for pulse in pulses],
            run_time=0.40,
        )

        self.play(
            FadeIn(subtitle),
            *[FadeIn(label) for label in blinking_labels],
            run_time=0.10,
        )
        self.play(
            *[FadeIn(shape, scale=0.25) for shape in leaving],
            *[FadeOut(pulse, scale=2.0) for pulse in pulses],
            run_time=13 * frame,
        )

        for wave in range(6):
            wave_motion = []
            for index, shape in enumerate(shapes):
                row = index // cols
                col = index % cols
                bucket = (index * 17 + row * 5 + col * 13) % 6
                if bucket != wave:
                    continue
                scale_factor = 0.90 + 0.025 * ((index * 11 + col) % 9)
                angle = (((index * 5 + row) % 13) - 6) * (PI / 48)
                dx = (((index * 41 + col) % 11) - 5) * 0.020
                dy = (((index * 43 + row) % 9) - 4) * 0.018
                color = palette[(index + 2 * wave + 5) % len(palette)]
                wave_motion.append(
                    shape.animate.scale(scale_factor)
                    .rotate(angle)
                    .shift(dx * RIGHT + dy * UP)
                    .set_color(color)
                )
            if wave_motion:
                self.play(*wave_motion, run_time=3 * frame)
