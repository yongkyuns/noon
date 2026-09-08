from pathlib import Path
p = Path('web/python/_manim_updaters.py')
s = p.read_text()
needle = '        self._signals: dict[tuple[int, int], float] = {}'
assert needle in s
s = s.replace(needle, needle + '\n        self._prefetch_errors: dict[tuple[int, int], Exception] = {}', 1)
needle = '    def _object_item(self, key: tuple[int, int]) -> dict[str, Any]:'
assert needle in s
s = s.replace(needle, '''    async def _read_scalar_async(self, key: tuple[int, int]) -> float:
        """Read the same Rust-pinned phase without suspending a Python stack."""
        from js import noonReadSemanticContinuationCallback

        request_id = self._next_read_request_id
        self._next_read_request_id += 1
        request = {"request_id": request_id, "kind": "scalar_signal", "node": _phase_node_json(key)}
        raw = await noonReadSemanticContinuationCallback(
            self._authoring_context,
            json.dumps(self.token, separators=(",", ":")),
            json.dumps(request, separators=(",", ":")),
        )
        result = json.loads(str(raw))
        if not isinstance(result, dict) or result.get("kind") != "scalar":
            raise RuntimeError("canonical callback scalar prefetch returned the wrong typed value")
        return _phase_number("scalar callback read", result.get("value"))

    async def prefetch_captured_scalars(self, callbacks, tracker_type) -> None:
        """Resolve direct Python captures, never invoke callbacks or user getters.

        These are optional phase-local read hints, not authored signal values or
        a callback dependency graph. Rust validates every read against the pinned
        token. Unused/invalid speculative reads must not change callback behavior;
        defer their errors until an actual scalar read. Dynamic misses keep the
        existing suspended-read contract instead of replaying callback effects.
        """
        from types import FunctionType, MethodType

        seen_functions = set()
        for callback in callbacks:
            function = callback.__func__ if isinstance(callback, MethodType) else callback
            if not isinstance(function, FunctionType) or id(function) in seen_functions:
                continue
            seen_functions.add(id(function))
            values = list(function.__defaults__ or ())
            values.extend((function.__kwdefaults__ or {}).values())
            for cell in function.__closure__ or ():
                try:
                    values.append(cell.cell_contents)
                except ValueError:
                    pass
            values.extend(function.__globals__[name] for name in function.__code__.co_names
                          if name in function.__globals__)
            for value in values:
                if type(value) is not tracker_type:
                    continue
                if inspect.getattr_static(value, "_canonical_context", None) is not self._authoring_context:
                    continue
                handle = inspect.getattr_static(value, "_canonical_handle", None)
                if handle is None or isinstance(handle, property):
                    continue
                key = (int(handle.semanticSlot), int(handle.semanticGeneration))
                if key in self._signals or key in self._prefetch_errors:
                    continue
                try:
                    self._signals[key] = await self._read_scalar_async(key)
                except Exception as error:
                    self._prefetch_errors[key] = error

''' + needle, 1)
needle = '''        cached = self._signals.get(key)
        if cached is not None:
            return cached
        result = self._read("scalar_signal", key)'''
assert needle in s
s = s.replace(needle, '''        cached = self._signals.get(key)
        if cached is not None:
            return cached
        if key in self._prefetch_errors:
            raise RuntimeError(f"canonical callback sparse read failed: {self._prefetch_errors[key]}") from None
        result = self._read("scalar_signal", key)''', 1)
needle = 'def run_canonical_callback_phase(session_id: int, frame: dict[str, Any]) -> str:'
assert needle in s
s = s.replace(needle, '''async def prepare_canonical_callback_phase(session_id: int, frame: dict[str, Any]):
    """Prepare bounded capture reads before executing this callback phase once."""
    session = _CANONICAL_SESSIONS[int(session_id)]
    context = _CanonicalCallbackContext(frame, session.context)
    from _manim_reactive import ValueTracker

    callbacks = [session.callbacks[int(item["callback_id"])] for item in frame.get("invocations", [])]
    await context.prefetch_captured_scalars(callbacks, ValueTracker)
    return context


def run_canonical_callback_phase(
    session_id: int, frame: dict[str, Any], *, prepared_context=None
) -> str:''', 1)
needle = '''    context = _CanonicalCallbackContext(frame, session.context)
    scene_key = id(session.scene)'''
assert needle in s
s = s.replace(needle, '''    context = prepared_context or _CanonicalCallbackContext(frame, session.context)
    if context.token != frame["token"] or context._authoring_context is not session.context:
        raise RuntimeError("prepared canonical callback reads belong to a different phase")
    scene_key = id(session.scene)''', 1)
p.write_text(s)
p = Path('web/python/_manim_canonical_scene.py')
s = p.read_text()
s = s.replace('''    scene: _base.Scene, event_json: object
) -> object | None:''', '''    scene: _base.Scene, event_json: object, *, prepared_callback=None
) -> object | None:''', 1)
s = s.replace('''        batch_json = _manim_updaters.run_canonical_callback_phase(session_id, phase)''', '''        batch_json = _manim_updaters.run_canonical_callback_phase(
            session_id, phase, prepared_context=prepared_callback
        )''', 1)
old = '''    event_json = await noonAwaitSemanticContinuation(_context(scene))
    while True:
        next_event = _service_semantic_continuation_event(scene, event_json)
        if next_event is None:
            return
        event_json = await next_event'''
assert old in s
s = s.replace(old, '''    event_json = await noonAwaitSemanticContinuation(_context(scene))
    while True:
        prepared = None
        event = _continuation_event(event_json)
        if event["kind"] == "callback":
            import _manim_updaters
            from js import noonFailSemanticContinuationCallback

            try:
                prepared = await _manim_updaters.prepare_canonical_callback_phase(
                    _manim_updaters.canonical_callback_session_id(scene), event["phase"]
                )
            except Exception as error:
                event_json = await noonFailSemanticContinuationCallback(
                    _context(scene), _json(event["phase"]["token"]), str(error)
                )
                continue
        next_event = _service_semantic_continuation_event(
            scene, event_json, prepared_callback=prepared
        )
        if next_event is None:
            return
        event_json = await next_event''', 1)
p.write_text(s)
p = Path('web/python/test_updater_snapshot.py')
p.write_text(p.read_text() + '\n\n' + (Path(__file__).parent / 'scalar-prefetch-tests.txt').read_text())
p = Path('docs/mobile-web-rendering.md')
p.write_text(p.read_text() + '''

For async continuation callbacks, direct captured/default/global ValueTracker
wrappers are optional sparse-read hints. Their values are fetched asynchronously
from the same Rust-pinned callback phase before invoking the unchanged callback
once. The cache expires with that phase; no authored tracker value, callback
replay, whole-scene snapshot or frontend dependency graph substitutes for Rust.
Failed speculative reads are deferred until actual use, so an unused invalid
capture does not introduce a callback failure. Dynamic indirect read misses still
require the existing suspended-read support. Cost is proportional to active
callback metadata plus unique captured scalar read hints; this does not claim
arbitrary callbacks have a complete statically discoverable read set.
''')
