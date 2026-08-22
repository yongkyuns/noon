from noon import (
    BLUE,
    GREEN,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    TEAL,
    WHITE,
    DOWN,
    RIGHT,
    UP,
    Circle,
    Scene,
    VGroup,
)

scene = Scene()

# Seven identical objects make timing the only changing variable. Explicit
# start_time is intentional here because stagger composition is the feature.
COLORS = (BLUE, TEAL, GREEN, ORANGE, RED, PINK, PURPLE)
STAGGER = 0.24
RUN_TIME = 1.45
TRAVEL = UP * 1.35

dots = [Circle(0.24, color=color).set_stroke(WHITE, 0.04) for color in COLORS]
VGroup(*dots).arrange(RIGHT, buff=0.34).shift(DOWN * 0.68)

for index, dot in enumerate(dots):
    scene.add(dot, key=f"dot.{index}")
    start = dot.get_center()
    scene.animate_position(
        dot,
        start,
        start + TRAVEL,
        start_time=index * STAGGER,
        duration=RUN_TIME,
        easing="ease_in_out_cubic",
        key=f"dot.{index}.rise",
    )

result = scene
