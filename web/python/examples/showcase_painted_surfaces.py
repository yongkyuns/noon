"""Animate image, gradient and procedural fills through ordinary paint targets.

The shared Rust paths are also exercised by example_scenes::{image_fill,
radial_gradient, noise_paint, hatch_paint}; no per-frame Python callback is used.
"""
from noon import *


class PaintedSurfaces(Scene):
    def construct(self):
        ink = Color(0.035, 0.045, 0.09)
        muted = Color(0.62, 0.69, 0.82)
        self.add(Rectangle(width=16, height=9, fill=ink, stroke=None))
        self.add(Text("PAINTED SURFACES", font_size=36, color=Color(0.96, 0.97, 1))
                 .move_to((0, 3.22, 0)))
        self.add(Text("One shape. A changing surface.", font_size=20, color=muted)
                 .move_to((0, 2.67, 0)))

        palette = [(24, 79, 112), (46, 181, 181), (244, 204, 132), (238, 119, 108)]
        pixels = [[[*palette[((x + y) // 3) % 4], 255] for x in range(24)]
                  for y in range(12)]
        stops = [(0, Color(1, 0.85, 0.55)), (0.4, Color(0.94, 0.38, 0.48)),
                 (1, Color(0.18, 0.12, 0.37))]
        radial = RadialGradient(stops, center=(0.22, 0.45), radii=(0.7, 0.9))
        moved_radial = RadialGradient(stops, center=(0.78, 0.55), radii=(0.55, 0.8))
        noise = NoisePaint(Color(0.08, 0.22, 0.38), Color(0.38, 0.9, 0.78),
                           seed=7, frequency=(6, 4), phase=(0, 0))
        moved_noise = NoisePaint(Color(0.08, 0.22, 0.38), Color(0.38, 0.9, 0.78),
                                 seed=7, frequency=(14, 7), phase=(0.75, -0.4))
        hatch = HatchPaint(Color(0.83, 0.43, 0.2), Color(0.98, 0.89, 0.7),
                           spacing=0.28, width=0.055, angle=0.3, cross=True)
        moved_hatch = HatchPaint(Color(0.83, 0.43, 0.2), Color(0.98, 0.89, 0.7),
                                 spacing=0.38, width=0.075, angle=1.0,
                                 phase=(0.2, -0.15), cross=True)
        panels = []
        for x, y, title, detail, paint in [
            (-3.05, 1.05, "IMAGE", "Zoom and turn the UVs", ImagePaint(pixels)),
            (3.05, 1.05, "RADIAL", "Move the color field", radial),
            (-3.05, -1.65, "NOISE", "Change scale and phase", noise),
            (3.05, -1.65, "HATCH", "Turn a woven pattern", hatch),
        ]:
            panel = Rectangle(width=5.35, height=1.65, stroke=None).set_fill(paint)
            panel.move_to((x, y, 0))
            panels.append(panel)
            self.add(panel)
            self.add(Text(title, font_size=18, color=Color(0.96, 0.97, 1))
                     .move_to((x, y + 1.08, 0)))
            self.add(Text(detail, font_size=16, color=muted).move_to((x, y - 1.1, 0)))

        self.wait(0.6)
        self.play(
            panels[0].animate.set_fill_image_mapping(
                ImageMapping(translation=(0.12, -0.1), scale=(0.7, 0.85), rotation=0.35)),
            panels[1].animate.set_fill(moved_radial),
            panels[2].animate.set_fill(moved_noise),
            panels[3].animate.set_fill(moved_hatch),
            run_time=2.5, rate_func=smooth,
        )
        self.wait(0.8)
        self.play(
            panels[0].animate.set_fill_image_mapping(ImageMapping()),
            panels[1].animate.set_fill(radial),
            panels[2].animate.set_fill(noise),
            panels[3].animate.set_fill(hatch),
            run_time=2.5, rate_func=smooth,
        )
        self.wait(1.1)
