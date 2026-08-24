from noon import BLUE, GREEN, PINK, RIGHT, UP, AnimationGroup, Circle, LaggedStart, Scene, VGroup

scene = Scene()
objects = VGroup(
    Circle(radius=0.3, color=BLUE),
    Circle(radius=0.3, color=GREEN),
    Circle(radius=0.3, color=PINK),
).arrange(RIGHT, buff=0.7)
scene.add(objects)
scene.play(
    LaggedStart(
        *(member.animate.shift(UP * 1.0) for member in objects),
        lag_ratio=0.35,
    ),
    run_time=1.4,
)
scene.play(
    AnimationGroup(*(member.animate.shift(UP * -0.5) for member in objects)),
    run_time=0.8,
)

result = scene
