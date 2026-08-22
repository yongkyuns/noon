from noon import Color, Scene, TransformMatchingShapes
from noon_layout import DOWN, UP, arrange

scene = Scene()

# Source and target rows are intentionally authored in different orders.
# TransformMatchingShapes pairs by semantic geometry signature, not list index.
source_slots = arrange(3, spacing=1.9, center=UP * 0.9)
target_slots = arrange(3, spacing=1.9, center=DOWN * 0.9)
OUTLINE = Color(0.94, 0.97, 1.0)

source_circle_a = scene.circle(0.30, position=source_slots[0], fill=Color(0.98, 0.66, 0.22), stroke=OUTLINE, stroke_width=0.06, key="source-circle-a")
source_rectangle = scene.rectangle(0.80, 0.48, position=source_slots[1], fill=Color(0.94, 0.46, 0.32), stroke=OUTLINE, stroke_width=0.06, key="source-rectangle")
source_circle_b = scene.circle(0.44, position=source_slots[2], fill=Color(0.90, 0.34, 0.64), stroke=OUTLINE, stroke_width=0.06, key="source-circle-b")

# Rectangle aspect ratio is preserved exactly so its matching signature stays stable.
target_rectangle = scene.rectangle(1.40, 0.84, position=target_slots[0], fill=Color(0.42, 0.62, 0.98), stroke=OUTLINE, stroke_width=0.08, key="target-rectangle")
target_circle_a = scene.circle(0.50, position=target_slots[1], fill=Color(0.36, 0.86, 0.72), stroke=OUTLINE, stroke_width=0.08, key="target-circle-a")
target_circle_b = scene.circle(0.68, position=target_slots[2], fill=Color(0.62, 0.48, 0.98), stroke=OUTLINE, stroke_width=0.08, key="target-circle-b")

scene.play(
    TransformMatchingShapes(
        [source_circle_a, source_rectangle, source_circle_b],
        [target_rectangle, target_circle_a, target_circle_b],
        key="matching.rearrange",
    ),
    duration=2.8,
    start_time=0.35,
    easing="ease_in_out_cubic",
)

result = scene
