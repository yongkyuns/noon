from noon import BLUE, GREEN, RIGHT, UP, Scene, Square

scene = Scene()
square = Square(side_length=1.0, color=BLUE).set_fill(BLUE, opacity=0.3)
scene.add(square)
scene.play(
    square.animate.shift(RIGHT * 2.0 + UP * 0.7).rotate(0.6).scale(1.35).set_color(GREEN),
    run_time=1.2,
)
scene.wait(0.2)

result = scene
