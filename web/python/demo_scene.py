from noon import (
    BLUE,
    GREEN,
    RED,
    WHITE,
    DEGREES,
    DOWN,
    LEFT,
    RIGHT,
    UP,
    Circle,
    Line,
    Rectangle,
    Scene,
    VGroup,
)

scene = Scene()

# Three primitives, three semantic operations. Layout is expressed through the
# objects themselves rather than precomputed coordinate slots.
circle = Circle(0.55, color=RED).set_stroke(WHITE, 0.05)
rectangle = (
    Rectangle(1.25, 0.8, color=BLUE)
    .set_stroke(WHITE, 0.05)
    .rotate(-45 * DEGREES)
)
line = Line(LEFT * 0.7, RIGHT * 0.7, color=GREEN).set_stroke(GREEN, 0.12)
line.set_opacity(0.25)

VGroup(circle, rectangle, line).arrange(RIGHT, buff=0.9)
circle.shift(DOWN * 0.65)

scene.add(circle, key="circle")
scene.add(rectangle, key="rectangle")
scene.add(line, key="line")

scene.play(
    circle.animate.shift(UP * 1.3),
    rectangle.animate.rotate(90 * DEGREES),
    line.animate.set_opacity(1.0),
    run_time=3.2,
    easing="ease_in_out_cubic",
)

result = scene
