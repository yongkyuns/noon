import math

from noon import BLUE, RED, Circle, Scene, Vec2

context_value = globals().get("context", {})
if not isinstance(context_value, dict):
    context_value = {}

object_count = context_value.get("object_count", 1000)
variant = context_value.get("variant", 0)
if isinstance(object_count, bool) or not isinstance(object_count, int):
    raise TypeError("object_count must be an integer")
if object_count <= 0 or object_count > 100_000:
    raise ValueError("object_count must be between 1 and 100000")
if isinstance(variant, bool) or not isinstance(variant, int):
    raise TypeError("variant must be an integer")

scene = Scene()
aspect = 16.0 / 9.0
columns = math.ceil(math.sqrt(object_count * aspect))
rows = math.ceil(object_count / columns)
spacing = 0.085
radius = 0.026

for index in range(object_count):
    column = index % columns
    row = index // columns
    x = (column - (columns - 1) * 0.5) * spacing
    y = ((rows - 1) * 0.5 - row) * spacing
    color = RED if variant == 1 and index == object_count // 2 else BLUE
    dot = Circle(radius, color=color).set_stroke(None).move_to(Vec2(x, y))
    scene.add(dot, key=f"perf.{index}")

result = scene
