"""Temporary exact-source qualification tooling; removal owned by #2211."""
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

EVIDENCE = Path('/tmp/noon-prefix-session')
EVIDENCE.mkdir(exist_ok=True)
payload = b''.join(Path(f'.github/diagnostics/prefix-session.part{i}').read_bytes() for i in (1, 2))
assert hashlib.sha256(payload).hexdigest() == 'a1f1899201a75696d2a00ef577d6cc797a43da7c38e11a5214d60f46cd979f50'
recipe = json.loads(lzma.decompress(payload))
paths = [entry['path'] for entry in recipe['files']]
assert len(paths) == len(set(paths))
assert all(not Path(p).is_absolute() and '..' not in Path(p).parts and (p.startswith('crates/') or p == 'docs/architecture.md') for p in paths)

def blob_sha(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

mode = sys.argv[1]
if mode == 'apply':
    for entry in recipe['files']:
        path = Path(entry['path'])
        original = path.read_bytes() if path.exists() else b''
        assert (blob_sha(original) if path.exists() else None) == entry['base'], path
        lines = original.decode().splitlines(keepends=True)
        for start, end, replacement in reversed(entry['edits']):
            lines[start:end] = replacement.splitlines(keepends=True)
        result = ''.join(lines).encode()
        assert hashlib.sha256(result).hexdigest() == entry['result'], path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(result)
    fixes = Path('.github/diagnostics/prefix-session-fixes.json')
    if fixes.exists():
        for fix in json.loads(fixes.read_text()):
            assert fix['path'] in paths
            path = Path(fix['path'])
            text = path.read_text()
            assert text.count(fix['old']) == 1, fix['path']
            path.write_text(text.replace(fix['old'], fix['new']))
    (EVIDENCE / 'source-commit.txt').write_bytes(subprocess.check_output(['git', 'rev-parse', 'HEAD']))
    (EVIDENCE / 'product-base.txt').write_text(recipe['product_base'] + '\n')
    (EVIDENCE / 'paths.json').write_text(json.dumps(paths, indent=2))
    print('Applied exact-baseline edits:', len(paths))
elif mode == 'save':
    subprocess.run(['git', 'add', '--', *paths], check=True)
    (EVIDENCE / 'candidate.patch').write_bytes(subprocess.check_output(['git', 'diff', '--cached', '--binary', 'HEAD', '--', *paths]))
    subprocess.run(['git', 'diff', '--cached', '--check'], check=True)
    with tarfile.open(EVIDENCE / 'candidate-files.tar.gz', 'w:gz') as archive:
        for path in paths:
            archive.add(path, arcname=path)
    print('Saved exact formatted candidate and patch')
elif mode == 'store':
    blobs = []
    for path in paths:
        data = Path(path).read_bytes()
        body = json.dumps({'content': base64.b64encode(data).decode(), 'encoding': 'base64'}).encode()
        request = urllib.request.Request('https://api.github.com/repos/yongkyuns/noon/git/blobs', body, headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json', 'X-GitHub-Api-Version': '2022-11-28'})
        with urllib.request.urlopen(request, timeout=60) as response:
            result = json.load(response)
        assert result['sha'] == blob_sha(data), path
        blobs.append({'path': path, 'sha': result['sha'], 'mode': '100644', 'type': 'blob'})
    (EVIDENCE / 'blobs.json').write_text(json.dumps(blobs, indent=2))
    print('Stored tested content-addressed blobs only:', len(blobs))
else:
    raise SystemExit('Unknown mode')
