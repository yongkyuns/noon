"""Python callback protocol over shared Rust own-point promotion."""
from inspect import getattr_static


def _uses_default_protocol(value):
    import noon
    import _manim_compat as compat

    # Inspect descriptors without calling them before the public copy/reset/add
    # sequence. Custom hooks retain their ordinary Python invocation order.
    if not any(cls is noon.Mobject for cls in type.__getattribute__(type(value), "__mro__")):
        return False
    defaults = {
        "__getattribute__": (object.__getattribute__,),
        "__setattr__": (object.__setattr__,),
        "copy": (noon.Mobject.copy, compat.VMobject.copy, compat.Group.copy),
        "reset_points": (noon.Mobject.reset_points,),
        "add": (noon.Mobject.add, compat.Group.add),
        "submobjects": (noon.Mobject.submobjects, compat.Group.submobjects),
        "_assert_valid_submobjects": (
            noon.Mobject._assert_valid_submobjects, compat.VMobject._assert_valid_submobjects,
        ),
        "_assert_valid_submobjects_internal": (noon.Mobject._assert_valid_submobjects_internal,),
    }
    return all(any(getattr_static(value, name, None) is expected for expected in choices)
               for name, choices in defaults.items())


def push_self_into_submobjects(value):
    if _uses_default_protocol(value):
        from _manim_semantic_handles import _group_copy_operation
        _group_copy_operation(value, cyclic_replace=False, promote_own_points=True)
    else:
        clone = value.copy()
        clone.submobjects = []
        value.reset_points()
        value.add(clone)
    return value
