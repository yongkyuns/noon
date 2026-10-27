#!/usr/bin/env python3
"""Read-only, source-authenticated Git integration conflict inventory.

No remote writes. Conflicts are expected data, not resolved by this runner.
"""
import hashlib
import json
import os
import shutil
import subprocess
import tarfile
from pathlib import Path

repo = Path.cwd()
root = Path(os.environ['RUNNER_TEMP'])
out = root / 'noon2308-current-inventory'
out.mkdir(parents=True, exist_ok=True)
source = root / 'noon2308-evidence' / 'candidate-source.tar.gz'
manifest = root / 'noon2308-evidence' / 'manifest.json'
patch = repo / '.github/ci/one-off-2308-clippy.patch'
refs = {name: os.environ[key] for name, key in (
    ('feature', 'PIN_PR2308'), ('old_master', 'PIN_MASTER_OLD'),
    ('current_master', 'PIN_MASTER_CURRENT'), ('memory', 'PIN_MEMORY'))}

def git(*args, cwd=repo, check=True):
    return subprocess.run(['git', *args], cwd=cwd, check=check,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)

def get(*args, cwd=repo):
    return git(*args, cwd=cwd).stdout.decode().strip()

assert hashlib.sha256(source.read_bytes()).hexdigest() == os.environ['ARCHIVE_SHA256']
assert hashlib.sha256(patch.read_bytes()).hexdigest() == os.environ['CLIPPY_PATCH_SHA256']
proof = json.loads(manifest.read_text())
assert proof['tested_candidate']['tree'] == os.environ['CANDIDATE_BASE_TREE']
assert proof['tested_candidate']['parents'] == [refs['feature'], refs['old_master']]
assert proof['tracked_source_unchanged']
assert proof['command_exit_codes']['gradient'] == 0
assert proof['command_exit_codes']['interactions-tests'] == 0

fetch = git('fetch', '--no-tags', '--depth=256', 'origin', *refs.values())
(out / 'fetch.log').write_bytes(fetch.stdout + fetch.stderr)
for name, sha in refs.items():
    assert get('rev-parse', sha + '^{commit}') == sha, name

work = root / 'noon2308-inventory-candidate'
git('worktree', 'add', '--detach', str(work), refs['feature'])
git('config', 'user.name', 'Noon read-only inventory', cwd=work)
git('config', 'user.email', 'inventory@example.invalid', cwd=work)
for entry in work.iterdir():
    if entry.name == '.git':
        continue
    if entry.is_dir() and not entry.is_symlink():
        shutil.rmtree(entry)
    else:
        entry.unlink()
with tarfile.open(source, mode='r:gz') as archive:
    members = archive.getmembers()
    assert all(not (m.name.startswith('/') or '..' in Path(m.name).parts
                    or m.issym() or m.islnk()) for m in members)
    archive.extractall(work, filter='data')
git('add', '--all', cwd=work)
assert get('write-tree', cwd=work) == os.environ['CANDIDATE_BASE_TREE']
git('apply', '--check', '--index', str(patch), cwd=work)
git('apply', '--index', str(patch), cwd=work)
assert get('write-tree', cwd=work) == os.environ['CANDIDATE_CLIPPY_TREE']
# Creates a *local-only* synthetic commit. The feature/master/memory refs are untouched.
commit_result = subprocess.run(
    ['git', 'commit-tree', os.environ['CANDIDATE_CLIPPY_TREE'],
     '-p', refs['feature'], '-p', refs['old_master']],
    input=b'Source-bound read-only integration inventory\n',
    cwd=work, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
candidate = commit_result.stdout.decode().strip()
assert get('rev-parse', candidate + '^{tree}', cwd=work) == os.environ['CANDIDATE_CLIPPY_TREE']

def examine(label, target):
    location = root / ('noon2308-inventory-' + label)
    git('worktree', 'add', '--detach', str(location), candidate)
    git('config', 'user.name', 'Noon read-only inventory', cwd=location)
    git('config', 'user.email', 'inventory@example.invalid', cwd=location)
    result = git('merge', '--no-commit', '--no-ff', target, cwd=location, check=False)
    conflict_paths = get('diff', '--name-only', '--diff-filter=U', cwd=location).splitlines()
    unmerged = get('ls-files', '-u', cwd=location).splitlines()
    (out / (label + '-merge.log')).write_bytes(result.stdout + result.stderr)
    (out / (label + '-conflicted-paths.txt')).write_text('\n'.join(conflict_paths) + '\n')
    (out / (label + '-index-stages.txt')).write_text('\n'.join(unmerged) + '\n')
    (out / (label + '-conflict.diff')).write_bytes(git('diff', '--cc', cwd=location).stdout)
    common = get('merge-base', candidate, target, cwd=location)
    status = {'target':target, 'merge_base':common,
              'exit_code':result.returncode, 'conflict_count':len(conflict_paths),
              'conflict_paths':conflict_paths, 'unmerged_stage_count':len(unmerged),
              'trivial_clean_merge':result.returncode == 0 and not conflict_paths}
    assert result.returncode in (0, 1), status
    print(label, json.dumps(status, indent=2), flush=True)
    return status

results = {
    'purpose': 'Read-only inventory, not integration acceptance',
    'source_artifact_run': 40503810362,
    'candidate_local_only_commit': candidate,
    'candidate_tree': os.environ['CANDIDATE_CLIPPY_TREE'],
    'source_artifact_sha256': os.environ['ARCHIVE_SHA256'],
    'references': refs,
    'merges': {
        'current_master': examine('current-master', refs['current_master']),
        'memory_stack': examine('memory-stack', refs['memory']),
    },
}
(out / 'inventory.json').write_text(json.dumps(results, indent=2) + '\n')
print('Source-authenticated conflict inventory complete; NO remote references updated.', flush=True)
