"""Animate detail on a composed image, through ordinary shared Rust effects."""
from noon import Pixelate, Pixels, Rectangle, Scene, VGroup, linear

class PixelDetail(Scene):
    def construct(self):
        self.camera.frame.scale(1.125)
        background = Rectangle(width=16,height=9,fill="#183040",fill_opacity=1,stroke=None)
        def source(x):
            a = Rectangle(width=1.2,height=0.8,fill="#ff0000",fill_opacity=0.6,stroke=None).shift((x-0.2,0,0))
            b = Rectangle(width=1.2,height=0.8,fill="#0000ff",fill_opacity=0.5,stroke=None).shift((x+0.2,0,0))
            return VGroup(a,b).set_object_opacity(0.75)
        reference, target = source(-2), source(2)
        target.add_effect(Pixelate(size=Pixels(12)),name="detail")
        self.add(background,reference,target)
        self.wait(0.5)
        self.play(target.animate.set_effect("detail",size=Pixels(1)),run_time=1,rate_func=linear)
        self.wait(0.5)
        target.set_effect("detail",size=Pixels(8))
        self.wait(0.5)
        target.set_effect("detail",size=0.2)
        self.wait(0.5)
        target.remove_effect("detail")
        self.wait(0.5)
