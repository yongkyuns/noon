"""Native source execution through the common Python compiler/Scene lifecycle.

This executes Python, not a second scene compiler. Scene operations and callbacks
continue to use the shared Rust engine through the selected binding.
"""
from __future__ import annotations

from _manim_scene import (
    execute_construct, await_source_barrier, await_module_source_barrier,
)
from _manim_source_execution import (
    BARRIER_GLOBAL, MODULE_BARRIER_GLOBAL, COOPERATIVE_CHECKPOINT_GLOBAL,
    await_source_checkpoint, compile_authoring_source,
    authoring_source_scope, execute_authoring_module,
)
from noon import Scene


async def execute_source(source: str, context=None, *, portable: bool = True,
                         filename: str = "<noon>", scene_name: str | None = None) -> Scene:
    if not isinstance(source, str) or not source.strip():
        raise TypeError("Python authoring source must be non-empty")
    code, portable_constructs = compile_authoring_source(source, filename, portable=portable)
    return await execute_compiled_source(code, portable_constructs, context, scene_name=scene_name)


async def execute_compiled_source(code, portable_constructs, context=None, *, scene_name=None) -> Scene:
    """Execute a once-compiled source through the common host lifecycle."""
    if scene_name is not None and (not isinstance(scene_name, str) or not scene_name.isidentifier()):
        raise ValueError("scene_name must be a Python class identifier")
    namespace = {"context": {} if context is None else context, "__name__": "__main__"}
    if not code.co_filename.startswith("<"):
        namespace["__file__"] = code.co_filename
    if portable_constructs:
        namespace[BARRIER_GLOBAL] = await_source_barrier
    namespace[COOPERATIVE_CHECKPOINT_GLOBAL] = await_source_checkpoint
    if portable_constructs or MODULE_BARRIER_GLOBAL in code.co_names:
        namespace[MODULE_BARRIER_GLOBAL] = await_module_source_barrier
    with authoring_source_scope():
        await execute_authoring_module(code, namespace)
    selected = None
    if scene_name is not None:
        selected = namespace.get(scene_name)
        if not isinstance(selected, type) or not issubclass(selected, Scene) or selected is Scene:
            raise ValueError(f"{scene_name!r} does not name a Scene subclass in this source")
    elif "result" in namespace:
        result = namespace["result"]
    else:
        classes = [value for value in namespace.values()
                   if isinstance(value, type) and issubclass(value, Scene)
                   and value is not Scene and getattr(value, "__module__", None) == "__main__"]
        if not classes:
            raise RuntimeError("Python authoring source must either assign result or define one Scene subclass")
        if len(classes) != 1:
            names = ", ".join(cls.__name__ for cls in classes)
            raise RuntimeError("Python authoring source defines multiple Scene subclasses; "
                               f"select one explicitly via result = SceneClass(): {names}")
        selected = classes[0]
    if selected is not None:
        result = selected()
        await execute_construct(result, portable_constructs=portable_constructs)
    if not isinstance(result, Scene):
        raise TypeError("Python authoring result must be a noon.Scene")
    return result
