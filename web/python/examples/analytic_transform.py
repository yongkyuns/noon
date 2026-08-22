from noon import (
    BLUE,
    PURPLE,
    WHITE,
    LEFT,
    RIGHT,
    UP,
    Circle,
    Line,
    Rectangle,
    Scene,
    Transform,
    VGroup,
)

scene = Scene()

# One idea only: analytic geometry stays analytic while Transform interpolates
# circle radius, rectangle size, and line endpoints directly.
circle = Circle(0.35, color=BLUE).set_stroke(WHITE, 0.06)
rectangle = Rectangle(0.85, 0.5, color=BLUE).set_stroke(WHITE, 0.06)
line = Line(LEFT * 0.45, RIGHT * 0.45, color=BLUE).set_stroke(BLUE, 0.12)
VGroup(circle, rectangle, line).arrange(RIGHT, buff=1.1)

scene.add(circle, key="analytic-circle")
scene.add(rectangle, key="analytic-rectangle")
scene.add(line, key="analytic-line")

circle_target = (
    Circle(0.8, color=PURPLE)
    .set_stroke(WHITE, 0.1)
    .move_to(circle.get_center())
)
rectangle_target = (
    Rectangle(1.55, 0.95, color=PURPLE)
    .set_stroke(WHITE, 0.1)
    .move_to(rectangle.get_center())
)
line_target = (
    Line(LEFT * 0.75 + UP * 0.55, RIGHT * 0.75 - UP * 0.55, color=PURPLE)
    .set_stroke(PURPLE, 0.2)
    .move_to(line.get_center())
)

scene.play(
    Transform(circle, circle_target, key="analytic-circle.transform"),
    Transform(rectangle, rectangle_target, key="analytic-rectangle.transform"),
    Transform(line, line_target, key="analytic-line.transform"),
    run_time=3.2,
    easing="ease_in_out_cubic",
)

result = scene
