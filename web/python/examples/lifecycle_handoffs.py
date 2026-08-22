from noon import (
    BLUE,
    GREEN,
    PURPLE,
    RED,
    WHITE,
    DOWN,
    RIGHT,
    UP,
    Circle,
    ReplacementTransform,
    Scene,
    TransformFromCopy,
    VGroup,
)

scene = Scene()

# ReplacementTransform transfers presence from one stable object to the next.
first = Circle(0.34, color=RED).set_stroke(WHITE, 0.06)
middle = Circle(0.50, color=BLUE).set_stroke(WHITE, 0.07)
last = Circle(0.68, color=PURPLE).set_stroke(WHITE, 0.08)
VGroup(first, middle, last).arrange(RIGHT, buff=1.25).shift(UP)

# TransformFromCopy leaves its source present while materializing the target.
copy_source = Circle(0.38, color=GREEN).set_stroke(WHITE, 0.06)
copy_target = Circle(0.62, color=BLUE).set_stroke(WHITE, 0.08)
VGroup(copy_source, copy_target).arrange(RIGHT, buff=1.5).shift(DOWN)

scene.add(first, key="replacement-first")
scene.add(middle, key="replacement-middle")
scene.add(last, key="replacement-last")
scene.add(copy_source, key="copy-source")
scene.add(copy_target, key="copy-target")

scene.play(
    ReplacementTransform(first, middle, key="replacement.first-to-middle"),
    run_time=1.0,
    easing="ease_in_out_cubic",
)
scene.play(
    ReplacementTransform(middle, last, key="replacement.middle-to-last"),
    run_time=1.0,
    easing="ease_in_out_cubic",
)
scene.play(
    TransformFromCopy(copy_source, copy_target, key="copy.spawn"),
    run_time=1.4,
    easing="ease_in_out_cubic",
)

result = scene
