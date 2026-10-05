"""Host symbol lifetime tests; real engine conformance lives in parity/python-host."""
import importlib.util
import json
import sys
import types
import unittest
from contextlib import contextmanager
from pathlib import Path
from unittest.mock import patch


@contextmanager
def browser_host():
    resolved = []
    js = types.ModuleType('js')
    def resolve(name):
        resolved.append(name)
        raise AttributeError(name)
    js.__getattr__ = resolve
    ffi = types.ModuleType('pyodide.ffi')
    ffi.can_run_sync = lambda: True
    ffi.run_sync = lambda value: value
    spec = importlib.util.spec_from_file_location(
        '_noon_host_binding_test', Path(__file__).with_name('_noon_host.py'))
    host = importlib.util.module_from_spec(spec)
    with patch.object(sys, 'platform', 'emscripten'), patch.dict(sys.modules, {
        'js': js, 'pyodide': types.ModuleType('pyodide'), 'pyodide.ffi': ffi,
        '_noon_host_binding_test': host,
    }):
        spec.loader.exec_module(host)
        yield host, js, ffi, resolved


class BrowserBindingTests(unittest.TestCase):
    def test_repeated_imports_use_the_original_function_without_host_rediscovery(self):
        with browser_host() as (host, js, ffi, resolved):
            calls = []
            def active(context):
                calls.append(context)
                if context == 'retired':
                    raise RuntimeError('retired')
            def resolve(name):
                resolved.append(name)
                if name == 'noonRequireSemanticContinuationActive':
                    return active
                raise AttributeError(name)
            js.__getattr__ = resolve
            for _ in range(2048):
                from _noon_host_binding_test import noonRequireSemanticContinuationActive
                self.assertIs(noonRequireSemanticContinuationActive, active)
                noonRequireSemanticContinuationActive('live')
            self.assertEqual(resolved.count('noonRequireSemanticContinuationActive'), 1)
            self.assertEqual(len(calls), 2048)
            # A cached function still validates each invocation, not just lookup.
            with self.assertRaisesRegex(RuntimeError, 'retired'):
                noonRequireSemanticContinuationActive('retired')
            self.assertIs(host.wait_sync, ffi.run_sync)
            self.assertIs(host.can_wait_sync, ffi.can_run_sync)

    def test_missing_optional_binding_is_not_cached_or_eagerly_resolved(self):
        with browser_host() as (host, js, ffi, resolved):
            self.assertEqual(resolved, [])
            with self.assertRaisesRegex(AttributeError, 'noonCompileOptionalResource'):
                host.noonCompileOptionalResource
            self.assertNotIn('noonCompileOptionalResource', vars(host))
            function = lambda value: value
            js.noonCompileOptionalResource = function
            self.assertIs(host.noonCompileOptionalResource, function)
            self.assertIs(vars(host)['noonCompileOptionalResource'], function)
            self.assertIs(host.noonCompileOptionalResource, function)
            with self.assertRaises(AttributeError):
                host.__path__
            self.assertNotIn('__path__', resolved)

    def test_browser_codec_preserves_exact_wire_values_and_rejects_nonfinite(self):
        with browser_host() as (host, js, ffi, resolved):
            value = {'token': [3, 7], 'value': 0.125, 'label': 'unicode \u03b1'}
            encoded = host.encode_callback(value)
            self.assertEqual(encoded, json.dumps(value, separators=(',', ':'), allow_nan=False))
            self.assertEqual(host.decode_callback(encoded), value)
            for invalid in (float('nan'), float('inf'), -float('inf')):
                with self.assertRaises(ValueError):
                    host.encode_callback({'value': invalid})
            class HostString:
                def __str__(self):
                    return encoded
            self.assertEqual(host.decode_callback(HostString()), value)
            self.assertEqual(resolved, [])

    def test_native_import_fixture_is_not_retained_after_its_scope(self):
        spec = importlib.util.spec_from_file_location(
            '_noon_host_fixture_test', Path(__file__).with_name('_noon_host.py'))
        host = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(host)
        for _ in range(2):
            js = types.ModuleType('js')
            js.noonRequireSemanticContinuationActive = object()
            with patch.dict(sys.modules, {'js': js}):
                self.assertIs(host.noonRequireSemanticContinuationActive,
                              js.noonRequireSemanticContinuationActive)
                self.assertEqual(host.encode_callback({'a': 1}), '{"a":1}')
        with patch.dict(sys.modules):
            sys.modules.pop('js', None)
            value = {'a': 1}
            self.assertIs(host.encode_callback(value), value)
            self.assertIs(host.decode_callback(value), value)


if __name__ == '__main__':
    unittest.main()
