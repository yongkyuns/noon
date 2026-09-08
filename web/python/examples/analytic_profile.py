"""Python counterpart of noon::example_scenes::analytic_profile (same shared engine)."""
import math
from noon import *

options = globals().get("context", {})
count = options.get("object_count", 1000)
layout = options.get("layout", "fit")
aspect = float(options.get("aspect", 16 / 9))
duration = float(options.get("duration", 60))
if isinstance(count, bool) or not isinstance(count, int) or not 1 <= count <= 100_000:
    raise ValueError("analytic object count must be between 1 and 100000")
if layout not in ("fit", "fixed", "overdraw"):
    raise ValueError("analytic layout must be fit, fixed, or overdraw")
if not math.isfinite(aspect) or aspect <= 0 or not math.isfinite(duration) or duration <= 0:
    raise ValueError("analytic aspect and duration must be positive and finite")

scene = MovingCameraScene()
columns = math.ceil(math.sqrt(count * aspect))
rows = math.ceil(count / columns)
camera_height = rows if layout == "fit" else 6
scene.camera.frame.scale(camera_height / DEFAULT_FRAME_HEIGHT)
dots = []
for index in range(count):
    column, row = index % columns, index // columns
    if layout == "fit":
        radius, x, y, alpha = 0.32, column - columns / 2 + 0.5, row - rows / 2 + 0.5, 1
    elif layout == "fixed":
        camera_width = camera_height * aspect
        radius = 0.06
        x = -camera_width / 2 + (column + 0.5) * camera_width / columns
        y = -camera_height / 2 + (row + 0.5) * camera_height / rows
        alpha = 1
    else:
        distance = 0.4 * math.sqrt((index + 0.5) / count)
        angle = index * math.pi * (3 - math.sqrt(5))
        radius, x, y, alpha = 0.35, math.cos(angle) * distance, math.sin(angle) * distance, 0.16
    dot = Circle(radius, color=Color(0.27, 0.65, 0.96), fill_opacity=alpha).set_stroke(width=0).move_to([x, y, 0])
    dots.append(dot)
scene.add(*dots)
scene.play(dots[0].animate.shift(8 * RIGHT), run_time=duration, rate_func=linear)
result = scene
