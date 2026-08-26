"""Qualification-only Manim runtime tweaks for the exact raster oracle.

The canonical source stays byte-for-byte upstream.  This module is injected through
``PYTHONPATH`` only in the raster qualification workflow so Cairo materializes a
static ``Wait`` at the pinned frame rate instead of writing one PNG with a repeat
count.  Rendering every static-wait frame keeps materialized frame indices aligned
with Manim's logical renderer time without changing scene state or animation output.
"""

from __future__ import annotations

import os


if os.environ.get("NOON_MANIM_MATERIALIZE_STATIC_WAITS") == "1":
    from manim.scene.scene import Scene

    _original_should_update_mobjects = Scene.should_update_mobjects

    def _materialize_static_wait_frames(self: Scene) -> bool:
        # Scene.should_update_mobjects() is only consulted for a single Wait.
        # Returning True disables Cairo's frozen-frame/repeat-count optimization;
        # genuinely dynamic waits already take this path, while static waits render
        # identical frames at the configured frame rate.
        return True

    _materialize_static_wait_frames.__name__ = _original_should_update_mobjects.__name__
    _materialize_static_wait_frames.__doc__ = _original_should_update_mobjects.__doc__
    Scene.should_update_mobjects = _materialize_static_wait_frames
