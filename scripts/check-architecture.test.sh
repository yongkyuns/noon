#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export NOON_TEST_SOURCE_ROOT="$ROOT"
python3 - <<'PY'
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

source = Path(os.environ['NOON_TEST_SOURCE_ROOT'])
cargo = shutil.which('cargo')
if cargo is None:
    raise SystemExit('Cargo is required; common-entrypoint fixtures cannot be skipped')

def run(args, root, **kwargs):
    return subprocess.run(args, cwd=root, text=True, capture_output=True,
                          env=dict(os.environ, GIT_OPTIONAL_LOCKS='0'), **kwargs)

def git(root, *args):
    return run(['git', *args], root, check=True).stdout.strip()

with tempfile.TemporaryDirectory(prefix='noon architecture gate ') as directory:
    root = Path(directory) / 'repo'
    root.mkdir()
    (root / 'scripts').mkdir()
    names = ['check.sh', 'check-architecture.sh', 'layer-dependency-ratchet.sh',
             'layer_dependencies.py', 'architecture-ratchet.sh',
             'architecture_migration_relocations.py', 'architecture_migration_relocations.json',
             'noon-core-module-ownership-ratchet.sh', 'renderer-host-boundary-ratchet.sh',
             'active-perf-frontend-ratchet.sh']
    for name in names:
        shutil.copyfile(source / 'scripts' / name, root / 'scripts' / name)
    (root / '.gitignore').write_text('ignored/\n__pycache__/\n')
    (root / 'Cargo.toml').write_text('[workspace]\nmembers = ["crates/*"]\nresolver = "2"\n')
    for name in ['noon-core', 'noon-compile', 'noon-runtime', 'noon-render-wgpu', 'noon', 'noon-native', 'noon-web']:
        path = root / 'crates' / name
        (path / 'src').mkdir(parents=True)
        (path / 'src/lib.rs').write_text('')
        (path / 'Cargo.toml').write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n')
    (root / 'crates/noon-core/src/semantic_store.rs').write_text('struct SemanticNodeId;\nimpl SemanticNodeId {}\nstruct SemanticStore;\n')
    (root / 'crates/noon-web/src/clock.rs').write_text('struct PlaybackClock;\n')
    (root / 'web').mkdir()
    for name in ['perf-profile.js', 'scene-perf.js']:
        (root / 'web' / name).write_text('// clean\n')
    git(root, 'init', '-q', '-b', 'master')
    git(root, 'config', 'user.name', 'Architecture fixture')
    git(root, 'config', 'user.email', 'architecture@example.invalid')
    git(root, 'add', '.')
    git(root, '-c', 'commit.gpgSign=false', '-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'fixture')
    base = git(root, 'rev-parse', 'HEAD')
    git(root, 'update-ref', 'refs/remotes/origin/master', base)

    def gate(label, expected=0, contains='', args=('architecture', 'HEAD'), repo=root):
        index_path = Path(git(repo, 'rev-parse', '--path-format=absolute', '--git-path', 'index'))
        before_index = index_path.read_bytes()
        before_status = git(repo, 'status', '--porcelain=v1', '--untracked-files=all')
        started = time.perf_counter()
        result = run(['bash', str(repo / 'scripts/check.sh'), *args], repo.parent)
        elapsed = time.perf_counter() - started
        output = result.stdout + result.stderr
        assert result.returncode == expected, (label, result.returncode, output)
        assert contains in output, (label, contains, output)
        assert index_path.read_bytes() == before_index, (label, 'real index changed')
        assert git(repo, 'status', '--porcelain=v1', '--untracked-files=all') == before_status, (label, 'worktree changed')
        print(f'{label}: {elapsed:.3f}s (exit {result.returncode})', flush=True)

    gate('clean root commit; explicit HEAD', contains='architecture gate passed')
    gate('default origin/master', args=('architecture',), contains=f'base origin/master ({base})')
    gate('missing comparison base', 2, 'comparison base is unavailable', ('architecture', 'missing-base'))
    gate('bad mode', 2, 'unknown check mode', ('unknown',))
    gate('excess arguments', 2, 'Usage:', ('architecture', 'HEAD', 'unexpected'))
    git(root, 'update-ref', '-d', 'refs/remotes/origin/master')
    gate('missing default is not replaced with HEAD', 2, 'comparison base is unavailable', ('architecture',))
    git(root, 'update-ref', 'refs/remotes/origin/master', base)

    # Real violations through the public entrypoint, never substitutes for guards.
    cases = [
        ('layer alias', 'crates/noon-core/Cargo.toml', '\n[dependencies]\nengine = { package = "noon-runtime", path = "../noon-runtime", optional = true }\n', 'must not depend on noon-runtime'),
        ('core ownership', 'crates/noon-core/src/lib.rs', '\ninclude!("other.rs");\n', 'noon-core module ownership ratchet:'),
        ('renderer host', 'crates/noon-render-wgpu/src/new host.rs', 'use winit::event_loop;\n', 'renderer host-boundary ratchet failed'),
        ('performance frontend', 'web/perf-profile.js', '\nconst bad = demoSceneJson();\n', 'active perf frontend ratchet:'),
        ('crate-private export', 'crates/noon-web/src/lib.rs', 'pub use legacy::*;\n', 'ScenePlayer must remain crate-private'),
        ('migration growth', 'crates/noon-runtime/src/new.rs', 'struct SceneDocument;\n', 'architecture ratchet:'),
        ('identity authority', 'crates/noon-core/src/duplicate.rs', 'struct SemanticNodeId;\n', 'SemanticNodeId definition must exist exactly once'),
        ('structural consumer', 'crates/noon-web/src/consumer.rs', 'use crate::legacy::ScenePlayer;\n', 'ScenePlayer consumer outside migration allowlist'),
    ]
    for label, relative, addition, diagnostic in cases:
        path = root / relative
        original = path.read_bytes() if path.exists() else None
        for staged in (False, True):
            path.write_text((original.decode() if original else '') + addition)
            if staged:
                git(root, 'add', '--', relative)
            gate(label + (' staged' if staged else ' working tree'), 1, diagnostic)
            git(root, 'reset', '-q', 'HEAD', '--', relative)
            if original is None:
                path.unlink()
            else:
                path.write_bytes(original)

    manifest = root / 'crates/noon-core/Cargo.toml'
    original = manifest.read_bytes()
    manifest.write_text('[broken\n')
    gate('malformed manifest fails closed', 2, 'cargo metadata failed')
    manifest.write_bytes(original)

    # Nonignored untracked declarations are structural input; ignored junk is not.
    ignored = root / 'ignored/junk.rs'
    ignored.parent.mkdir()
    ignored.write_text('struct SemanticNodeId;\n')
    gate('ignored junk excluded')
    git(root, 'add', '-f', 'ignored/junk.rs')
    gate('explicitly staged ignored source included', 1, 'SemanticNodeId definition must exist exactly once')
    git(root, 'reset', '-q', 'HEAD', '--', 'ignored/junk.rs')
    ignored.unlink()
    git(root, 'update-index', '--split-index')
    gate('split real index preserved')
    git(root, 'update-index', '--no-split-index')

    # A shallow checkout is fine with an available explicit base, never with an
    # unavailable ancestor. The gate itself must not fetch or choose another base.
    git(root, '-c', 'commit.gpgSign=false', '-c', 'core.hooksPath=/dev/null', 'commit', '--allow-empty', '-qm', 'second')
    shallow = Path(directory) / 'shallow'
    git(root, 'clone', '-q', '--depth=1', root.as_uri(), str(shallow))
    gate('shallow explicit HEAD', repo=shallow)
    gate('shallow missing ancestor', 2, 'comparison base is unavailable', ('architecture', base), repo=shallow)

print('common architecture gate self-test passed')
PY
