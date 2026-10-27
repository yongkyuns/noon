#!/usr/bin/env python3
"""Source-bound combined qualification; no remote writes or feature promotion."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile

FEATURE = 'b3a4b7f484e607a4b14bc57e70f4947f8520ca42'
PREVIOUS_MASTER = 'f71a4c121bc29921133f790ae9783e89e962e7d9'
LATEST_MASTER = '0351da54754cd2d5f95a7624aacd10872a5b8499'
MEMORY = '472a61fadfdabba44756b75caaa184cee8c4be57'
COMMON = 'ac9bd59bbcfa4d74573734cede83e477862396a2'
SOURCE_TREE = '9d49d95c88f709dc420591ccdd531fe0a703eead'
SOURCE_SHA256 = '0931e147b9e94a93d79c5a766c26228a1c1b5e4be1ef702ebe0bc76ff55f8805'
MEMORY_TREE = '1035d88b24839b3cc434ee57cc5f5fb98d4e13ed'
LIB = 'crates/noon-runtime/src/lib.rs'
PREFIX = 'crates/noon-runtime/src/replay/prefix.rs'
SEEK_TEST = 'crates/noon-runtime/src/seek_storage_tests.rs'
INSET_TEST = 'crates/noon-runtime/src/replay/prefix/inset_tests.rs'
NEW_TEST = 'seek_storage_tests::merged_seek_preserves_paint_effect_and_reserved_storage'
MEMORY_FILES = {
    'crates/noon-compile/src/replay_revision/prefix/storage.rs', LIB, PREFIX,
    'crates/noon-runtime/src/replay/prefix/history_index_budget_tests.rs',
    'crates/noon-runtime/src/replay/prefix/history_payload_budget.rs',
    'crates/noon-runtime/src/replay/prefix/history_payload_budget_tests.rs',
    'crates/noon-runtime/src/replay/prefix/object_budget.rs',
    'crates/noon-runtime/src/replay/prefix/object_budget_tests.rs',
    'crates/noon-runtime/src/replay/prefix/storage.rs', SEEK_TEST,
    'crates/noon-web/src/prepared_execution_player/source.rs',
    'crates/noon-web/src/prepared_execution_player/source/tests.rs',
    'web/python-worker-prepared-budget.test.mjs',
}
CONFLICT = re.compile(r'(?ms)^<<<<<<< [^\n]+\n(.*?)^=======\n(.*?)^>>>>>>> [^\n]+\n')

def digest(data):
    return hashlib.sha256(data).hexdigest()

def git(repo, *args, check=True, data=None):
    result = subprocess.run(['git', *args], cwd=repo, input=data,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check and result.returncode:
        raise RuntimeError(f'git {args}: {result.stderr.decode(errors="replace")[-2000:]}')
    return result

def text(repo, *args):
    return git(repo, *args).stdout.decode().strip()

def tests(source):
    return set(re.findall(r'#\[test\]\s*(?:#\[[^\]]+\]\s*)*fn\s+(\w+)', source))

def resolve(path, ours, theirs):
    if path == LIB:
        assert 'let appearances = initial_scalar_property(' in ours
        assert 'FrameObjectState {' in ours
        assert theirs.strip() == 'let mut frame = FrameState {'
        # Keep the incoming in-place initializer, then add modern row fields below.
        return theirs
    if path == PREFIX and 'mod inset_tests;' in ours:
        assert ours.strip() == 'mod inset_tests;'
        assert all(f'mod {name};' in theirs for name in
                   ('history_index_budget_tests', 'history_payload_budget_tests', 'object_budget_tests'))
        return ours + '#[cfg(test)]\n' + theirs
    if path == PREFIX and 'changes_inset_2d_view()' in ours:
        assert 'UnsupportedDomain' in ours
        assert 'preflight_history_index_capacity(' in theirs
        assert 'object_budget::preflight_clone(' in theirs
        # Domain admission must precede component-budget and clone admission.
        return ours + theirs
    raise AssertionError(f'Unexpected conflict at {path}: {ours[:150]!r} / {theirs[:150]!r}')

def modernize_row(source):
    old = 'glow: object.glow.clone(),'
    assert source.count(old) == 1
    assert 'fill_paint: object.fill_paint.clone(),' not in source
    return source.replace(old, 'fill_paint: object.fill_paint.clone(),\n'
                               '                effect: object.effect.clone(),')

def stage(repo, root):
    evidence = root / 'noon2308-combined-evidence'
    evidence.mkdir(parents=True, exist_ok=True)
    archive_dir = root / 'noon2308-green-input'
    archive = archive_dir / 'current-master-candidate-source.tar.gz'
    assert digest(archive.read_bytes()) == SOURCE_SHA256
    assert (archive_dir / 'final-tree.txt').read_text().strip() == SOURCE_TREE
    assert (archive_dir / 'status.txt').read_text() == ''
    assert all((archive_dir / (name + '.exit')).read_text().strip() == '0'
               for name in ('fast', 'prepared', 'wasm'))
    provenance = json.loads((archive_dir / 'manifest.json').read_text())
    assert provenance['pinned_current_master'] == PREVIOUS_MASTER
    pins = [FEATURE, PREVIOUS_MASTER, LATEST_MASTER, MEMORY]
    fetched = git(repo, 'fetch', '--no-tags', '--depth=256', 'origin', *pins)
    (evidence / 'fetch.log').write_bytes(fetched.stdout + fetched.stderr)
    assert all(text(repo, 'rev-parse', sha + '^{commit}') == sha for sha in pins)
    assert text(repo, 'rev-parse', MEMORY + '^{tree}') == MEMORY_TREE
    assert text(repo, 'merge-base', FEATURE, MEMORY) == COMMON
    assert set(text(repo, 'diff', '--name-only', COMMON, MEMORY).splitlines()) == MEMORY_FILES
    work = root / 'noon2308-combined'
    git(repo, 'worktree', 'add', '--detach', str(work), FEATURE)
    git(work, 'config', 'user.name', 'Noon isolated combined qualification')
    git(work, 'config', 'user.email', 'qualification@example.invalid')
    for entry in work.iterdir():
        if entry.name == '.git':
            continue
        if entry.is_dir() and not entry.is_symlink():
            shutil.rmtree(entry)
        else:
            entry.unlink()
    with tarfile.open(archive, 'r:gz') as tar:
        assert all(not (m.name.startswith('/') or '..' in Path(m.name).parts
                        or m.issym() or m.islnk()) for m in tar.getmembers())
        tar.extractall(work, filter='data')
    git(work, 'add', '--all')
    assert text(work, 'write-tree') == SOURCE_TREE
    synthetic = git(work, 'commit-tree', SOURCE_TREE, '-p', FEATURE, '-p', PREVIOUS_MASTER,
                    data=b'Authenticated green integration source; local only\n').stdout.decode().strip()
    git(work, 'checkout', '--detach', '--force', synthetic)
    latest = git(work, 'merge', '--no-commit', '--no-ff', LATEST_MASTER, check=False)
    (evidence / 'latest-master-merge.log').write_bytes(latest.stdout + latest.stderr)
    assert latest.returncode == 0, latest.stderr.decode()
    assert not text(work, 'ls-files', '-u')
    git(work, 'commit', '-m', 'Update source-qualified candidate to pinned latest master; local only')
    latest_candidate = text(work, 'rev-parse', 'HEAD')
    assert text(work, 'show', '-s', '--format=%P', 'HEAD').split() == [synthetic, LATEST_MASTER]
    before = {p: (work / p).read_text() for p in MEMORY_FILES | {INSET_TEST} if (work / p).exists()}
    merged = git(work, 'merge', '--no-commit', '--no-ff', MEMORY, check=False)
    (evidence / 'memory-merge.log').write_bytes(merged.stdout + merged.stderr)
    conflicts = text(work, 'diff', '--name-only', '--diff-filter=U').splitlines()
    (evidence / 'memory-conflicts.diff').write_bytes(git(work, 'diff', '--cc').stdout)
    (evidence / 'memory-stages.txt').write_text(text(work, 'ls-files', '-u') + '\n')
    assert merged.returncode == 1 and set(conflicts) == {LIB, PREFIX}, conflicts
    resolutions = []
    for path, expected in ((LIB, 1), (PREFIX, 2)):
        raw = (work / path).read_text()
        assert len(list(CONFLICT.finditer(raw))) == expected, path
        def replace(match):
            ours, theirs = match.group(1), match.group(2)
            result = resolve(path, ours, theirs)
            resolutions.append({'file': path, 'ours': digest(ours.encode()),
                                'theirs': digest(theirs.encode()), 'result': digest(result.encode())})
            return result
        (work / path).write_text(CONFLICT.sub(replace, raw))
    lib = (work / LIB).read_text()
    start, end = lib.index('fn reset_base_frame('), lib.index('\nconst PROPERTY_ORDER:')
    lib = lib[:start] + modernize_row(lib[start:end]) + lib[end:]
    assert 'SemanticPaint::Hatch' in lib
    assert 'reset_base_frame(&self.compiled, &mut self.frame, time);' in lib
    assert 'self.frame = base_frame(' not in lib
    (work / LIB).write_text(lib)
    seek = (work / SEEK_TEST).read_text()
    assert text(work, 'hash-object', SEEK_TEST) == 'd2e536e74465b6107cdb234577ca390b9c9e185f'
    seek = modernize_row(seek)
    additions = ((SEEK_TEST, 'one-off-2308-combined-seek.rs'),
                 (INSET_TEST, 'one-off-2308-combined-inset.rs'))
    (work / SEEK_TEST).write_text(seek)
    for path, helper in additions:
        snippet = (repo / '.github/ci' / helper).read_text()
        old = (work / path).read_text()
        assert tests(snippet) and not (tests(old) & tests(snippet))
        (work / path).write_text(old + snippet)
    # The compiler's historical inset snapshot is now one optional boxed field.
    # Memory-stack storage-accounting tests construct a deliberately empty revision;
    # retain that fixture and all assertions rather than weakening Fast Gate.
    fixture_path = 'crates/noon-compile/src/replay_revision/prefix/storage.rs'
    fixture = work / fixture_path
    fixture_original = fixture.read_text()
    assert digest(fixture_original.encode()) == '0751278e9a68ce4ff21efd3a0cca5e5e398b059e41652aaa06d4ece28aaf922d'
    compiled_revision = (work / 'crates/noon-compile/src/replay_revision.rs').read_text()
    assert compiled_revision.count('inset_2d_view: Option<Box<SavedInset2DView>>') == 1
    missing_field = ('        let mut revision = CompiledReplayRevision {\n'
                     '            rows: Vec::with_capacity(3),\n')
    filled_field = missing_field + '            inset_2d_view: None,\n'
    assert fixture_original.count(missing_field) == 1
    assert 'inset_2d_view: None,' not in fixture_original
    fixture_updated = fixture_original.replace(missing_field, filled_field, 1)
    assert tests(fixture_updated) == tests(fixture_original)
    assert fixture_updated.count('inset_2d_view: None,') == 1
    fixture.write_text(fixture_updated)
    fixture_repair = {
        'file': fixture_path,
        'reason': 'Compile existing saved-revision vector budget test with current inset metadata',
        'original_sha256': digest(fixture_original.encode()),
        'corrected_sha256': digest(fixture_updated.encode()),
        'assertions_and_test_names_unchanged': True
    }
    prefix = (work / PREFIX).read_text()
    for module in ('inset_tests', 'history_index_budget_tests', 'history_payload_budget_tests', 'object_budget_tests'):
        assert prefix.count('#[cfg(test)]\nmod ' + module + ';') == 1
    assert prefix.index('changes_inset_2d_view()') < prefix.index('storage::preflight_history_index_capacity(')
    inventory = {}
    for path in MEMORY_FILES | {INSET_TEST}:
        now = (work / path).read_text()
        previous = before.get(path, '')
        assert tests(previous) <= tests(now), ('lost current tests', path, tests(previous) - tests(now))
        # Only memory tests newly introduced since the common ancestor are required;
        # this does not resurrect obsolete tests already replaced on master.
        base = git(work, 'show', COMMON + ':' + path, check=False)
        incoming = git(work, 'show', MEMORY + ':' + path, check=False)
        added_tests = tests(incoming.stdout.decode()) - tests(base.stdout.decode())
        assert added_tests <= tests(now), ('lost memory tests', path, added_tests - tests(now))
        assert now.count('#[ignore') <= max(previous.count('#[ignore'), incoming.stdout.decode().count('#[ignore'))
        inventory[path] = {'retained_current_tests': len(tests(previous)),
                           'incoming_added_tests': len(added_tests), 'combined_tests': len(tests(now))}
    git(work, 'add', '--all')
    assert not text(work, 'ls-files', '-u')
    git(work, 'diff', '--cached', '--check')
    git(work, 'commit', '-m', 'Combine latest paint/effect history and bounded seek storage; local only')
    combined = text(work, 'rev-parse', 'HEAD')
    assert text(work, 'show', '-s', '--format=%P', 'HEAD').split() == [latest_candidate, MEMORY]
    for sha in (FEATURE, LATEST_MASTER, MEMORY):
        git(work, 'merge-base', '--is-ancestor', sha, combined)
    record = {'scope': 'Isolated combined candidate; no remote feature/master writes',
              'inputs': {'feature': FEATURE, 'previous_master': PREVIOUS_MASTER,
                         'latest_master': LATEST_MASTER, 'memory': MEMORY},
              'source_tree': SOURCE_TREE, 'source_archive_sha256': SOURCE_SHA256,
              'source_workflow_run': 40563358160, 'latest_candidate_local': latest_candidate,
              'combined_local': combined, 'unformatted_tree': text(work, 'rev-parse', 'HEAD^{tree}'),
              'resolutions': resolutions, 'test_inventory': inventory,
              'test_fixture_repair': fixture_repair}
    (evidence / 'manifest.json').write_text(json.dumps(record, indent=2) + '\n')
    (evidence / 'combined-vs-latest.patch').write_bytes(git(work, 'diff', '--binary', LATEST_MASTER, combined).stdout)
    (evidence / 'combined-vs-green.patch').write_bytes(git(work, 'diff', '--binary', synthetic, combined).stdout)
    print(json.dumps(record, indent=2), flush=True)

def controls(root):
    work, out = root / 'noon2308-combined', root / 'noon2308-combined-evidence'
    path = work / LIB
    original = path.read_bytes()
    outcomes = {}
    def run(label):
        args = ['cargo', 'test', '--locked', '-p', 'noon-runtime', '--features', 'prepared-prefix',
                '--lib', NEW_TEST, '--', '--exact', '--test-threads=1']
        result = subprocess.run(args, cwd=work, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / (label + '.log')).write_bytes(result.stdout)
        log = result.stdout.decode(errors='replace')
        print(log, flush=True)
        outcomes[label] = result.returncode
        return result.returncode, log
    try:
        code, log = run('combined-positive')
        assert code == 0 and 'test result: ok. 1 passed; 0 failed' in log
        start = original.index(b'fn reset_base_frame(')
        end = original.index(b'\nconst PROPERTY_ORDER:')
        block = original[start:end]
        for name, field in (('paint', 'fill_paint'), ('effect', 'effect')):
            needle = (field + ': object.' + field + '.clone(),').encode()
            assert block.count(needle) == 1
            path.write_bytes(original[:start] + block.replace(needle, (field + ': None,').encode()) + original[end:])
            try:
                code, log = run('combined-negative-' + name)
                assert code != 0 and 'error[E' not in log
                assert 'test ' + NEW_TEST + ' ... FAILED' in log
                assert 'seek lost the retained ' + name + ' handle' in log
            finally:
                path.write_bytes(original)
        code, log = run('combined-restored')
        assert code == 0 and 'test result: ok. 1 passed; 0 failed' in log
    finally:
        path.write_bytes(original)
        (out / 'mutation-outcomes.json').write_text(json.dumps(outcomes, indent=2) + '\n')
    git(work, 'diff', '--exit-code')

if __name__ == '__main__':
    root = Path(os.environ['RUNNER_TEMP'])
    if len(sys.argv) == 2 and sys.argv[1] == 'controls':
        controls(root)
    else:
        stage(Path.cwd(), root)
