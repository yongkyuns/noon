# ManimCE v0.21 `MoveToTarget` parity slice

This slice uses the literal `MoveToTargetExample` from ManimCE v0.21 with only the
import changed for the public Noon example. `Mobject.generate_target()` creates a
detached semantic copy; `MoveToTarget` therefore lowers to the existing retained
`Transform` path rather than introducing a second scheduler or renderer primitive.

The first exact-output slice covers leaf 2D mobjects and the standard generated-target
workflow. Group/VGroup family alignment remains partial under #82 and is rejected rather
than approximated. Qualification uses the unchanged canonical duration, semantic-state,
Cairo raster, direct-seek/incremental-playback, WebGPU, and WebGL gates.
