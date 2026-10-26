"""Image UVs and opacity, paired with noon::example_scenes::image_fill."""
from noon import Color, ImageMapping, ImagePaint, Rectangle, Scene, linear


class ImageSurface(Scene):
    def construct(self):
        pixels = [[[255, 0, 0, 255], [0, 255, 255, 0], [0, 0, 255, 128]],
                  [[240, 100, 10, 64], [20, 240, 100, 192], [150, 20, 240, 255]]]
        panel = Rectangle(width=4, height=2, stroke=None)
        panel.set_fill(ImagePaint(pixels), opacity=1)
        self.add(panel)
        self.wait(0.5)
        mapping = ImageMapping(translation=(0.173, -0.231), scale=(0.63, 1.37), rotation=0.37)
        self.play(panel.animate.set_fill_image_mapping(mapping, opacity=0.5), run_time=1, rate_func=linear)
        self.wait(0.25)
        panel.set_fill_image_mapping(ImageMapping(translation=(-0.23, 0.17), scale=(-0.71, 0.58), rotation=-0.61), opacity=0.8)
        self.wait(0.25)
        panel.set_fill(Color(0.2, 0.4, 0.8), opacity=1)
        self.wait(0.25)
