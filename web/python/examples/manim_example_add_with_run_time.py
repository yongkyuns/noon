from noon import *


class AddWithRunTimeScene(Scene):
    def construct(self):
        # A 5x5 grid of circles
        circles = VGroup(
            *[Circle(radius=0.5) for _ in range(25)]
        ).arrange_in_grid(5, 5)

        self.play(
            Succession(
                # Add a run_time of 0.2 to wait for 0.2 seconds after
                # adding the circle, instead of using Wait(0.2) after Add!
                *[Add(circle, run_time=0.2) for circle in circles],
                rate_func=smooth,
            )
        )
        self.wait()
