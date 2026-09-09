"""Inert subset-display requests over shared Rust family/lifecycle semantics.

Python retains constructor ergonomics and wrapper identity. Rust prepares the
family, validates lifecycle, resolves thresholds and owns playback/publication.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_animate as _animate
import _manim_compat as _compat

_INSTALLED = False


def _int_func_mode(value: object) -> str:
    name = getattr(value, "__name__", None)
    if value is math.floor or name == "floor":
        return "floor"
    if value is math.ceil or name == "ceil":
        return "ceil"
    raise NotImplementedError(
        "subset-display animations currently support only floor/ceil int_func semantics"
    )


def _subset_family(group: object) -> tuple[_compat.Group, list[_base.Mobject], object]:
    if not isinstance(group, _compat.Group):
        raise TypeError("subset-display animation requires a Group or VGroup")
    if not group.submobjects:
        raise ValueError("subset-display animation requires at least one direct submobject")
    members: list[_base.Mobject] = []
    for member in group.submobjects:
        if isinstance(member, _compat.Group) or not isinstance(member, _base.Mobject):
            raise NotImplementedError(
                "nested retained families are not yet supported by subset-display parity"
            )
        members.append(member)
    family = getattr(group, "_semantic_family_handle", None)
    if family is None:
        raise NotImplementedError(
            "subset-display animation requires an ordinary typed family"
        )
    return group, members, family


def _prepare_subset_family(group: object) -> tuple[_compat.Group, list[_base.Mobject]]:
    typed_group, members, family = _subset_family(group)
    bound_scenes = {member._scene for member in members if member._scene is not None}
    if len(bound_scenes) > 1:
        raise ValueError("subset-display family members belong to different Scenes")
    try:
        # Live-created members remain detached until the subset animation admits
        # them, but their semantic mutations still belong to the active session.
        # Use the same family context resolution as shared arrangement/targets.
        import _manim_semantic_handles as _semantic_handles

        context = _semantic_handles._group_target_context(typed_group)
        if context is not None:
            context.prepareFamilySubsetDisplay(family)
        elif bound_scenes:
            scene = next(iter(bound_scenes))
            context = getattr(scene, "_canonical_authoring_context", None)
            if context is None:
                raise NotImplementedError(
                    "already-bound subset-display families require canonical shared execution"
                )
            context.prepareFamilySubsetDisplay(family)
        else:
            family.prepareSubsetDisplay()
    except Exception as error:
        raise ValueError(str(error)) from None
    return typed_group, members


class ShowIncreasingSubsets:
    """ManimCE v0.21 direct-leaf subset display with exact discrete thresholds."""

    def __init__(
        self,
        group: object,
        suspend_mobject_updating: bool = False,
        int_func: object = math.floor,
        reverse_rate_function: bool = False,
        **kwargs: Any,
    ) -> None:
        if suspend_mobject_updating:
            raise NotImplementedError(
                "suspend_mobject_updating=True is not yet supported for subset-display parity"
            )
        if reverse_rate_function:
            raise NotImplementedError(
                "reverse_rate_function=True is not yet supported for subset-display parity"
            )
        if _int_func_mode(int_func) != "floor":
            raise NotImplementedError(
                "ShowIncreasingSubsets currently supports only its floor int_func"
            )
        self.mode = "increasing-floor"
        self.group, _ = _prepare_subset_family(group)
        self.mobject = self.group
        self.anim_args = dict(kwargs)


class ShowSubmobjectsOneByOne(ShowIncreasingSubsets):
    """Show exactly one direct child at a time with Manim's default ceil semantics."""

    def __init__(
        self,
        group: object,
        int_func: object = math.ceil,
        **kwargs: Any,
    ) -> None:
        try:
            members = list(group)  # type: ignore[arg-type]
        except TypeError as error:
            raise TypeError("ShowSubmobjectsOneByOne group must be iterable") from error
        if _int_func_mode(int_func) != "ceil":
            raise NotImplementedError(
                "ShowSubmobjectsOneByOne currently supports only its ceil int_func"
            )
        self.mode = "one-by-one-ceil"
        self.group, _ = _prepare_subset_family(_compat.Group(*members))
        self.mobject = self.group
        self.anim_args = dict(kwargs)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    public = {
        "ShowIncreasingSubsets": ShowIncreasingSubsets,
        "ShowSubmobjectsOneByOne": ShowSubmobjectsOneByOne,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        setattr(_animate, name, value)
    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports

install()
