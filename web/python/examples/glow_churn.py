"""One thousand authored attach/remove cycles; paired with Rust glow_churn."""
from noon import *


class GlowChurn(MovingCameraScene):
    def construct(self):
        self.camera.frame.scale(6 / DEFAULT_FRAME_HEIGHT)
        dots = []
        for index in range(600):
            dot = Circle(0.05, color=WHITE, fill_opacity=1).set_stroke(width=0)
            dot.move_to([-3.75 + (index % 30) * 0.25, -2.375 + (index // 30) * 0.25, 0])
            dots.append(dot)
        self.add(*dots)
        self.wait(1 / 60)
        for _ in range(10):
            for dot in dots[:100]:
                dot.set_glow(radius=Pixels(3), intensity=1)
            self.wait(26 / 60)
            for dot in dots[:100]:
                dot.remove_glow()
            self.wait(26 / 60)
        self.wait(10 - self.time)
