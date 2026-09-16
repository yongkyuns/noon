"""Check standard-library imports and the actual Noon tutorial vocabulary."""
from dataclasses import dataclass
from bisect import bisect_right
import json
import math
import random
from noon import Scene, Text, MathTypst, VMobject, Dot, Ellipse, Rectangle, Transform, FadeIn, FadeOut, Create, Rotate, Color, linear

@dataclass(frozen=True)
class ProbeConfig:
    seed: int = 42

assert json.loads(json.dumps({"seed": ProbeConfig().seed}))["seed"] == 42
assert bisect_right([0., 1., 2.], 1.) == 2
assert math.isfinite(random.Random(ProbeConfig().seed).gauss(0, 1))

async def move_marker(scene, marker):
    await scene.play(Transform(marker, marker.copy().shift((4, 0))), run_time=2, rate_func=linear)

class CapabilityProbe(Scene):
    async def construct(self):
        title = Text("INS + GNSS | runtime probe", font_size=34).move_to((0, 2.7))
        equation = MathTypst(r"hat(p)^+ = hat(p)^- + K (z - hat(p)^-)", font_size=38).move_to((0, 1.5))
        covariance = MathTypst(r"P^+ = (I-K H) P^- (I-K H)^T + K R K^T", font_size=25).move_to((0, -2.6))
        path = VMobject(color=Color(0.25, 0.8, 1)).set_points_as_corners([(-4,-1),(-2,0),(0,-0.5),(2,1),(4,0)])
        marker = Dot(radius=0.11, color=Color(1,0.8,0.3)).move_to((-2,0))
        ellipse = Ellipse(width=1.2, height=0.6).move_to((2,-1.5))
        box = Rectangle(width=1, height=0.45).move_to((-3,-1.6))
        self.add(title, equation, covariance, marker, ellipse, box)
        await self.play(Create(path), run_time=1)
        await move_marker(self, marker)
        await self.play(Rotate(box, angle=math.pi/4), run_time=1)
        await self.play(FadeOut(equation), FadeIn(Text("Dependency and geometry checks passed", font_size=24).move_to((0,1.5))), run_time=0.5)
        await self.wait(1.5)
