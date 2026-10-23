"""Temporary #2211 exact-source qualification. Never updates repository refs."""
import base64
import hashlib
import json
import lzma
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import urllib.request

out = Path('/tmp/noon-canvas')
out.mkdir(exist_ok=True)
payload = Path('.github/diagnostics/prepared-canvas.xz').read_bytes()
assert hashlib.sha256(payload).hexdigest() == '28fd29e47c8ca98c88cac3f9a32a12b9550a14292673639e81d14a2e3b3a359a'
recipe = json.loads(lzma.decompress(payload))
paths = [row['path'] for row in recipe['files']]
assert len(paths) == len(set(paths))
assert all(not Path(p).is_absolute() and '..' not in Path(p).parts and (p.startswith('crates/') or p.startswith('scripts/') or p in ['docs/architecture.md', '.github/workflows/playground-playback-controls.yml']) for p in paths)

def blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

mode = sys.argv[1]
if mode == 'apply':
    for row in recipe['files']:
        path = Path(row['path'])
        original = path.read_bytes() if path.exists() else b''
        assert (blob(original) if path.exists() else None) == row['base'], path
        lines = original.decode().splitlines(keepends=True)
        for start, end, replacement in reversed(row['edits']):
            lines[start:end] = replacement.splitlines(keepends=True)
        data = ''.join(lines).encode()
        assert hashlib.sha256(data).hexdigest() == row['result'], path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    fixes = Path('.github/diagnostics/prepared-canvas-fixes.json')
    if fixes.exists():
        for fix in json.loads(fixes.read_text()):
            assert fix['path'] in paths
            path = Path(fix['path'])
            value = path.read_text()
            assert value.count(fix['old']) == 1, (fix['path'], fix['old'])
            path.write_text(value.replace(fix['old'], fix['new']))
    (out / 'product-base.txt').write_text(recipe['product_base'] + '\n')
    (out / 'diagnostic-head.txt').write_bytes(subprocess.check_output(['git', 'rev-parse', 'HEAD']))
    (out / 'paths.json').write_text(json.dumps(paths, indent=2))
elif mode == 'save':
    subprocess.run(['git', 'add', '--', *paths], check=True)
    subprocess.run(['git', 'diff', '--cached', '--check'], check=True)
    (out / 'candidate.patch').write_bytes(subprocess.check_output(['git', 'diff', '--cached', '--binary', 'HEAD', '--', *paths]))
    with tarfile.open(out / 'candidate-files.tar.gz', 'w:gz') as archive:
        for path in paths:
            archive.add(path, arcname=path)
    (out / 'file-hashes.json').write_text(json.dumps({p: blob(Path(p).read_bytes()) for p in paths}, indent=2))
elif mode == 'store':
    entries = []
    for path in paths:
        data = Path(path).read_bytes()
        request = urllib.request.Request('https://api.github.com/repos/yongkyuns/noon/git/blobs', json.dumps({'content': base64.b64encode(data).decode(), 'encoding': 'base64'}).encode(), headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json', 'X-GitHub-Api-Version': '2022-11-28'})
        with urllib.request.urlopen(request, timeout=60) as response:
            saved = json.load(response)
        assert saved['sha'] == blob(data), path
        entries.append({'path': path, 'sha': saved['sha'], 'mode': '100644', 'type': 'blob'})
    (out / 'blobs.json').write_text(json.dumps(entries, indent=2))
else:
    raise SystemExit('Unknown operation')
