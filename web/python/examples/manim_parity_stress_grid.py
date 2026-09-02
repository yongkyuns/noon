from noon import *


class MixedObjectParityStress(Scene):
    def construct(self):
        rows = 20
        cols = 30
        palette = [BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE]
        font = "DejaVu Sans Mono"

        shape_count = rows * cols
        title = Text(
            "MANIM PARITY STRESS",
            font=font,
            font_size=30,
            color=WHITE,
        ).shift(3.45 * UP)
        subtitle = Text(
            f"{shape_count} shapes | 200 churn | repeated morph + motion + color",
            font=font,
            font_size=14,
            color=GRAY,
        ).shift(3.02 * UP)

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

        self.play(FadeIn(title), FadeIn(subtitle), run_time=0.25)
        self.play(*[Create(shape) for shape in shapes], run_time=0.55)

        self.play(
            *[
                Transform(shape, target)
                for shape, target in zip(shapes, targets_a)
            ],
            run_time=0.75,
        )

        motion_a = []
        for index, shape in enumerate(shapes):
            row = index // cols
            angle = PI / 3 if index % 2 == 0 else -PI / 3
            x_direction = RIGHT if row % 2 == 0 else LEFT
            y_direction = UP if index % 3 == 0 else DOWN
            color = palette[(index + 5) % len(palette)]
            motion_a.append(
                shape.animate.rotate(angle)
                .shift(0.035 * x_direction + 0.025 * y_direction)
                .set_color(color)
            )
        self.play(*motion_a, title.animate.rotate(PI / 24), run_time=0.65)

        self.play(
            *[
                Transform(shape, target)
                for shape, target in zip(shapes, targets_b)
            ],
            run_time=0.75,
        )

        motion_b = []
        for index, shape in enumerate(shapes):
            row = index // cols
            angle = -PI / 2 if index % 2 == 0 else PI / 2
            x_direction = LEFT if row % 2 == 0 else RIGHT
            y_direction = DOWN if index % 3 == 0 else UP
            color = palette[(index + 1) % len(palette)]
            motion_b.append(
                shape.animate.rotate(angle)
                .shift(0.055 * x_direction + 0.035 * y_direction)
                .set_color(color)
            )
        self.play(*motion_b, title.animate.rotate(-PI / 12), run_time=0.65)

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

        self.play(FadeOut(subtitle), run_time=0.10)
        self.play(
            *[FadeOut(shape, scale=0.25) for shape in leaving],
            *[FadeIn(pulse, scale=0.15) for pulse in pulses],
            run_time=0.50,
        )

        self.play(
            FadeIn(subtitle),
            title.animate.rotate(PI / 24),
            run_time=0.10,
        )
        self.play(
            *[FadeIn(shape, scale=0.25) for shape in leaving],
            *[FadeOut(pulse, scale=2.0) for pulse in pulses],
            run_time=0.70,
        )
