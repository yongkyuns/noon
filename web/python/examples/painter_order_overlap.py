from noon import BLUE, GREEN, RED, Circle, Rectangle, Scene, Vec2, VectorPath

scene = Scene()

# Deliberately cross renderer pipelines at the same canvas location. Semantic
# insertion order requires the filled green vector path to remain above the
# analytic red circle and blue rectangle.
circle = Circle(1.25, color=RED).set_stroke(None)
rectangle = Rectangle(2.1, 2.1, color=BLUE).set_stroke(None)
scene.add(circle, key="painter.circle")
scene.add(rectangle, key="painter.rectangle")

square = (
    VectorPath()
    .move_to(Vec2(-0.8, -0.8))
    .line_to(Vec2(0.8, -0.8))
    .line_to(Vec2(0.8, 0.8))
    .line_to(Vec2(-0.8, 0.8))
    .close()
)
scene.path(
    square,
    fill=GREEN,
    stroke=GREEN,
    stroke_width=0.0,
    key="painter.path",
)

# Give the fixture a deterministic non-zero timeline while preserving the
# center overlap at every sample.
scene.play(rectangle.animate.rotate(1.5707963267948966), run_time=1.0, easing="linear")

result = scene
