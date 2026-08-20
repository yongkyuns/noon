import math

from noon import PatchBatch

sequence = context["sequence"]
phase = -1.0 if sequence % 2 else 1.0

# Directly mutate the first three runtime objects without rebuilding the scene.
# Repeated presses alternate between two transform states while preserving the
# current playhead and all active timeline tracks.
result = (
    PatchBatch(sequence)
    .set_transform(
        0,
        translation=(-1.55 * phase, 0.82),
        rotation=0.35 * phase,
        scale=(1.18, 1.18),
    )
    .set_transform(
        1,
        translation=(1.45 * phase, -0.72),
        rotation=math.pi / 4.0 * phase,
        scale=(0.82, 1.22),
    )
    .set_transform(
        2,
        translation=(0.0, 1.15 * phase),
        rotation=math.pi / 2.0 * phase,
        scale=(1.0, 1.0),
    )
)
