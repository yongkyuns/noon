from noon import BLUE, DOWN, LEFT, ORIGIN, RIGHT, UP, Scene, VectorPath

scene = Scene()

# A single multi-contour path demonstrates the reveal domain. Geometry numbers
# here define the path itself; layout and styling use Noon semantic vocabulary.
WIDTH = 2.7
ARCH_HEIGHT = 0.95
LOWER_ROW = DOWN

path = (
    VectorPath()
    .move_to(LEFT * WIDTH + UP * 0.6)
    .cubic_to(
        LEFT * (WIDTH * 0.55) + UP * (0.6 + ARCH_HEIGHT),
        RIGHT * (WIDTH * 0.55) + UP * (0.6 + ARCH_HEIGHT),
        RIGHT * WIDTH + UP * 0.6,
    )
    .move_to(LEFT * WIDTH + LOWER_ROW)
    .quadratic_to(LEFT * (WIDTH * 0.45) + DOWN * 1.75, ORIGIN + LOWER_ROW)
    .quadratic_to(RIGHT * (WIDTH * 0.45) + DOWN * 1.75, RIGHT * WIDTH + LOWER_ROW)
)

stroke = scene.path(
    path,
    fill=None,
    stroke=BLUE,
    stroke_width=0.11,
    key="reveal-path",
)
scene.animate_reveal(
    stroke,
    duration=3.2,
    easing="ease_in_out_cubic",
    key="reveal-path.draw",
)

result = scene
