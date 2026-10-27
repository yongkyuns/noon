"""Reproduce the reviewed audio integration; publish no refs from this script."""
from pathlib import Path
import gzip
import hashlib
import json
import os
import subprocess
import sys

ROOT = Path.cwd()
PLAN = {
    'base': 'd6afd19037d8eb421fcbd3ddc222fa20f150922b',
    'head': '5cc7e16e2df651e62fe66e71b1b607e4e418f90e',
    'old_master': 'f102a74b9bf28440f4862e799604a57c7882fa68',
    'master': 'f71a4c121bc29921133f790ae9783e89e962e7d9',
    'old_resolved_tree': '5bda2373ae9b5467874573f5dbb7a997f07cc74f',
    'candidate_tree': '208489a69539639f1c42f67303a9609db7f1c09d',
    'patch_sha256': '17919e8afe152caa15d8c83e853684e5577b1e0a9ef8191ee386451cb4fb2861',
}
PATCH = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2]).resolve()
OUT.mkdir(parents=True, exist_ok=True)

def git(*args, data=None, check=True):
    proc = subprocess.run(['git', *args], input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check and proc.returncode:
        raise RuntimeError(f"git {' '.join(args)} failed: {proc.stderr.decode()}")
    return proc

def value(*args):
    return git(*args).stdout.decode().strip()

raw = PATCH.read_bytes()
if hashlib.sha256(raw).hexdigest() != PLAN['patch_sha256']:
    raise RuntimeError('Resolution patch checksum mismatch')
patch = gzip.decompress(raw)
if value('status', '--porcelain'):
    raise RuntimeError('Candidate source checkout is not clean')
if value('merge-base', PLAN['head'], PLAN['old_master']) != PLAN['base']:
    raise RuntimeError('Original merge base changed')
git('config', 'core.abbrev', '7')
git('config', 'merge.conflictStyle', 'diff3')
git('checkout', '--detach', PLAN['head'])
git('branch', 'local-master', PLAN['old_master'])
merge = git('merge', '--no-commit', '--no-ff', 'local-master', check=False)
(OUT / 'first-merge.log').write_bytes(merge.stdout + merge.stderr)
if merge.returncode != 1 or not value('diff', '--name-only', '--diff-filter=U'):
    raise RuntimeError('Expected reviewed first-stage conflicts were not reproduced')
(OUT / 'first-conflicts.txt').write_text(value('diff', '--name-only', '--diff-filter=U') + '\n')
# Apply exactly the reviewed resolution, including post-merge ownership and teardown fixes.
git('apply', '--check', '-', data=patch)
git('apply', '-', data=patch)
git('add', '-A')
if value('ls-files', '-u'):
    raise RuntimeError('Unresolved index entries remain')
old_tree = value('write-tree')
if old_tree != PLAN['old_resolved_tree']:
    raise RuntimeError(f'First-stage tree mismatch: {old_tree}')
git('commit', '-m', 'Temporary local old-master integration; never publish this intermediate commit')
merge = git('merge', '--no-commit', '--no-ff', PLAN['master'], check=False)
(OUT / 'current-merge.log').write_bytes(merge.stdout + merge.stderr)
if merge.returncode != 0 or value('ls-files', '-u'):
    raise RuntimeError('Current-master integration has unexpected conflicts')
git('diff', '--check')
tree = value('write-tree')
if tree != PLAN['candidate_tree']:
    raise RuntimeError(f'Current candidate tree mismatch: {tree}')
# Only the real published head and pinned master become product parents.
commit = value('commit-tree', tree, '-p', PLAN['head'], '-p', PLAN['master'],
               '-m', 'Merge current master into finite Python scene playback controls')
# Finish the temporary merge only in this isolated scratch checkout.
git('merge', '--quit')
git('checkout', '--detach', commit)
receipt = {**PLAN, 'preformat_commit': commit, 'preformat_tree': tree,
           'parents': [PLAN['head'], PLAN['master']],
           'rust_validation': 'pending', 'published': False}
(OUT / 'candidate-preformat.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt, indent=2))
