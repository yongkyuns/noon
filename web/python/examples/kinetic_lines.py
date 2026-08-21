import math

from noon import Color, Scene

scene = Scene()

# Analytic line batching: a rotating fan of independent line instances.
for index in range(32):
    angle = index * math.tau / 32.0
    length = 1.45 + 1.05 * ((index % 5) / 4.0)
    line = scene.line(
        (-length, 0.0),
        (length, 0.0),
        key=f"ray.{index}",
        rotation=angle,
        stroke=Color(
            0.35 + 0.50 * ((index % 8) / 7.0),
            0.48 + 0.36 * (((index + 3) % 8) / 7.0),
            1.0,
            0.52,
        ),
        stroke_width=0.018 + 0.018 * (index % 3),
        opacity=0.72,
    )
    scene.animate_rotation(
        line,
        angle,
        angle + math.tau * (1.0 if index % 2 == 0 else -1.0),
        duration=4.0,
        easing="ease_in_out_cubic",
        key=f"ray.{index}.rotation",
    )
    scene.animate_opacity(
        line,
        0.18 + 0.58 * ((index % 6) / 5.0),
        0.92,
        start_time=(index % 8) * 0.07,
        duration=2.4,
        easing="ease_in_out_cubic",
        key=f"ray.{index}.opacity",
    )

for ring_index, radius in enumerate((0.34, 0.62, 0.92)):
    ring = scene.circle(
        radius,
        key=f"ring.{ring_index}",
        fill=None,
        stroke=Color(0.85, 0.90, 1.0, 0.70 - ring_index * 0.12),
        stroke_width=0.026,
        opacity=0.76,
    )
    scene.animate_opacity(
        ring,
        0.24,
        0.92 - ring_index * 0.10,
        start_time=ring_index * 0.18,
        duration=2.8,
        easing="ease_in_out_cubic",
        key=f"ring.{ring_index}.opacity",
    )

result = scene
