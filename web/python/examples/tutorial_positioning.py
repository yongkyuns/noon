from noon import BLUE, GREEN, PINK, RIGHT, UP, Circle, Line, Scene, Square, VGroup

scene = Scene()
circle = Circle(radius=0.45, color=BLUE)
square = Square(side_length=0.9, color=PINK)
line = Line(color=GREEN)
row = VGroup(circle, square, line).arrange(RIGHT, buff=0.55).to_edge(UP, buff=0.8)
scene.add(row)
scene.play(row.animate.shift(UP * -1.4), run_time=1.0)
scene.wait(0.2)

result = scene
