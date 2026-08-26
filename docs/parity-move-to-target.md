# ManimCE v0.21 `MoveToTarget` parity slice

This slice targets the source-equivalent `MoveToTargetExample` from the ManimCE v0.21
API documentation. `Mobject.generate_target()` already creates a detached semantic copy
in Noon. `MoveToTarget` is therefore lowered to the existing Manim-compatible retained
`Transform` path rather than introducing another scheduler or renderer primitive.

The first exact-output slice covers leaf 2D mobjects and the standard generated-target
workflow. Retained groups/families remain partial until family alignment semantics are
represented exactly.
