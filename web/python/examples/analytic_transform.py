from noon import Circle, Color, Line, Rectangle, Scene, Transform
from noon_layout import LEFT, RIGHT, UP, arrange

scene = Scene()

# One idea only: analytic geometry stays analytic while Transform interpolates
# circle radius, rectangle size, and line endpoints directly.
SOURCE = Color(0.34, 0.68, 0.96)
TARGET = Color(0.72, 0.42, 0.96)
OUTLINE = Color(0.92, 0.96, 1.0)

circle_slot, rectangle_slot, line_slot = arrange(3, spacing=2.25)

circle = scene.add(
    Circle(0.35, position=circle_slot, fill=SOURCE, stroke=OUTLINE, stroke_width=0.06),
    key="analytic-circle",
)
rectangle = scene.add(
    Rectangle(
        0.85,
        0.5,
        position=rectangle_slot,
        fill=SOURCE,
        stroke=OUTLINE,
        stroke_width=0.06,
    ),
    key="analytic-rectangle",
)
line = scene.add(
    Line(
        LEFT * 0.45,
        RIGHT * 0.45,
        position=line_slot,
        stroke=SOURCE,
        stroke_width=0.12,
    ),
    key="analytic-line",
)

scene.play(
    Transform(
        circle,
        Circle(0.8, position=circle_slot, fill=TARGET, stroke=OUTLINE, stroke_width=0.1),
        key="analytic-circle.transform",
    ),
    Transform(
        rectangle,
        Rectangle(
            1.55,
            0.95,
            position=rectangle_slot,
            fill=TARGET,
            stroke=OUTLINE,
            stroke_width=0.1,
        ),
        key="analytic-rectangle.transform",
    ),
    Transform(
        line,
        Line(
            LEFT * 0.75 + UP * 0.55,
            RIGHT * 0.75 - UP * 0.55,
            position=line_slot,
            stroke=TARGET,
            stroke_width=0.2,
        ),
        key="analytic-line.transform",
    ),
    duration=3.2,
    easing="ease_in_out_cubic",
)

result = scene
