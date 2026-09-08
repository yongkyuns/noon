import asyncio
import inspect
import textwrap
import sys
import subprocess
from pathlib import Path
import unittest

from _manim_source_execution import (
    BARRIER_GLOBAL, bind_portable_construct, compile_authoring_source,
)


class SourceExecutionTests(unittest.IsolatedAsyncioTestCase):
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


    def run_canonical_contract(self, contract):
        python_dir = Path(__file__).resolve().parent
        worker = (python_dir.parent / "python-worker.source.js").read_text()
        bootstrap = worker.split("  pyodide.runPython(`", 1)[1].split("`);", 1)[0]
        source = textwrap.dedent("""
            import asyncio, sys, types
            from unittest.mock import patch
            fake_js = types.ModuleType("js")
            def unavailable(*args):
                raise AssertionError("host-control test must not fabricate Rust semantics")
            fake_js.__getattr__ = lambda name: unavailable
            sys.modules["js"] = fake_js
        """) + f"\nsys.path.insert(0, {str(python_dir)!r})\n" + bootstrap
        source += "\nimport _manim_canonical_scene as canonical\nimport noon\n"
        source += textwrap.dedent(contract)
        result = subprocess.run([sys.executable, "-c", source], capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_actual_canonical_wait_uses_same_awaitable_and_cleans_up(self):
        self.run_canonical_contract(r'''
            from _manim_source_execution import BARRIER_GLOBAL, compile_authoring_source, bind_portable_construct
            events = []
            class Context:
                def beginOrdinaryWait(self, duration):
                    events.append(("begin", duration))
            async def complete(scene):
                assert not getattr(scene, canonical._PORTABLE_BARRIER_CALL)
                events.append("completed")
            source = "class Example(Scene):\n    def construct(self):\n        self.wait(0.5)\n        events.append('after')\n"
            code, pairs = compile_authoring_source(source)
            namespace = {"Scene": noon.Scene, "events": events, BARRIER_GLOBAL: canonical.await_source_barrier}
            exec(code, namespace)
            scene = namespace["Example"]()
            scene._canonical_authoring_context = Context()
            portable = bind_portable_construct(scene.construct, pairs)
            async def main():
                with (patch.object(canonical, "_require_semantic_continuation_active"),
                      patch.object(canonical, "_prepare_semantic_continuation_callbacks"),
                      patch.object(canonical, "_await_semantic_continuation", complete)):
                    await canonical.execute_construct(scene, portable_constructs=pairs)
            asyncio.run(main())
            assert events == [("begin", 0.5), "completed", "after"], events
            assert not getattr(scene, canonical._PORTABLE_CONSTRUCT_MODE)
            assert not getattr(scene, canonical._ASYNC_CONTINUATION_MODE)
        ''')

    def test_uncompiled_nested_barrier_rejects_before_mutation(self):
        self.run_canonical_contract(r'''
            scene = noon.Scene()
            setattr(scene, canonical._PORTABLE_CONSTRUCT_MODE, True)
            for method, argument in [(canonical._play, object()), (canonical._canonical_wait, 1)]:
                try:
                    method(scene, argument)
                except RuntimeError as error:
                    assert "indirect synchronous" in str(error), str(error)
                else:
                    raise AssertionError("uncompiled barrier was admitted")
            assert getattr(scene, "_canonical_authoring_context", None) is None
            async def main():
                try:
                    await canonical.await_source_barrier(lambda: None)
                except RuntimeError as error:
                    assert "current Scene" in str(error), str(error)
                else:
                    raise AssertionError("unrelated method was admitted")
            asyncio.run(main())
        ''')
