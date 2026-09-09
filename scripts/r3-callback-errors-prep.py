# Development-only assembly; never included in the product commit.
from pathlib import Path
root = Path.cwd()

def replace_once(text, old, new):
    assert text.count(old) == 1, (old[:120], text.count(old))
    return text.replace(old, new, 1)

p = root / 'crates/noon-web/src/authoring_error.rs'
s = p.read_text()
s = replace_once(s, '    ExecutionSessionPublicationError, LiveSessionError,', '    ExecutionSessionCallbackError, ExecutionSessionCallbackReadError,\n    ExecutionSessionPublicationError, LiveSessionError,')
projection = '''// Callback transactions already have typed shared errors. Preserve their domain
// causes here; decoder/player-local String failures remain explicitly unclassified.
impl From<ExecutionSessionCallbackReadError> for AuthoringFailure {
    fn from(error: ExecutionSessionCallbackReadError) -> Self {
        use ExecutionSessionCallbackReadError as E;
        let message = error.to_string();
        match error {
            E::NoPendingPhase => Self::new(
                "stale_publication",
                "callback_read.no_pending_phase",
                message,
            ),
            E::StaleToken { expected, actual } => Self::new(
                "stale_publication",
                "callback_read.stale_token",
                format!("{message}; expected {expected:?}, actual {actual:?}"),
            ),
            E::UnknownSignal(_) => {
                Self::new("stale_handle", "callback_read.unknown_signal", message)
            }
            E::NonScalarSignal(_) => {
                Self::new("invalid_input", "callback_read.non_scalar_signal", message)
            }
            E::UnknownObject(_) => {
                Self::new("stale_handle", "callback_read.unknown_object", message)
            }
        }
    }
}

impl From<ExecutionSessionCallbackError> for AuthoringFailure {
    fn from(error: ExecutionSessionCallbackError) -> Self {
        use ExecutionSessionCallbackError as E;
        let message = error.to_string();
        match error {
            E::NoPendingPhase => {
                Self::new("stale_publication", "callback.no_pending_phase", message)
            }
            E::StaleToken { expected, actual } => Self::new(
                "stale_publication",
                "callback.stale_token",
                format!("{message}; expected {expected:?}, actual {actual:?}"),
            ),
            E::UnknownObject(_) => {
                Self::new("stale_handle", "callback.unknown_object", message)
            }
            E::Read(cause) => Self::caused_by("callback.read", message, cause.into()),
            E::Evaluation(cause) => Self::caused_by(
                "callback.evaluation",
                message,
                Self::unclassified("runtime.evaluation", &cause),
            ),
            E::InvalidEffectiveWrite(cause) => Self::caused_by(
                "callback.invalid_effective_write",
                message,
                Self::unclassified("runtime.effective_write", &cause),
            ),
            E::Commit(cause) => Self::caused_by(
                "callback.commit",
                message,
                Self::unclassified("runtime.frame_commit", &cause),
            ),
            // Advance/driver policy is outside this transaction-boundary slice.
            other => Self::unclassified("callback.unclassified", &other),
        }
    }
}

'''
s = replace_once(s, 'impl From<ExecutionSessionPublicationError> for AuthoringFailure {', projection + 'impl From<ExecutionSessionPublicationError> for AuthoringFailure {')
p.write_text(s)
p = root / 'crates/noon-web/src/semantic_execution_player.rs'
s = p.read_text()
s = replace_once(s, '#[cfg(any(target_arch = "wasm32", test))]\nuse crate::authoring_error::AuthoringFailure;', 'use crate::authoring_error::AuthoringFailure;')
methods = [
    ('required_callback_read_json', 'requiredCallbackReadJson', '&mut self,\n        token_json: &str,\n        request_json: &str,', 'String', 'token_json, request_json'),
    ('commit_callback_phase_json', 'commitCallbackPhaseJson', '&mut self, batch_json: &str', '()', 'batch_json'),
    ('fail_callback_phase_json', 'failCallbackPhaseJson', '&mut self, phase_json: &str', '()', 'phase_json'),
    ('interrupt_callback_phase_json', 'interruptCallbackPhaseJson', '&mut self, phase_json: &str', '()', 'phase_json'),
]
moved = []
for name, js, args, ret, callargs in methods:
    attr = f'    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = {js}))]'
    assert s.count(attr) == 1
    start = s.index(attr)
    fnstart = s.index('    pub fn ', start)
    brace = s.index('{', fnstart)
    depth, end = 1, brace + 1
    while depth:
        if s[end] == '{': depth += 1
        elif s[end] == '}': depth -= 1
        end += 1
    original = s[fnstart:end]
    native = replace_once(original, f'Result<{ret}, String>', f'Result<{ret}, AuthoringFailure>')
    if name == 'required_callback_read_json':
        native = replace_once(native, '.required_callback_read(token, request_wire.into())\n            .map_err(|error| error.to_string())?', '.required_callback_read(token, request_wire.into())\n            .map_err(AuthoringFailure::from)?')
        native = replace_once(native, 'serde_json::to_string(&wire).map_err(|error| error.to_string())', 'serde_json::to_string(&wire).map_err(|error| AuthoringFailure::from(error.to_string()))')
    else:
        target = {'commit_callback_phase_json': '.commit_required_callback_phase(batch)', 'fail_callback_phase_json': '.fail_required_callback_phase(token)', 'interrupt_callback_phase_json': '.interrupt_required_callback_phase(token)'}[name]
        native = replace_once(native, target + '\n            .map_err(|error| error.to_string())?', target + '\n            .map_err(AuthoringFailure::from)?')
    moved.append(native)
    if '\n' in args:
        signature = f'    pub fn {name}_wasm(\n        {args}\n    ) -> Result<{ret}, wasm_bindgen::JsValue> {{'
    else:
        signature = f'    pub fn {name}_wasm(\n        {args},\n    ) -> Result<{ret}, wasm_bindgen::JsValue> {{'
    replacement = '    #[cfg(target_arch = "wasm32")]\n' + attr + '\n' + signature + f'\n        self.{name}({callargs})\n            .map_err(crate::authoring_error::js_error)\n    }}'
    s = s[:start] + replacement + s[end:]
insert = '#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]\nimpl SemanticExecutionPlayer {'
new = '''// Keep the shared callback failures typed until the actual JS boundary. The
// decoding and preflight/commit order below are the existing worker protocol.
impl SemanticExecutionPlayer {
''' + '\n\n'.join(moved) + '\n}\n\n'
s = replace_once(s, insert, new + insert)
s += '\n#[cfg(test)]\nmod callback_error_tests;\n'
p.write_text(s)
p = root / 'web/python/_manim_updaters.py'
s = p.read_text()
s = replace_once(s, 'import noon as _base\n', 'import noon as _base\nfrom _noon_errors import engine_await, raise_engine_error\n')
s = replace_once(s, 'raise RuntimeError(f"canonical callback sparse read failed: {error}") from None', 'raise_engine_error(error, operation="callback.read")')
s = replace_once(s, '''        raw = await noonReadSemanticContinuationCallback(
            self._authoring_context,
            json.dumps(self.token, separators=(",", ":")),
            json.dumps(request, separators=(",", ":")),
        )''', '''        raw = await engine_await(noonReadSemanticContinuationCallback(
            self._authoring_context,
            json.dumps(self.token, separators=(",", ":")),
            json.dumps(request, separators=(",", ":")),
        ), operation="callback.read")''')
s = replace_once(s, 'raise RuntimeError(f"canonical callback sparse read failed: {self._prefetch_errors[key]}") from None', 'raise_engine_error(self._prefetch_errors[key], operation="callback.read")')
p.write_text(s)
p = root / 'scripts/typed-authoring-errors-smoke.mjs'
s = p.read_text()
s = replace_once(s, 'const tests = await readFile(path.join(root, "web/python/test_noon_errors_wasm.py"), "utf8");', 'const tests = await readFile(path.join(root, "web/python/test_noon_errors_wasm.py"), "utf8");\nconst callbackTests = await readFile(path.join(root, "web/python/test_noon_callback_errors_wasm.py"), "utf8");')
s = replace_once(s, 'async ({modules, tests, pyodideUrl})', 'async ({modules, tests, callbackTests, pyodideUrl})')
s = replace_once(s, '    pyodide.FS.writeFile("/tmp/test_noon_errors_wasm.py", tests);', '    pyodide.FS.writeFile("/tmp/test_noon_errors_wasm.py", tests);\n    pyodide.FS.writeFile("/tmp/test_noon_callback_errors_wasm.py", callbackTests);')
s = replace_once(s, '      completeAsPromise: async context => context.liveCompleteSegment(),', '''      completeAsPromise: async context => context.liveCompleteSegment(),
      installCallbackReader: player => {
        const previous = globalThis.noonReadSemanticContinuationCallback;
        globalThis.noonReadSemanticContinuationCallback = async (_context, token, request) =>
          player.requiredCallbackReadJson(token, request);
        return () => {
          if (previous === undefined) delete globalThis.noonReadSemanticContinuationCallback;
          else globalThis.noonReadSemanticContinuationCallback = previous;
        };
      },''')
s = replace_once(s, 'await tests.check_real_promise_rejection()', '''import test_noon_callback_errors_wasm as callback_tests
callback_result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromModule(callback_tests))
assert not callback_result.skipped, callback_result.skipped
assert callback_result.wasSuccessful(), "real callback transaction boundary tests failed"
await callback_tests.check_sparse_callback_read_callsite()
await tests.check_real_promise_rejection()''')
s = replace_once(s, '"additionalTests": result.testsRun, "promiseRejectionAndRecovery": True', '"additionalTests": result.testsRun, "callbackTests": callback_result.testsRun, "sparseCallbackRead": True, "promiseRejectionAndRecovery": True')
s = replace_once(s, '  }, {modules, tests, pyodideUrl});', '  }, {modules, tests, callbackTests, pyodideUrl});')
s = replace_once(s, '  assert.equal(report.python.additionalTests, 8);', '  assert.equal(report.python.additionalTests, 8);\n  assert.equal(report.python.callbackTests, 6);\n  assert.equal(report.python.sparseCallbackRead, true);')
p.write_text(s)
