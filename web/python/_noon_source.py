"""One Python source/Scene lifecycle shared by CPython and Pyodide.

This executes Python, not a second scene compiler. Scene operations and callbacks
continue to use the shared Rust engine through the selected binding.
"""
from __future__ import annotations

from _manim_scene import (
    execute_construct, await_source_barrier, await_module_source_barrier,
)
from _manim_source_execution import (
    BARRIER_GLOBAL, MODULE_BARRIER_GLOBAL, compile_authoring_source,
    authoring_source_scope, execute_authoring_module,
)
from noon import Scene


async def execute_source(source: str, context=None, *, portable: bool = True,
                         filename: str = "<noon>", scene_name: str | None = None) -> Scene:
    if scene_name is not None and (not isinstance(scene_name, str) or not scene_name.isidentifier()):
        raise ValueError("scene_name must be a Python class identifier")
    if not isinstance(source, str) or not source.strip():
        raise TypeError("Python authoring source must be non-empty")
    namespace = {"context": {} if context is None else context, "__name__": "__main__"}
    code, portable_constructs = compile_authoring_source(source, filename, portable=portable)
    if portable_constructs:
        namespace[BARRIER_GLOBAL] = await_source_barrier
    if MODULE_BARRIER_GLOBAL in code.co_names:
        namespace[MODULE_BARRIER_GLOBAL] = await_module_source_barrier
    with authoring_source_scope():
        await execute_authoring_module(code, namespace)
    if scene_name is not None:
        if "result" in namespace:
            raise RuntimeError("named scene selection cannot be combined with a module-level result")
        selected = namespace.get(scene_name)
        if not isinstance(selected, type) or not issubclass(selected, Scene) or selected is Scene:
            raise ValueError(f"source does not define a Scene class named {scene_name!r}")
        result = selected()
        await execute_construct(result, portable_constructs=portable_constructs)
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
        result = classes[0]()
        await execute_construct(result, portable_constructs=portable_constructs)
    if not isinstance(result, Scene):
        raise TypeError("Python authoring result must be a noon.Scene")
    return result
