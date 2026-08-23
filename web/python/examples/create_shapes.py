from noon import (
    BLUE,
    DOWN,
    LEFT,
    PINK,
    RIGHT,
    UP,
    WHITE,
    Circle,
    Create,
    Line,
    Path,
    Scene,
    Square,
    VectorPath,
)

scene = Scene()

circle = Circle(0.9).set_fill(BLUE).set_stroke(WHITE, 0.055).shift(LEFT * 3 + UP)
square = Square(1.7).set_fill(PINK).set_stroke(WHITE, 0.055).shift(UP)
line = Line(LEFT, RIGHT).set_stroke(BLUE, 0.055).scale(1.25).shift(RIGHT * 3 + UP)

wave_path = (
    VectorPath()
    .move_to(LEFT * 2.4 + DOWN)
    .cubic_to(LEFT * 1.2 + DOWN * 2.0, RIGHT * 1.2, RIGHT * 2.4 + DOWN)
)
wave = Path(wave_path).set_fill(None).set_stroke(PINK, 0.05).shift(DOWN * 0.6)

scene.add(circle, square, line, wave)
scene.play(
    Create(circle),
    Create(square),
    Create(line),
    Create(wave),
    run_time=3.2,
    easing="ease_in_out_cubic",
)

result = scene
