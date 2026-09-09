"""Independent first-wait publication fix, not composition or error-enum work."""
from pathlib import Path
import ast
import sys

TOOLING = Path(__file__).resolve().parent
p = Path('crates/noon-web/src/canonical_authoring_scene.rs')
s = p.read_text()
marker = '#[cfg(test)]\nmod ownership_tests;'
assert s.count(marker) == 1
if 'mod wait_bootstrap_tests;' not in s:
    s = s.replace(marker, marker + '\n#[cfg(test)]\nmod wait_bootstrap_tests;', 1)
assert s.count('mod wait_bootstrap_tests;') == 1
p.write_text(s)
Path('crates/noon-web/src/canonical_authoring_scene/wait_bootstrap_tests.rs').write_text(
    (TOOLING / 'wait_bootstrap_tests.rs').read_text())
if len(sys.argv) > 1 and sys.argv[1] == 'tests-only':
    raise SystemExit(0)
old = '''            self.live_player(duration.max(1.0))?;
        }
        let player = self.active_live_player()?;
        player.live_wait(duration)
'''
new = '''            let mut player = self.build_live_player(duration.max(1.0), 0)?;
            let end_time = player.live_wait(duration)?;
            // Shared admission is fallible. Publish the first runtime only once
            // it owns a valid segment; rejection leaves this context unstarted.
            self.player_ownership = PlayerOwnership::Active(player);
            return Ok(end_time);
        }
        let player = self.active_live_player()?;
        player.live_wait(duration)
'''
assert s.count(old) == 1
p.write_text(s.replace(old, new, 1))
p = Path('web/python/test_noon_errors_wasm.py')
s = p.read_text()
assert s.count('    NoonForeignHandleError, NoonOwnershipError') == 1
s = s.replace('    NoonForeignHandleError, NoonOwnershipError',
              '    NoonError, NoonForeignHandleError, NoonOwnershipError', 1)
marker = '\n\nasync def check_real_promise_rejection():'
assert s.count(marker) == 1
s = s.replace(marker, '\n' + (TOOLING / 'wait_python_test.txt').read_text() + marker, 1)
module = ast.parse(s)
cls = next(n for n in module.body if isinstance(n, ast.ClassDef) and n.name == 'WasmErrorProjectionTests')
tests = [n.name for n in cls.body if isinstance(n, ast.FunctionDef) and n.name.startswith('test_')]
assert len(tests) == 9 and 'test_invalid_first_wait_does_not_publish_a_player_and_retries' in tests
p.write_text(s)
# The same runner keeps an exact discovered-case assertion; add the new case.
p = Path('scripts/typed-authoring-errors-smoke.mjs')
s = p.read_text()
old = 'assert.equal(report.python.additionalTests, 8);'
assert s.count(old) == 1
p.write_text(s.replace(old, 'assert.equal(report.python.additionalTests, 9);', 1))
