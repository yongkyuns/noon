#!/usr/bin/env python3
"""Facade lifecycle unit tests. These do not emulate or qualify Rust rendering."""
import asyncio
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
from unittest import TestCase, main, mock

HOST = Path(__file__).resolve().parents[1] / 'web/python/_noon_native_host.py'

class FacadeTests(TestCase):
    def setUp(self):
        self.events = []
        events = self.events
        class Video:
            def __init__(self, path, **options):
                events.append(('new', path, options))
            def finish(self):
                events.append('finish')
                return {'frames': 3}
            def abort(self): events.append('abort')
            def drive(self, context):
                events.append(('drive', context))
                return 'video-event'
        self.native = SimpleNamespace(VideoExport=Video, GeometryOptions=object,
                                      resolve_animation_options=object, resolve_transform_options=object)
        spec = importlib.util.spec_from_file_location('export_facade_under_test', HOST)
        self.host = importlib.util.module_from_spec(spec)
        with mock.patch.dict(sys.modules, {'_noon_native': self.native}):
            spec.loader.exec_module(self.host)

    def test_source_finishes_before_file_and_context_is_restored(self):
        scene = object()
        async def execute(*args, **kwargs):
            self.assertIsNotNone(self.host._video.get())
            self.events.append('source')
            return scene
        with mock.patch.object(self.host, 'run_scene', execute), mock.patch.object(self.host, 'close_scene') as close:
            result = asyncio.run(self.host.export_scene(object, 'out.mp4', fps=(60000, 1001)))
            self.assertEqual(result, {'frames': 3})
            self.assertEqual(self.events[1:], ['source', 'finish', 'abort'])
            self.assertEqual(self.events[0][2]['p'], 60000)
            self.assertEqual(self.events[0][2]['q'], 1001)
            close.assert_called_once_with(scene)
        self.assertIsNone(self.host._video.get())

    def test_source_error_and_cancellation_do_not_finish(self):
        for failure in (ValueError('source failed'), asyncio.CancelledError()):
            self.events.clear()
            async def execute(*args, **kwargs): raise failure
            with mock.patch.object(self.host, 'run_scene', execute):
                with self.assertRaises(type(failure)):
                    asyncio.run(self.host.export_scene(object, 'out.mp4'))
            self.assertNotIn('finish', self.events)
            self.assertEqual(self.events[-1], 'abort')
            self.assertIsNone(self.host._video.get())

    def test_source_mode_and_filename_are_forwarded(self):
        scene = object()
        async def execute(*args, **kwargs):
            self.assertEqual(args, ('source bytes', {'name': 'value'}))
            self.assertEqual(kwargs, {'portable': True, 'filename': 'scene.py'})
            return scene
        with mock.patch.object(self.host, 'run_source', execute), mock.patch.object(self.host, 'close_scene'):
            asyncio.run(self.host.export_source('source bytes', 'out', {'name': 'value'},
                                               portable=True, filename='scene.py', png=True))
        self.assertIsNone(self.host._video.get())

    def test_non_export_drive_is_unchanged_and_export_is_scoped(self):
        context = SimpleNamespace(drive=lambda: 'ordinary-event')
        self.assertEqual(self.host._drive(context), 'ordinary-event')
        token = self.host._video.set(self.native.VideoExport('out'))
        try: self.assertEqual(self.host._drive(context), 'video-event')
        finally: self.host._video.reset(token)
        self.assertEqual(self.host._drive(context), 'ordinary-event')

    def test_missing_feature_and_nested_export_fail_before_source(self):
        del self.native.VideoExport
        with self.assertRaisesRegex(RuntimeError, '--export-video'):
            asyncio.run(self.host.export_scene(object, 'out'))
        token = self.host._video.set(object())
        try:
            with self.assertRaisesRegex(RuntimeError, 'nested'):
                asyncio.run(self.host.export_scene(object, 'out'))
        finally: self.host._video.reset(token)
        self.assertEqual(self.events, [])

    def test_cleanup_error_does_not_leak_contextvar(self):
        scene = object()
        async def execute(*args, **kwargs): return scene
        with mock.patch.object(self.host, 'run_scene', execute), mock.patch.object(self.host, 'close_scene', side_effect=RuntimeError('close')):
            with self.assertRaisesRegex(RuntimeError, 'close'):
                asyncio.run(self.host.export_scene(object, 'out'))
        self.assertIsNone(self.host._video.get())

if __name__ == '__main__': main()
