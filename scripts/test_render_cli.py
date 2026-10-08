#!/usr/bin/env python3
"""Syntax, delegation, source selection and lifecycle tests, not renderer mocks."""
from __future__ import annotations
import asyncio
from contextlib import nullcontext, redirect_stderr, redirect_stdout
import importlib.util
import io
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
from types import SimpleNamespace
from unittest import TestCase, main, mock

PYTHON = Path(__file__).resolve().parents[1] / 'web/python'
sys.path.insert(0, str(PYTHON))
import _noon_render_cli as cli
from _noon_render_options import resolve_render_options


class OptionProjectionTests(TestCase):
    def test_all_values_and_defaults_go_to_one_rust_call(self):
        result = SimpleNamespace(pixelWidth=322, pixelHeight=182,
                                 frameRateNumerator=123, frameRateDenominator=7, format='png')
        resolver = mock.Mock(return_value=result)
        with mock.patch.dict(sys.modules, {'_noon_host': SimpleNamespace(noonResolveRenderOptions=resolver)}):
            resolved = resolve_render_options(quality='l', resolution=(65, 33),
                                              frame_rate=(60000, 1001), format='png')
            resolver.assert_called_once_with('l', '65,33', '60000/1001', 'png', None, None)
            self.assertEqual((resolved.width, resolved.height, resolved.fps, resolved.format),
                             (322, 182, (123, 7), 'png'))
            resolver.reset_mock()
            resolve_render_options()
            resolver.assert_called_once_with(None, None, None, None, None, None)

    def test_numeric_coercion_does_not_guess_fps_or_preset_values(self):
        result = SimpleNamespace(pixelWidth=1, pixelHeight=1,
                                 frameRateNumerator=1, frameRateDenominator=1, format='mp4')
        resolver = mock.Mock(return_value=result)
        with mock.patch.dict(sys.modules, {'_noon_host': SimpleNamespace(noonResolveRenderOptions=resolver)}):
            from fractions import Fraction
            for value, expected in [(60, '60'), (29.97, '29.97'), ('6e1', '6e1'),
                                    (Fraction(60000, 1001), '60000/1001')]:
                resolve_render_options(frame_rate=value)
                self.assertEqual(resolver.call_args.args[2], expected)
            for value in [True, (30, True), (30, 1, 2), object()]:
                with self.assertRaises(TypeError):
                    resolve_render_options(frame_rate=value)
            for value in [-1, 1 << 32]:
                with self.assertRaises(ValueError):
                    resolve_render_options(pixel_width=value)
            with self.assertRaises(TypeError):
                resolve_render_options(pixel_height=True)

    def test_typed_rust_error_projection_is_preserved(self):
        error = RuntimeError('unsupported profile')
        error.noonErrorVersion = 1
        error.category = 'unsupported_operation'
        error.code = 'render.unsupported_format'
        error.message = str(error)
        with mock.patch.dict(sys.modules, {'_noon_host': SimpleNamespace(
                noonResolveRenderOptions=mock.Mock(side_effect=error))}):
            from _noon_errors import NoonUnsupportedError
            with self.assertRaises(NoonUnsupportedError) as caught:
                resolve_render_options(format='gif')
            self.assertEqual(caught.exception.code, 'render.unsupported_format')


class CliTests(TestCase):
    def test_supported_manim_flag_shapes_and_subcommand(self):
        for prefix in ([], ['render']):
            _, args = cli.parse_args([*prefix, '-pqh', 'scene.py', 'Demo', '--fps', '60', '-o', 'demo.mp4'])
            self.assertEqual((args.scene, args.quality, args.frame_rate), ('Demo', 'h', '60'))
            self.assertTrue(args.preview)
        _, args = cli.parse_args(['scene.py', 'Demo', '-r', '65,33', '--frame_rate', '29.97', '--format', 'png'])
        self.assertEqual((args.resolution, args.frame_rate, args.format), ('65,33', '29.97', 'png'))

    def test_unsupported_options_cannot_acquire_unrelated_meanings(self):
        for suffix in [['-n', '4'], ['-a'], ['-s'], ['-t'], ['--config_file', 'x.cfg'],
                       ['--renderer', 'cairo'], ['--format', 'png', '--png'], ['Demo', 'Other']]:
            with self.subTest(suffix=suffix), redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                cli.parse_args(['scene.py', *suffix])

    def test_help_never_requires_native_extension(self):
        with redirect_stdout(io.StringIO()), self.assertRaises(SystemExit) as error:
            cli.main(['--help'])
        self.assertEqual(error.exception.code, 0)

    def test_cli_forwards_resolved_values_and_only_previews_committed_output(self):
        events = []
        async def export(source, output, **kwargs):
            self.assertEqual(source, 'source text')
            self.assertEqual(kwargs['scene_name'], 'Demo')
            self.assertEqual((kwargs['width'], kwargs['height'], kwargs['fps']), (322, 182, (123, 7)))
            events.append('finalized')
            return {'frames': 3, 'path': str(output)}
        binding = SimpleNamespace(run_source=object, close_scene=object, export_source=export)
        resolved = SimpleNamespace(width=322, height=182, fps=(123, 7), format='mp4')
        with TemporaryDirectory() as temporary:
            source = Path(temporary) / 'scene.py'; source.write_text('source text')
            with mock.patch.dict(sys.modules, {'_noon_native_host': binding}), \
                 mock.patch('_noon_render_options.resolve_render_options', return_value=resolved) as resolve, \
                 mock.patch.object(cli, 'preview_file', side_effect=lambda path: events.append('preview')), \
                 redirect_stdout(io.StringIO()):
                self.assertEqual(cli.main(['-pqh', str(source), 'Demo', '--fps', '60', '-o', str(Path(temporary) / 'out')]), 0)
                self.assertEqual(resolve.call_args.kwargs['frame_rate'], '60')
        self.assertEqual(events, ['finalized', 'preview'])

    def test_bad_options_fail_before_source_loading_or_output(self):
        export = mock.AsyncMock()
        binding = SimpleNamespace(run_source=object, close_scene=object, export_source=export)
        with mock.patch.dict(sys.modules, {'_noon_native_host': binding}), \
             mock.patch('_noon_render_options.resolve_render_options', side_effect=ValueError('bad fps')), \
             redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            cli.main(['nonexistent.py', 'Demo', '--fps', 'nan'])
        export.assert_not_called()

    def test_failed_source_or_encoder_never_previews(self):
        binding = SimpleNamespace(run_source=object, close_scene=object,
                                  export_source=mock.AsyncMock(side_effect=RuntimeError('failed')))
        resolved = SimpleNamespace(width=64, height=32, fps=(60, 1), format='mp4')
        with TemporaryDirectory() as temporary:
            source = Path(temporary)/'x.py'; source.write_text('source')
            with mock.patch.dict(sys.modules, {'_noon_native_host': binding}), \
                 mock.patch('_noon_render_options.resolve_render_options', return_value=resolved), \
                 mock.patch.object(cli, 'preview_file') as preview, redirect_stderr(io.StringIO()), \
                 self.assertRaises(SystemExit):
                cli.main(['-p', str(source)])
            preview.assert_not_called()


class SourceSelectionTests(TestCase):
    def setUp(self):
        self.calls = []
        calls = self.calls
        class Scene: pass
        self.Scene = Scene
        async def construct(scene, **options):
            calls.append((type(scene).__name__, options))
            scene.construct()
        async def module(code, namespace): exec(code, namespace)
        self.portable = object()
        compiler = SimpleNamespace(BARRIER_GLOBAL='barrier', MODULE_BARRIER_GLOBAL='module_barrier',
            compile_authoring_source=lambda source, filename, portable: (compile(source, filename, 'exec'), self.portable),
            authoring_source_scope=nullcontext, execute_authoring_module=module)
        self.modules = {'noon': SimpleNamespace(Scene=Scene), '_manim_source_execution': compiler,
                        '_manim_scene': SimpleNamespace(execute_construct=construct,
                            await_source_barrier=object(), await_module_source_barrier=object())}
        spec = importlib.util.spec_from_file_location('source_selection_test', PYTHON/'_noon_source.py')
        self.source = importlib.util.module_from_spec(spec)
        with mock.patch.dict(sys.modules, self.modules): spec.loader.exec_module(self.source)

    def run_source(self, text, **options):
        with mock.patch.dict(sys.modules, self.modules):
            return asyncio.run(self.source.execute_source(text, **options))

    def test_selected_class_only_and_original_portable_mapping(self):
        text = """from noon import Scene
class Wrong(Scene):
    def construct(self): raise AssertionError('unselected scene executed')
class Demo(Scene):
    def construct(self): self.value = 7
"""
        result = self.run_source(text, scene_name='Demo')
        self.assertEqual(result.value, 7)
        self.assertEqual(self.calls, [('Demo', {'portable_constructs': self.portable})])
        with self.assertRaisesRegex(RuntimeError, 'multiple Scene'):
            self.run_source(text)
        with self.assertRaisesRegex(ValueError, 'named'):
            self.run_source(text, scene_name='Absent')
        self.assertEqual(len(self.calls), 1)

    def test_invalid_selector_and_result_conflict_never_construct_selected_scene(self):
        for name in ('', 'Demo()', 'a.b', 1):
            with self.assertRaises(ValueError): self.run_source('raise AssertionError()', scene_name=name)
        text = 'from noon import Scene\nclass Demo(Scene): pass\nresult = Scene()'
        with self.assertRaisesRegex(RuntimeError, 'module-level result'):
            self.run_source(text, scene_name='Demo')
        self.assertEqual(self.calls, [])
        self.assertIsInstance(self.run_source(text), self.Scene)


if __name__ == '__main__': main()
