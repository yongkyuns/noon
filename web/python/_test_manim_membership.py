"""Minimal membership hooks for isolated Python adapter tests.

Production installs the shared Rust membership implementation. Tests that load
only the Python ergonomics modules use this stand-in to bind wrappers without
installing another production membership path.
"""

from __future__ import annotations

from typing import Any


def install_test_membership(compat: Any) -> None:
    def edit(
        scene: Any,
        kind: str,
        values: tuple[object, ...] = (),
        *,
        key: str | None = None,
    ) -> None:
        if kind == "add":
            leaves = [
                member
                for value in values
                for member in compat._leaf_mobjects(value)
            ]
            if key is not None and len(leaves) != 1:
                raise ValueError("an explicit key can only be used when adding one Mobject")
            for index, member in enumerate(leaves):
                if member._scene is None:
                    member._bind_to_scene(scene, key=key if index == 0 else None)
                elif member._scene is not scene:
                    raise ValueError("Mobject already belongs to another Scene")
            for value in values:
                scene._register_top_level(value)
            return

        if kind == "remove":
            removed = {id(value) for value in values}
            scene._compat_top_level = [
                value for value in scene._compat_top_level if id(value) not in removed
            ]
            return
        if kind == "clear":
            scene._compat_top_level.clear()
            return
        if kind == "replace":
            old, new = values
            previous = list(scene._compat_top_level)
            edit(scene, "add", (new,))
            scene._compat_top_level = [
                new if value is old else value for value in previous
            ]
            return
        raise ValueError(f"unknown test membership operation {kind!r}")

    compat._STANDARD_MEMBERSHIP_EDIT = edit
    compat._STANDARD_MEMBERSHIP_VIEW = lambda scene: [
        value for value in scene._compat_top_level if scene._is_present(value)
    ]
