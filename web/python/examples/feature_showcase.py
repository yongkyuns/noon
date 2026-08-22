from noon import (
    Circle,
    Color,
    FadeIn,
    FadeOut,
    Rectangle,
    ReplacementTransform,
    Scene,
    TransformFromCopy,
    TransformMatchingShapes,
)

scene = Scene()

# Top lane: one stable semantic object hands off through two lifecycle targets.
# Exact-boundary chaining exercises Presence continuity while the visible shape
# moves and changes geometry.
first = scene.add(
    Circle(
        0.42,
        position=(-2.55, 1.25),
        fill=Color(0.96, 0.38, 0.42),
        stroke=Color(1.0, 0.82, 0.84),
        stroke_width=0.06,
    ),
    key="lifecycle-first",
)
middle = scene.add(
    Rectangle(
        1.05,
        0.72,
        position=(0.0, 1.25),
        rotation=0.25,
        fill=Color(0.36, 0.64, 0.98),
        stroke=Color(0.78, 0.9, 1.0),
        stroke_width=0.07,
    ),
    key="lifecycle-middle",
)
last = scene.add(
    Circle(
        0.7,
        position=(2.55, 1.25),
        fill=Color(0.7, 0.42, 0.96),
        stroke=Color(0.9, 0.8, 1.0),
        stroke_width=0.08,
    ),
    key="lifecycle-last",
)
scene.play(
    ReplacementTransform(first, middle, key="lifecycle.first-to-middle"),
    duration=2.0,
    easing="ease_in_out_cubic",
)
scene.play(
    ReplacementTransform(middle, last, key="lifecycle.middle-to-last"),
    duration=2.0,
    start_time=2.0,
    easing="ease_in_out_cubic",
)

# Middle lane: TransformFromCopy leaves the source untouched. The copied target
# has authored semantic opacity 0.42, so FadeOut/FadeIn visibly proves that the
# Appearance channel modulates opacity without rewriting that authored value.
copy_source = scene.add(
    Circle(
        0.34,
        position=(-2.55, 0.0),
        fill=Color(0.2, 0.86, 0.68),
        stroke=Color(0.72, 1.0, 0.9),
        stroke_width=0.055,
    ),
    key="copy-source",
)
copy_target = scene.add(
    Circle(
        0.68,
        position=(0.0, 0.0),
        fill=Color(0.18, 0.78, 0.94),
        stroke=Color(0.72, 0.94, 1.0),
        stroke_width=0.08,
        opacity=0.42,
    ),
    key="copy-target",
)
scene.play(
    TransformFromCopy(copy_source, copy_target, key="copy.spawn"),
    duration=2.0,
    easing="ease_in_out_cubic",
)
scene.play(
    FadeOut(copy_target, key="copy.fade-out"),
    duration=1.25,
    start_time=2.5,
    easing="ease_in_out_cubic",
)
scene.play(
    FadeIn(copy_target, key="copy.fade-in"),
    duration=1.25,
    start_time=4.25,
    easing="ease_in_out_cubic",
)

# Bottom lane: targets are deliberately authored out of source order.
# TransformMatchingShapes pairs by semantic geometry signature, not by list
# position. Duplicate circles use stable input-order tie breaking.
source_circle_a = scene.circle(
    0.28,
    position=(-2.8, -1.35),
    fill=Color(0.98, 0.66, 0.22),
    key="match-source-circle-a",
)
source_rectangle = scene.rectangle(
    0.78,
    0.46,
    position=(-1.75, -1.35),
    fill=Color(0.94, 0.46, 0.32),
    key="match-source-rectangle",
)
source_circle_b = scene.circle(
    0.42,
    position=(-0.7, -1.35),
    fill=Color(0.9, 0.34, 0.64),
    key="match-source-circle-b",
)

target_rectangle = scene.rectangle(
    1.08,
    0.62,
    position=(0.45, -1.35),
    rotation=0.28,
    fill=Color(0.42, 0.62, 0.98),
    key="match-target-rectangle",
)
target_circle_a = scene.circle(
    0.48,
    position=(1.65, -1.35),
    fill=Color(0.36, 0.86, 0.72),
    key="match-target-circle-a",
)
target_circle_b = scene.circle(
    0.64,
    position=(2.85, -1.35),
    fill=Color(0.62, 0.48, 0.98),
    key="match-target-circle-b",
)
scene.play(
    TransformMatchingShapes(
        [source_circle_a, source_rectangle, source_circle_b],
        [target_rectangle, target_circle_a, target_circle_b],
        key="matching.rearrange",
    ),
    duration=2.0,
    start_time=4.0,
    easing="ease_in_out_cubic",
)

result = scene
