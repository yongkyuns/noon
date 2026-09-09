"""Minimal membership hooks for isolated Python adapter tests.

Production calls the shared Rust membership implementation directly. Tests that load
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
        if not hasattr(scene, "_test_membership"):
            scene._test_membership = []
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
                    # Adapter-only fixture identity, without a scene/timeline engine.
                    object_id = scene._next_object_id
                    scene._next_object_id += 1
                    obj = compat._ir.Object(object_id, scene._owner)
                    handle = getattr(member, "_semantic_handle", None)
                    if handle is not None:
                        scene._binding_handles[object_id] = handle
                    member._bind(scene, obj)
                elif member._scene is not scene:
                    raise ValueError("Mobject already belongs to another Scene")
            for value in values:
                scene._register_top_level(value)
            return

        if kind == "remove":
            removed = {id(value) for value in values}
            scene._test_membership = [
                value for value in scene._test_membership if id(value) not in removed
            ]
            return
        if kind == "clear":
            scene._test_membership.clear()
            return
        if kind == "replace":
            old, new = values
            previous = list(scene._test_membership)
            edit(scene, "add", (new,))
            scene._test_membership = [
                new if value is old else value for value in previous
            ]
            return
        raise ValueError(f"unknown test membership operation {kind!r}")

    def register(scene, value):
        if not hasattr(scene, "_test_membership"):
            scene._test_membership = []
        if not any(existing is value for existing in scene._test_membership):
            scene._test_membership.append(value)

    compat.Scene._edit_membership = edit
    compat.Scene._register_top_level = register
    compat.Scene.mobjects = property(lambda scene: list(getattr(scene, "_test_membership", ())))
