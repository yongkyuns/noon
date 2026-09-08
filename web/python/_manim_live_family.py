"""Live-session family construction over the canonical semantic owner.

The shared semantic-handle layer owns detached family identity. Once canonical
execution has started, however, creating a family through the raw authoring store
would advance semantic revision outside the retained execution mutation boundary.
This adapter routes those late Group/VGroup creations and constructor-free copies
through the canonical context returned by the current authoring Scene.
"""

from __future__ import annotations

import sys

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _handles

_INSTALLED = False


def _live_constructor_context(kind: str = "primitive"):
    """Return the retained context allowed to publish a new semantic object."""
    reactive = sys.modules.get("_manim_reactive")
    if reactive is None:
        return None
    scene = reactive._current_authoring_scene()
    context = getattr(scene, "_canonical_authoring_context", None)
    if context is None:
        return None
    ownership = str(context.liveExecutionOwnership())
    if ownership in {"active", "returned"}:
        return context
    if ownership == "transferred":
        raise RuntimeError(
            f"live {kind} construction is unavailable while execution is transferred"
        )
    return None


def _publish_live_family(
    self: _compat.Group, mobjects: tuple[object, ...], context: object
) -> None:
    """Publish once through Rust, then mirror its authoritative direct-member order."""
    _handles._validate_group_members(self, mobjects)
    batch = context.beginMembershipBatch("add")
    wrappers: dict[tuple[int, int], object] = {}
    for member in mobjects:
        kind, handle = _handles._family_member_handle(member)
        if handle is None:
            raise RuntimeError("family member has no shared semantic identity")
        key = (int(handle.semanticSlot), int(handle.semanticGeneration))
        wrappers.setdefault(key, member)
        if kind == "family":
            batch.appendFamily(handle)
        elif kind == "mobject":
            batch.appendMobject("", handle)
        else:
            raise RuntimeError("unsupported family member kind")

    family = context.createLiveFamily(batch)
    mirrored = [
        wrappers[(int(family.memberSlot(index)), int(family.memberGeneration(index)))]
        for index in range(int(family.memberCount))
    ]
    self._semantic_family_handle = family
    self.submobjects = mirrored


def _group_init(self: _compat.Group, *mobjects: object) -> None:
    context = _live_constructor_context("family")
    if context is not None:
        _publish_live_family(self, mobjects, context)
        return
    self._semantic_family_handle = _handles._create_family_handle()
    _handles._ORIGINAL_GROUP_INIT(self, *mobjects)


def _group_copy(self: _compat.Group) -> _compat.Group:
    if _handles._GROUP_TARGET_COPY.get():
        return _handles._group_target_copy(self)
    delegate = _handles._GROUP_COPY_DELEGATE
    if delegate is None:
        raise RuntimeError("shared Group copy delegate is not installed")

    family_handle = self.__dict__.pop("_semantic_family_handle", None)
    try:
        clone = delegate(self)
    finally:
        if family_handle is not None:
            self._semantic_family_handle = family_handle

    if getattr(clone, "_semantic_family_handle", None) is None:
        context = _live_constructor_context("family")
        if context is not None:
            _publish_live_family(clone, tuple(clone.submobjects), context)
        else:
            clone._semantic_family_handle = _handles._create_family_handle()
            for member in clone.submobjects:
                _handles._family_add_handle(clone._semantic_family_handle, member)
    return clone


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    # Keep the implementation visible through the semantic-handle module because
    # its target-copy path resolves this helper dynamically and wrapper-only tests
    # exercise the same public adapter boundary.
    _handles._live_constructor_context = _live_constructor_context
    _handles._publish_live_family = _publish_live_family
    _handles._group_init = _group_init
    _handles._group_copy = _group_copy
    _compat.Group.__init__ = _group_init
    _compat.Group.copy = _group_copy
