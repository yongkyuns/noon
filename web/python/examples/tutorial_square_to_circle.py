from noon import BLUE, PINK, Circle, Create, Scene, Square, Transform

scene = Scene()
square = Square(side_length=1.5, color=BLUE).set_fill(BLUE, opacity=0.25)
scene.play(Create(square), run_time=0.8)
scene.play(
    Transform(square, Circle(radius=0.9, color=PINK).set_fill(PINK, opacity=0.25)),
    run_time=1.0,
)
scene.wait(0.2)

result = scene
