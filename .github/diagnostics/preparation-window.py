"""Temporary exact-source qualification for #2211; remove after publication."""
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

OUT = Path('/tmp/preparation-window')
OUT.mkdir(exist_ok=True)
payload = base64.b64decode(Path('.github/diagnostics/preparation-window.b64').read_bytes())
assert hashlib.sha256(payload).hexdigest() == 'e9a2e357ca37214b8554816402dcfff03b7e84c6b6522404da11632cbb7d1540'
recipe = json.loads(lzma.decompress(payload))
paths = [entry['path'] for entry in recipe['files']]
assert len(paths) == len(set(paths))
assert all(not Path(p).is_absolute() and '..' not in Path(p).parts and p.startswith(('crates/', 'web/', 'scripts/', 'docs/', '.github/workflows/')) for p in paths)

def git_blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

mode = sys.argv[1]
if mode == 'apply':
    for entry in recipe['files']:
        path = Path(entry['path'])
        original = path.read_bytes()
        assert git_blob(original) == entry['base'], path
        lines = original.decode().splitlines(keepends=True)
        for start, end, replacement in reversed(entry['edits']):
            lines[start:end] = replacement.splitlines(keepends=True)
        data = ''.join(lines).encode()
        assert hashlib.sha256(data).hexdigest() == entry['result'], path
        path.write_bytes(data)
    fixes = Path('.github/diagnostics/preparation-window-fixes.json')
    if fixes.exists():
        for fix in json.loads(fixes.read_text()):
            assert fix['path'] in paths
            path = Path(fix['path'])
            text = path.read_text()
            assert text.count(fix['old']) == 1, fix['path']
            path.write_text(text.replace(fix['old'], fix['new']))
    (OUT / 'product-base.txt').write_text(recipe['product_base'] + '\n')
    (OUT / 'diagnostic-head.txt').write_bytes(subprocess.check_output(['git', 'rev-parse', 'HEAD']))
    (OUT / 'paths.json').write_text(json.dumps(paths, indent=2))
elif mode == 'save':
    subprocess.run(['git', 'add', '--', *paths], check=True)
    subprocess.run(['git', 'diff', '--cached', '--check'], check=True)
    (OUT / 'candidate.patch').write_bytes(subprocess.check_output(['git', 'diff', '--cached', '--binary', 'HEAD', '--', *paths]))
    with tarfile.open(OUT / 'candidate-files.tar.gz', 'w:gz') as archive:
        for path in paths:
            archive.add(path, arcname=path)
    (OUT / 'file-hashes.json').write_text(json.dumps({p: git_blob(Path(p).read_bytes()) for p in paths}, indent=2))
elif mode == 'store':
    blobs = []
    for path in paths:
        data = Path(path).read_bytes()
        body = json.dumps({'encoding': 'base64', 'content': base64.b64encode(data).decode()}).encode()
        request = urllib.request.Request('https://api.github.com/repos/yongkyuns/noon/git/blobs', body, headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json', 'X-GitHub-Api-Version': '2022-11-28'})
        with urllib.request.urlopen(request, timeout=60) as response:
            result = json.load(response)
        assert result['sha'] == git_blob(data), path
        blobs.append({'path': path, 'sha': result['sha'], 'mode': '100644', 'type': 'blob'})
    (OUT / 'blobs.json').write_text(json.dumps(blobs, indent=2))
else:
    raise SystemExit('Unknown mode')
