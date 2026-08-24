from noon import BLUE, GREEN, PINK, RIGHT, Circle, ReplacementTransform, Scene, Square, Transform

scene = Scene()
left = Square(side_length=0.9, color=BLUE).shift(RIGHT * -1.8)
left_target = Circle(radius=0.55, color=GREEN).shift(RIGHT * -1.8)
right = Square(side_length=0.9, color=BLUE).shift(RIGHT * 1.8)
right_target = Circle(radius=0.55, color=PINK).shift(RIGHT * 1.8)

scene.add(left, right, right_target)
scene.play(Transform(left, left_target), run_time=0.9)
scene.play(ReplacementTransform(right, right_target), run_time=0.9)
scene.wait(0.2)

result = scene
