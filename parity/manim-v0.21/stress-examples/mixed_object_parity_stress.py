from manim import *


class MixedObjectParityStress(Scene):
    def construct(self):
        rows = 6
        cols = 12
        palette = [BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE]
        font = "DejaVu Sans Mono"

        title = Text(
            "MANIM PARITY STRESS",
            font=font,
            font_size=34,
            color=WHITE,
        ).shift(3.25 * UP)
        subtitle = Text(
            "72 shapes | morph | rotate | color | text | lifecycle",
            font=font,
            font_size=17,
            color=GRAY_B,
        ).shift(2.72 * UP)

        shapes = []
        targets = []
        x_spacing = 0.90
        y_spacing = 0.72
        x_origin = -0.5 * (cols - 1) * x_spacing
        y_origin = 1.52

        for row in range(rows):
            for col in range(cols):
                index = row * cols + col
                color = palette[index % len(palette)]
                target_color = palette[(index + 3) % len(palette)]
                point = (x_origin + col * x_spacing) * RIGHT + (
                    y_origin - row * y_spacing
                ) * UP

                if (row + col) % 2 == 0:
                    shape = Square(
                        side_length=0.42,
                        fill_color=color,
                        fill_opacity=0.58,
                        stroke_color=color,
                        stroke_width=2,
                    )
                    target = Circle(
                        radius=0.23,
                        fill_color=target_color,
                        fill_opacity=0.88,
                        stroke_color=target_color,
                        stroke_width=2,
                    )
                else:
                    shape = Circle(
                        radius=0.21,
                        fill_color=color,
                        fill_opacity=0.58,
                        stroke_color=color,
                        stroke_width=2,
                    )
                    target = Square(
                        side_length=0.46,
                        fill_color=target_color,
                        fill_opacity=0.88,
                        stroke_color=target_color,
                        stroke_width=2,
                    ).rotate(PI / 4)

                shape.move_to(point)
                target.move_to(point + (0.07 if row % 2 == 0 else -0.07) * RIGHT)
                shapes.append(shape)
                targets.append(target)

        self.play(
            Create(title),
            FadeIn(subtitle),
            *[Create(shape) for shape in shapes],
            run_time=1.0,
        )

        self.play(
            *[
                Transform(shape, target)
                for shape, target in zip(shapes, targets)
            ],
            run_time=1.2,
        )

        motion = []
        for index, shape in enumerate(shapes):
            angle = PI / 2 if index % 2 == 0 else -PI / 2
            direction = UP if (index // cols) % 2 == 0 else DOWN
            color = palette[(index + 5) % len(palette)]
            motion.append(
                shape.animate.rotate(angle).shift(0.11 * direction).set_color(color)
            )

        self.play(
            *motion,
            title.animate.rotate(PI / 18),
            run_time=1.2,
        )

        leaving = shapes[::2]
        pulses = [
            Circle(
                radius=0.085,
                fill_color=WHITE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(shape.get_center())
            for shape in leaving
        ]

        self.play(
            *[FadeOut(shape, scale=0.35) for shape in leaving],
            *[FadeIn(pulse, scale=0.2) for pulse in pulses],
            FadeOut(subtitle),
            run_time=0.8,
        )

        self.play(
            *[FadeIn(shape, scale=0.35) for shape in leaving],
            *[FadeOut(pulse, scale=1.8) for pulse in pulses],
            FadeIn(subtitle),
            title.animate.rotate(-PI / 18),
            run_time=0.8,
        )
