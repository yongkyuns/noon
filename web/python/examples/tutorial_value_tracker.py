from noon import BLUE, RIGHT, Scene, Square, ValueTracker, linear

scene = Scene()
square = Square(side_length=0.9, color=BLUE)
scene.add(square)
progress = ValueTracker(0.0)
scene.bind_position(square, progress, direction=RIGHT)
scene.play(progress.animate(run_time=1.5, rate_func=linear).set_value(2.5))

result = scene
