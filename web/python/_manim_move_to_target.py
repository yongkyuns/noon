"""MoveToTarget compatibility marker.

The implementation intentionally lives in the bootstrap rate-function adapter because
that module is installed before the Manim-compatible Transform class. MoveToTarget
resolves ``noon.Transform`` only when instantiated, so playback remains the canonical
retained Transform path rather than a second scheduler.
"""
