from noon import Circle, Color, ReplacementTransform, Scene, TransformFromCopy
from noon_layout import DOWN, UP, arrange

scene = Scene()
OUTLINE = Color(0.94, 0.97, 1.0)

# Top row: ReplacementTransform hands stable scene presence from one object to
# the next at exact boundaries.
first_slot, middle_slot, last_slot = arrange(3, spacing=2.15, center=UP * 0.9)
first = scene.add(
    Circle(
        0.34,
        position=first_slot,
        fill=Color(0.96, 0.38, 0.42),
        stroke=OUTLINE,
        stroke_width=0.06,
    ),
    key="replacement-first",
)
middle = scene.add(
    Circle(
        0.50,
        position=middle_slot,
        fill=Color(0.34, 0.68, 0.98),
        stroke=OUTLINE,
        stroke_width=0.07,
    ),
    key="replacement-middle",
)
last = scene.add(
    Circle(
        0.68,
        position=last_slot,
        fill=Color(0.70, 0.42, 0.96),
        stroke=OUTLINE,
        stroke_width=0.08,
    ),
    key="replacement-last",
)
scene.play(
    ReplacementTransform(first, middle, key="replacement.first-to-middle"),
    duration=1.25,
    easing="ease_in_out_cubic",
)
scene.play(
    ReplacementTransform(middle, last, key="replacement.middle-to-last"),
    duration=1.25,
    start_time=1.25,
    easing="ease_in_out_cubic",
)

# Bottom row: TransformFromCopy materializes the target without consuming its
# source, making the lifecycle difference visible beside the replacement row.
copy_source_slot, copy_target_slot = arrange(2, spacing=2.8, center=DOWN * 0.9)
copy_source = scene.add(
    Circle(
        0.38,
        position=copy_source_slot,
        fill=Color(0.22, 0.86, 0.66),
        stroke=OUTLINE,
        stroke_width=0.06,
    ),
    key="copy-source",
)
copy_target = scene.add(
    Circle(
        0.62,
        position=copy_target_slot,
        fill=Color(0.18, 0.76, 0.94),
        stroke=OUTLINE,
        stroke_width=0.08,
    ),
    key="copy-target",
)
scene.play(
    TransformFromCopy(copy_source, copy_target, key="copy.spawn"),
    duration=1.6,
    start_time=0.45,
    easing="ease_in_out_cubic",
)

result = scene
