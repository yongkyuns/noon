from noon import Add, BLUE, Circle, FadeIn, GREEN, LaggedStartMap, LEFT, PINK, RIGHT, Scene, Square, Succession, VGroup, Wait

scene = Scene()

left = Circle(radius=0.3, color=BLUE).shift(LEFT)
right = Circle(radius=0.3, color=GREEN).shift(RIGHT)
scene.play(
    Succession(
        Wait(0.4),
        Add(left),
        Wait(0.6),
        Add(right),
    )
)

squares = VGroup(
    Square(side_length=0.4, color=PINK).shift(LEFT * 0.5),
    Square(side_length=0.4, color=PINK).shift(RIGHT * 0.5),
)
scene.play(LaggedStartMap(FadeIn, squares, run_time=2.2, lag_ratio=0.1))

result = scene
