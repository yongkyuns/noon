from noon import BLUE, WHITE, Circle, Create, Scene

scene = Scene()
circle = Circle(radius=1.0, color=BLUE).set_fill(BLUE, opacity=0.35).set_stroke(WHITE, 0.06)
scene.play(Create(circle), run_time=1.0)
scene.wait(0.25)

result = scene
