"""Shared painter-order proof for the Noon browser/Pyodide authoring environment.

Rust counterpart: `cargo run -p noon-native --example painter_order_overlap`.
Both predeclare the same animation and permit deterministic midpoint sampling.
"""

from noon import BLUE, GREEN, RED, Circle, Path, Rectangle, Scene, Vec2, VectorPath, linear

scene = Scene()

# Deliberately cross renderer pipelines at the same canvas location. Semantic
# insertion order requires the filled green vector path to remain above the
# analytic red circle and blue rectangle.
circle = Circle(1.25, color=RED).set_fill(RED, opacity=1.0).set_stroke(None)
rectangle = Rectangle(2.1, 2.1, color=BLUE).set_fill(BLUE, opacity=1.0).set_stroke(None)
scene.add(circle, key="painter.circle")
scene.add(rectangle, key="painter.rectangle")

square = (
    VectorPath()
    .move_to(Vec2(-0.8, -0.8))
    .line_to(Vec2(0.8, -0.8))
    .line_to(Vec2(0.8, 0.8))
    .line_to(Vec2(-0.8, 0.8))
    .close()
)
scene.add(Path(square, fill=GREEN, stroke=GREEN, stroke_width=0.0), key="painter.path")

# Predeclare the shared affine animation so either host can seek deterministically
# without keeping a Python source continuation on the playback path.
target = rectangle.copy().rotate(1.5707963267948966)
animation = scene.declare_live_transform_to(rectangle, target, run_time=1.0, rate_func=linear)
live = scene.live_execution()
end = live.play(animation)
live.advance_to(end)
live.complete()

result = scene
