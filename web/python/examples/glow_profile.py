"""G4 counterpart of noon::example_scenes::glow_profile; ordinary target animation."""
import math
from noon import *

options = globals().get("context", {})
active = options.get("active_glows", 1)
property_name = options.get("property", "intensity")
duration = float(options.get("duration", 60))
if isinstance(active, bool) or not isinstance(active, int) or active not in (1, 100):
    raise ValueError("G4 active glow count must be 1 or 100")
if property_name not in ("intensity", "radius"):
    raise ValueError("glow property must be intensity or radius")
if not math.isfinite(duration) or duration < 10:
    raise ValueError("G4 duration must be finite and at least 10 seconds")

scene = MovingCameraScene()
scene.camera.frame.scale(6 / DEFAULT_FRAME_HEIGHT)
dots = []
for index in range(600):
    dot = Circle(0.05, color=WHITE, fill_opacity=1).set_stroke(width=0)
    dot.move_to([-3.75 + (index % 30) * 0.25, -2.375 + (index // 30) * 0.25, 0])
    if index < active:
        dot.set_glow(radius=Pixels(3), intensity=0.4)
    dots.append(dot)
scene.add(*dots)
if property_name == "intensity":
    animations = [dot.animate.set_glow(intensity=1) for dot in dots[:active]]
else:
    animations = [dot.animate.set_glow(radius=Pixels(6)) for dot in dots[:active]]
scene.play(*animations, run_time=duration, rate_func=linear)
result = scene
