import math

from noon import Color, Scene
from noon_layout import DOWN, LEFT, RIGHT, UP, arrange

scene = Scene()

# Keep the first example intentionally small: three primitive types and three
# narrow timeline properties. Later examples introduce Transform and lifecycle.
CIRCLE_COLOR = Color(0.98, 0.38, 0.36)
RECTANGLE_COLOR = Color(0.27, 0.65, 0.96)
LINE_COLOR = Color(0.30, 0.88, 0.57)
OUTLINE = Color(1.0, 1.0, 1.0)

circle_slot, rectangle_slot, line_slot = arrange(3, spacing=2.25)

circle = scene.circle(
    0.55,
    key="circle",
    position=circle_slot + DOWN * 0.65,
    fill=CIRCLE_COLOR,
    stroke=OUTLINE,
    stroke_width=0.05,
)
rectangle = scene.rectangle(
    1.25,
    0.8,
    key="rectangle",
    position=rectangle_slot,
    fill=RECTANGLE_COLOR,
    stroke=OUTLINE,
    stroke_width=0.05,
)
line = scene.line(
    LEFT * 0.7,
    RIGHT * 0.7,
    key="line",
    position=line_slot,
    stroke=LINE_COLOR,
    stroke_width=0.12,
    opacity=0.25,
)

TIMING = {"duration": 3.2, "easing": "ease_in_out_cubic"}
scene.animate_position(
    circle,
    circle_slot + DOWN * 0.65,
    circle_slot + UP * 0.65,
    key="circle.move",
    **TIMING,
)
scene.animate_rotation(
    rectangle,
    -math.pi / 4.0,
    math.pi / 4.0,
    key="rectangle.rotate",
    **TIMING,
)
scene.animate_opacity(
    line,
    0.25,
    1.0,
    key="line.appear",
    **TIMING,
)

result = scene
