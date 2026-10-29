"""Paired with Rust camera_bloom: independent radiance and camera spread."""
from noon import Bloom, Circle, Color, MovingCameraScene, Pixels, linear


class CameraBloom(MovingCameraScene):
    def construct(self):
        self.camera.frame.set_view_color_profile("linear_hdr")
        self.camera.frame.add_effect(Bloom(radius=Pixels(4), threshold=0.15, intensity=0), name="bloom")
        panel = Circle(radius=1).set_fill(Color(0.2, 0.4, 0.8), opacity=1).set_stroke(width=0)
        self.add(panel)
        self.wait(0.5)
        self.play(panel.animate.set_emission(7), self.camera.frame.animate.set_effect("bloom", intensity=3), run_time=2, rate_func=linear)
        self.wait(0.5)
        self.play(panel.animate.set_emission(0), self.camera.frame.animate.set_effect("bloom", intensity=0), run_time=2, rate_func=linear)
