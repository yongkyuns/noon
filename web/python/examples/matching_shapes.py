from noon import (
    BLUE,
    GREEN,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    WHITE,
    DOWN,
    RIGHT,
    UP,
    Circle,
    Rectangle,
    Scene,
    TransformMatchingShapes,
    VGroup,
)

scene = Scene()

# Source and target rows are authored in different orders. Matching is by
# semantic geometry signature, not by list position.
source_circle_a = Circle(0.30, color=ORANGE).set_stroke(WHITE, 0.06)
source_rectangle = Rectangle(0.80, 0.48, color=RED).set_stroke(WHITE, 0.06)
source_circle_b = Circle(0.44, color=PINK).set_stroke(WHITE, 0.06)
VGroup(source_circle_a, source_rectangle, source_circle_b).arrange(RIGHT, buff=0.9).shift(UP)

# Rectangle aspect ratio is preserved so its matching signature remains stable.
target_rectangle = Rectangle(1.40, 0.84, color=BLUE).set_stroke(WHITE, 0.08)
target_circle_a = Circle(0.50, color=GREEN).set_stroke(WHITE, 0.08)
target_circle_b = Circle(0.68, color=PURPLE).set_stroke(WHITE, 0.08)
VGroup(target_rectangle, target_circle_a, target_circle_b).arrange(RIGHT, buff=0.9).shift(DOWN)

for key, mobject in (
    ("source-circle-a", source_circle_a),
    ("source-rectangle", source_rectangle),
    ("source-circle-b", source_circle_b),
    ("target-rectangle", target_rectangle),
    ("target-circle-a", target_circle_a),
    ("target-circle-b", target_circle_b),
):
    scene.add(mobject, key=key)

scene.play(
    TransformMatchingShapes(
        [source_circle_a, source_rectangle, source_circle_b],
        [target_rectangle, target_circle_a, target_circle_b],
        key="matching.rearrange",
    ),
    run_time=2.8,
    easing="ease_in_out_cubic",
)

result = scene
