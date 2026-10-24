"""Temporary exact-source qualification, not product runtime or a ref updater."""
import base64
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import urllib.request

ROOT = Path.cwd()
OUT = Path('/tmp/python-prefix')
OUT.mkdir(exist_ok=True)
payload = json.loads(gzip.decompress(base64.b64decode(Path('.github/diagnostics/python-prefix.patch.b64').read_text())))
paths = payload['paths']

def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()

def blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

mode = sys.argv[1]
if mode == 'apply':
    # The diagnostic commit adds only its own tooling above this product parent.
    assert git('rev-parse', 'HEAD^') == payload['base']
    for path, expected in payload['before'].items():
        target = ROOT / path
        assert not path.startswith('/') and '..' not in Path(path).parts
        actual = blob(target.read_bytes()) if target.exists() else None
        assert actual == expected, (path, actual, expected)
    patch = OUT / 'candidate.patch'
    patch.write_text(payload['patch'])
    subprocess.run(['git', 'apply', '--check', str(patch)], check=True)
    subprocess.run(['git', 'apply', str(patch)], check=True)
    subprocess.run(['git', 'add', '-N', '--', *paths], check=True)
    (OUT / 'product-base.txt').write_text(payload['base'] + '\n')
    (OUT / 'paths.json').write_text(json.dumps(paths, indent=2))
elif mode == 'save':
    changed = git('diff', '--name-only', 'HEAD').splitlines()
    assert set(changed) == set(paths), (changed, paths)
    hashes = {path: blob((ROOT / path).read_bytes()) for path in paths}
    (OUT / 'file-hashes.json').write_text(json.dumps(hashes, indent=2))
    with tarfile.open(OUT / 'candidate-files.tar.gz', 'w:gz') as archive:
        for path in paths:
            archive.add(ROOT / path, arcname=path)
    (OUT / 'candidate.patch').write_bytes(subprocess.check_output(['git', 'diff', '--binary', 'HEAD', '--', *paths]))
elif mode == 'upload':
    hashes = json.loads((OUT / 'file-hashes.json').read_text())
    blobs = {}
    for path in paths:
        data = (ROOT / path).read_bytes()
        assert blob(data) == hashes[path], path
        request = urllib.request.Request(
            f'https://api.github.com/repos/{os.environ["GITHUB_REPOSITORY"]}/git/blobs',
            data=json.dumps({'encoding': 'base64', 'content': base64.b64encode(data).decode()}).encode(),
            headers={'Authorization': f'Bearer {os.environ["GH_TOKEN"]}', 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json'},
            method='POST',
        )
        with urllib.request.urlopen(request) as response:
            stored = json.load(response)
        assert stored['sha'] == hashes[path], path
        blobs[path] = stored['sha']
    (OUT / 'blobs.json').write_text(json.dumps(blobs, indent=2))
else:
    raise SystemExit('unknown mode')
