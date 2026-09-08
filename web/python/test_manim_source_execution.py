import asyncio
import inspect
import textwrap
from unittest.mock import patch
import unittest

from _manim_source_execution import (
    BARRIER_GLOBAL, bind_portable_construct, compile_authoring_source,
    has_portable_scene_methods,
    authoring_source_scope, current_source_invocation,
)


class SourceExecutionTests(unittest.IsolatedAsyncioTestCase):
    async def test_source_invocation_restores_mode_and_cleanup_after_failure(self):
        cleanups = []
        self.assertIsNone(current_source_invocation())
        with authoring_source_scope():
            ordinary = current_source_invocation()
            self.assertFalse(ordinary.export_document)
            ordinary.cleanup.callback(cleanups.append, "ordinary")
            with self.assertRaisesRegex(RuntimeError, "source failed"):
                with authoring_source_scope(export_document=True):
                    exported = current_source_invocation()
                    self.assertTrue(exported.export_document)
                    exported.cleanup.callback(cleanups.append, "export")
                    await asyncio.sleep(0)
                    raise RuntimeError("source failed")
            self.assertIs(current_source_invocation(), ordinary)
            self.assertEqual(cleanups, ["export"])
        self.assertEqual(cleanups, ["export", "ordinary"])
        self.assertIsNone(current_source_invocation())

    async def test_source_invocation_modes_are_isolated_between_tasks(self):
        async def observe(export_document):
            with authoring_source_scope(export_document=export_document):
                await asyncio.sleep(0)
                return current_source_invocation().export_document
        self.assertEqual(await asyncio.gather(observe(True), observe(False)), [True, False])
        self.assertIsNone(current_source_invocation())

    def compile_scene(self, source, namespace=None):
        namespace = {} if namespace is None else namespace
        code, pairs = compile_authoring_source(textwrap.dedent(source))
        exec(code, namespace)
        scene = namespace["Example"]()
        return scene, bind_portable_construct(scene.construct, pairs), namespace

    async def test_each_barrier_waits_before_later_source_runs(self):
        events = []
        release = asyncio.Event()

        class Base:
            def play(self, *args, **kwargs):
                events.append(("play", args, kwargs))

            def wait(self, duration):
                events.append(("wait", duration))

        async def await_barrier(method, *args, **kwargs):
            method(*args, **kwargs)
            await release.wait()
            events.append("completed")

        scene, portable, _ = self.compile_scene('''
            class Example(Base):
                def construct(self):
                    events.append("before")
                    self.play(argument(), *[2], run_time=keyword())
                    events.append("after play")
                    self.wait(0.5)
                    events.append("after wait")
        ''', {"Base": Base, "events": events,
              "argument": lambda: events.append("argument") or 1,
              "keyword": lambda: events.append("keyword") or 3,
              BARRIER_GLOBAL: await_barrier})
        self.assertFalse(inspect.iscoroutinefunction(scene.construct))
        self.assertTrue(inspect.iscoroutinefunction(portable))
        task = asyncio.create_task(portable())
        await asyncio.sleep(0)
        self.assertEqual(events, ["before", "argument", "keyword", ("play", (1, 2), {"run_time": 3})])
        release.set()
        await task
        self.assertEqual(events[-5:], ["completed", "after play", ("wait", 0.5), "completed", "after wait"])

    async def test_globals_closure_defaults_and_definition_effects_are_not_replayed(self):
        effects = []
        async def await_barrier(method, *args, **kwargs):
            return method(*args, **kwargs)
        scene, portable, _ = self.compile_scene('''
            effects.append("module")
            def factory():
                closed = [7]
                class Example:
                    effects.append("class")
                    def construct(self, default=effects.append("default") or 2, *, kw=3):
                        self.wait(closed[0] + default + kw)
                    def wait(self, value):
                        effects.append(value)
                return Example
            Example = factory()
        ''', {"effects": effects, BARRIER_GLOBAL: await_barrier})
        self.assertEqual(effects, ["module", "class", "default"])
        await portable()
        self.assertEqual(effects, ["module", "class", "default", 12])
        self.assertEqual(scene.construct.__func__.__defaults__, (2,))

    async def test_loop_branches_exception_and_finally_order(self):
        events = []
        class Base:
            def play(self, number):
                events.append(number)
                if number == 2:
                    raise ValueError("stop")
        async def barrier(method, *args, **kwargs):
            return method(*args, **kwargs)
        _, portable, _ = self.compile_scene('''
            class Example(Base):
                def construct(self):
                    try:
                        for i in range(4):
                            if i:
                                self.play(i)
                    except ValueError as error:
                        events.append(str(error))
                    finally:
                        events.append("finally")
        ''', {"Base": Base, "events": events, BARRIER_GLOBAL: barrier})
        await portable()
        self.assertEqual(events, [1, 2, "stop", "finally"])

    async def test_cancellation_does_not_continue_or_replay(self):
        events = []
        class Base:
            def wait(self, duration):
                events.append("admitted")
        async def barrier(method, *args, **kwargs):
            method(*args, **kwargs)
            await asyncio.Event().wait()
        _, portable, _ = self.compile_scene('''
            class Example(Base):
                def construct(self):
                    try:
                        self.wait(1)
                        events.append("late")
                    finally:
                        events.append("finally")
        ''', {"Base": Base, "events": events, BARRIER_GLOBAL: barrier})
        task = asyncio.create_task(portable())
        await asyncio.sleep(0)
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task
        self.assertEqual(events, ["admitted", "finally"])

    def test_indirect_and_ambiguous_constructs_keep_original_execution(self):
        bodies = [
            "p = self.play\np(1)",
            "result = self.play(1)",
            "self.play(1)\nhelper(self)",
            "self.play(1)\nself.helper()",
            "self.play(1)\nsuper().construct()",
            "self.play(1)\nself.play = replacement",
            "self.play(1)\nself.add = replacement",
            "self.play(1)\ndel self.clear",
            "self.play(1)\nself.__dict__['wait'] = replacement",
            "self.play(1)\nyield 2",
            "self.play(1)\nf = lambda: self.wait(1)",
            "self.play(1)\ndef helper():\n    self.wait(1)",
            "self.play(1)\neval('self.wait(1)')",
            "self.play(1)\n_noon_await_source_barrier = 1",
        ]
        for body in bodies:
            with self.subTest(body=body):
                source = "class Example:\n    def construct(self):\n" + textwrap.indent(body, "        ")
                _, pairs = compile_authoring_source(source)
                self.assertEqual(pairs, {})
        _, pairs = compile_authoring_source("class Example:\n    async def construct(self):\n        await self.wait(1)\n")
        self.assertEqual(pairs, {})

    def test_decorators_are_not_replayed_or_unwrapped(self):
        effects = []
        def decorate(function):
            effects.append("decorated")
            return function
        code, pairs = compile_authoring_source('class Example:\n    @decorate\n    def construct(self):\n        self.wait(1)\n')
        namespace = {"decorate": decorate}
        exec(code, namespace)
        self.assertEqual(effects, ["decorated"])
        self.assertIsNone(bind_portable_construct(namespace["Example"]().construct, pairs))

    def test_errors_keep_original_source_line(self):
        with self.assertRaises(SyntaxError) as caught:
            compile_authoring_source("class Example:\n    def construct(self):\n        self.play(\n", "user-scene.py")
        self.assertEqual(caught.exception.filename, "user-scene.py")
        self.assertEqual(caught.exception.lineno, 3)


    def test_static_async_and_export_source_never_allocates_an_ast(self):
        cases = [
            ("\n".join(f"value_{i} = {i}" for i in range(1000)), True),
            ("class Example:\n    def construct(self):\n        self.add(1)\n", True),
            ("class Example:\n    async def construct(self):\n        await self.wait(1)\n", True),
            ("class Example:\n    def construct(self):\n        self.wait(1)\n", False),
        ]
        for source, portable in cases:
            with self.subTest(source=source[:60], portable=portable):
                with patch("_manim_source_execution.ast.parse", side_effect=AssertionError("AST allocated")):
                    code, pairs = compile_authoring_source(source, portable=portable)
                self.assertEqual(pairs, {})
                self.assertEqual(code, compile(source, "<string>", "exec", dont_inherit=True))

    def test_instance_class_and_rebound_method_overrides_are_not_admitted(self):
        class Base:
            def play(self, *args): pass
            def wait(self, *args): pass
        scene = Base()
        self.assertTrue(has_portable_scene_methods(scene, play=Base.play, wait=Base.wait))
        for name in ("play", "wait"):
            for replacement in (lambda *args: None, getattr(Base(), name)):
                with self.subTest(name=name, replacement=replacement):
                    scene = Base()
                    setattr(scene, name, replacement)
                    self.assertFalse(has_portable_scene_methods(scene, play=Base.play, wait=Base.wait))
        class Override(Base):
            def wait(self, *args): pass
        self.assertFalse(has_portable_scene_methods(Override(), play=Base.play, wait=Base.wait))

    def test_admission_does_not_invoke_descriptors_or_dynamic_lookup(self):
        effects = []
        class Base:
            def play(self, *args): pass
            def wait(self, *args): pass
        class Descriptor(Base):
            @property
            def wait(self):
                effects.append("get wait")
                return lambda *args: None
        class Dynamic(Base):
            def __getattribute__(self, name):
                effects.append(name)
                return super().__getattribute__(name)
        for scene in (Descriptor(), Dynamic()):
            self.assertFalse(has_portable_scene_methods(scene, play=Base.play, wait=Base.wait))
        self.assertEqual(effects, [])

    def test_membership_overrides_cannot_hide_uncompiled_barriers(self):
        class Base:
            def play(self, *args): pass
            def wait(self, *args): pass
            def add(self, *args): pass
            def remove(self, *args): pass
            def clear(self, *args): pass
        methods = {name: getattr(Base, name) for name in ("play", "wait", "add", "remove", "clear")}
        self.assertTrue(has_portable_scene_methods(Base(), **methods))
        for name in ("add", "remove", "clear"):
            with self.subTest(name=name):
                overridden = type("Example", (Base,), {name: lambda self: self.wait(1)})
                self.assertFalse(has_portable_scene_methods(overridden(), **methods))

    async def test_lambda_callbacks_keep_identity_closure_and_synchronous_results(self):
        events = []
        class Base:
            def play(self, callback):
                self.callback = callback
                self.assertion = callback(3)
            def wait(self, duration):
                events.append(self.callback(4))
        async def barrier(method, *args, **kwargs):
            return method(*args, **kwargs)
        scene, portable, _ = self.compile_scene('''
            class Example(Base):
                def construct(self):
                    value = 2
                    callback = lambda item: item + value
                    events.append(callback)
                    self.play(callback)
                    value = 7
                    self.wait(1)
                    events.append(callback)
        ''', {"Base": Base, "events": events, BARRIER_GLOBAL: barrier})
        self.assertIsNotNone(portable)
        await portable()
        self.assertEqual(scene.assertion, 5)
        self.assertEqual(events[1], 11)
        self.assertIs(events[0], events[2])
        self.assertIs(events[0], scene.callback)
        self.assertFalse(inspect.iscoroutinefunction(scene.callback))

    async def test_nested_callback_definitions_and_defaults_run_once(self):
        events = []
        class Base:
            def play(self, callback):
                events.append(callback(2))
            def wait(self, duration): pass
        async def barrier(method, *args, **kwargs):
            return method(*args, **kwargs)
        _, portable, _ = self.compile_scene('''
            class Example(Base):
                def construct(self):
                    offset = 3
                    def callback(value, scale=events.append("default") or 4):
                        return value * scale + offset
                    self.play(callback)
                    offset = 5
                    self.play(callback)
                    self.wait(0.5)
        ''', {"Base": Base, "events": events, BARRIER_GLOBAL: barrier})
        self.assertEqual(events, [])
        await portable()
        self.assertEqual(events, ["default", 11, 13])

    def test_callbacks_with_hidden_scene_barriers_or_introspection_stay_original(self):
        bodies = [
            "lambda: other.wait(1)", "lambda: globals()['self']", "lambda: self",
            "lambda: helper(self)", "lambda: eval('scene.play(1)')",
        ]
        for callback in bodies:
            with self.subTest(callback=callback):
                _, pairs = compile_authoring_source(
                    f"class Example:\n    def construct(self):\n        f = {callback}\n        self.wait(1)\n"
                )
                self.assertEqual(pairs, {})
        _, pairs = compile_authoring_source('''class Example:
    def construct(self):
        def helper():
            return other.play(1)
        self.wait(1)
''')
        self.assertEqual(pairs, {})

    async def test_module_barriers_preserve_namespace_definition_effects_and_order(self):
        from _manim_source_execution import MODULE_BARRIER_GLOBAL, execute_authoring_module
        events = []
        release = asyncio.Event()
        class Base:
            def play(self, value, **kwargs):
                events.append((value, kwargs))
        async def barrier(method, /, *args, **kwargs):
            method(*args, **kwargs)
            await release.wait()
            events.append("done")
        code, pairs = compile_authoring_source(textwrap.dedent('''
            events.append("module")
            class Example(Base):
                events.append("class")
                def construct(self, value=events.append("default") or 1):
                    self.play(value)
            scene = Example()
            scene.play(7, method=9)
            after = 42
            result = scene
        '''))
        namespace = {"Base": Base, "events": events,
                     BARRIER_GLOBAL: barrier, MODULE_BARRIER_GLOBAL: barrier}
        task = asyncio.create_task(execute_authoring_module(code, namespace))
        await asyncio.sleep(0)
        self.assertEqual(events, ["module", "class", "default", (7, {"method": 9})])
        self.assertNotIn("after", namespace)
        release.set()
        await task
        self.assertEqual(namespace["after"], 42)
        self.assertIs(namespace["result"], namespace["scene"])
        portable = bind_portable_construct(namespace["scene"].construct, pairs)
        self.assertIsNotNone(portable)
        await portable()
        self.assertEqual(events, ["module", "class", "default", (7, {"method": 9}), "done", (1, {}), "done"])

    async def test_module_cancellation_unwinds_scope_without_later_source_or_replay(self):
        from _manim_source_execution import MODULE_BARRIER_GLOBAL, execute_authoring_module
        events = []
        class Base:
            def wait(self, value): events.append(value)
        async def barrier(method, /, *args, **kwargs):
            method(*args, **kwargs)
            await asyncio.Event().wait()
        code, _ = compile_authoring_source('''try:
    scene.wait(1)
    events.append("late")
finally:
    events.append("finally")
''')
        async def run():
            with authoring_source_scope():
                current_source_invocation().cleanup.callback(events.append, "cleanup")
                await execute_authoring_module(code, {"scene": Base(), "events": events, MODULE_BARRIER_GLOBAL: barrier})
        task = asyncio.create_task(run())
        await asyncio.sleep(0)
        task.cancel()
        with self.assertRaises(asyncio.CancelledError): await task
        self.assertEqual(events, [1, "finally", "cleanup"])
        self.assertIsNone(current_source_invocation())

    async def test_module_dispatch_does_not_await_arbitrary_return_values(self):
        from _manim_source_execution import MODULE_BARRIER_GLOBAL, execute_authoring_module
        events = []
        class Returned:
            def __await__(self):
                events.append("unexpected await")
                yield
        class Other:
            def play(self):
                events.append("called")
                return Returned()
        async def barrier(method, /, *args, **kwargs):
            return method(*args, **kwargs)
        source = 'other.play()\nevents.append("after")'
        code, _ = compile_authoring_source(source)
        await execute_authoring_module(code, {"other": Other(), "events": events, MODULE_BARRIER_GLOBAL: barrier})
        self.assertEqual(events, ["called", "after"])
        with patch("_manim_source_execution.ast.parse", side_effect=AssertionError("AST allocated")):
            code, pairs = compile_authoring_source(source, portable=False)
        self.assertFalse(code.co_flags & inspect.CO_COROUTINE)
        self.assertEqual(pairs, {})

    def test_module_compiler_leaves_deferred_calls_and_return_values_untouched(self):
        for source in [
            'def helper():\n    scene.play(1)\n',
            'class Other:\n    def helper(self):\n        scene.play(1)\n',
            'result = other.wait(1)',
        ]:
            with self.subTest(source=source):
                code, pairs = compile_authoring_source(source)
                self.assertFalse(code.co_flags & inspect.CO_COROUTINE)
                self.assertEqual(pairs, {})
