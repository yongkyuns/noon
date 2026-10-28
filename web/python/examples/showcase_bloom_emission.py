"""Material brightness and camera spread are independent ordinary controls."""
from noon import *


class BloomEmission(MovingCameraScene):
    def construct(self):
        camera = self.camera.frame
        camera.set_view_color_profile("linear_hdr")
        camera.add_effect(Bloom(radius=Pixels(12), threshold=1.2, intensity=0), name="bloom")
        muted = Color(0.62, 0.69, 0.82)
        self.add(Rectangle(width=16, height=9, fill=Color(0.035, 0.045, 0.09),
                           fill_opacity=1, stroke=None))
        self.add(Text("BRIGHTNESS AND BLOOM", font_size=36, color=WHITE).move_to((0, 3.1, 0)))
        self.add(Text("Make the source bright. Then spread its light.", font_size=20, color=muted)
                 .move_to((0, 2.55, 0)))
        self.add(Text("MATERIAL", font_size=20, color=muted).move_to((-4.1, 0.65, 0)))
        self.add(Text("Emission: 0 → 7 → 0", font_size=19, color=WHITE).move_to((-4.1, 0, 0)))
        self.add(Text("CAMERA", font_size=20, color=muted).move_to((4.1, 0.65, 0)))
        self.add(Text("Bloom: 0 → 3 → 0", font_size=19, color=WHITE).move_to((4.1, 0, 0)))
        source = Circle(radius=0.85, fill=Color(0.12, 0.7, 1), fill_opacity=1, stroke=None)
        self.add(source)
        self.add(Text("Radiance first. Bloom second. Then restore.", font_size=21, color=WHITE)
                 .move_to((0, -2.3, 0)))
        self.add(Text("The camera treatment leaves the source geometry editable.", font_size=18, color=muted)
                 .move_to((0, -2.9, 0)))
        self.wait(0.75)
        self.play(source.animate.set_emission(7), run_time=1.5, rate_func=linear)
        self.wait(0.75)
        self.play(camera.animate.set_effect("bloom", intensity=3), run_time=1.5, rate_func=linear)
        self.wait(0.75)
        self.play(source.animate.set_emission(0), camera.animate.set_effect("bloom", intensity=0),
                  run_time=1.5, rate_func=linear)
        self.wait(0.75)
