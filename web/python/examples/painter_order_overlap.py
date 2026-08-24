from noon import BLUE, GREEN, RED, Circle, Rectangle, Scene, Vec2, VectorPath

scene = Scene()

# These shapes deliberately overlap at the canvas center and cross renderer
# pipelines: circle/rectangle use analytic instances while the square is a
# tessellated vector path. Semantic insertion order requires the green path to
# remain above the red circle and blue rectangle for the whole scene.
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

# Keep a non-zero deterministic timeline so browser smoke captures several
# checkpoints while the ordering relationship stays visually unambiguous.
scene.play(rectangle.animate.rotate(1.5707963267948966), run_time=1.0, easing="linear")

result = scene
