from noon import Color, Scene
from noon_layout import DOWN, UP, arrange

scene = Scene()

# Seven identical objects make timing the only changing variable. Each starts a
# little later than its neighbor, making start_time composition easy to read.
COUNT = 7
BASE_POSITIONS = arrange(COUNT, spacing=0.82)
TRAVEL = UP * 1.35
STAGGER = 0.24
RUN_TIME = 1.45

for index, base in enumerate(BASE_POSITIONS):
    progress = index / (COUNT - 1)
    color = Color(0.30 + 0.58 * progress, 0.78 - 0.28 * progress, 0.96)
    start = base + DOWN * 0.68
    end = start + TRAVEL
    dot = scene.circle(
        0.24,
        position=start,
        fill=color,
        stroke=Color(0.94, 0.97, 1.0),
        stroke_width=0.04,
        key=f"dot.{index}",
    )
    scene.animate_position(
        dot,
        start,
        end,
        start_time=index * STAGGER,
        duration=RUN_TIME,
        easing="ease_in_out_cubic",
        key=f"dot.{index}.rise",
    )

result = scene
