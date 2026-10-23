"""Temporary exact-source qualification; removal owned by #2211."""
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import urllib.request
import zlib

BASE = 'a83e4ee62932cc062d41c6d2d5fe5fc38fef29e5'
ROOT = Path('/tmp/noon-prefix-browser')
ROOT.mkdir(exist_ok=True)
PATHS = [
    '.github/workflows/pr-fast.yml',
    'crates/noon-web/Cargo.toml',
    'crates/noon-web/src/browser_pointer_input.rs',
    'crates/noon-web/src/browser_pointer_input/prefix.rs',
    'crates/noon-web/src/browser_pointer_input/prefix/tests.rs',
    'crates/noon-web/src/direct_pointer_presentation.rs',
    'crates/noon-web/src/direct_pointer_presentation/inspection.rs',
    'crates/noon-web/src/execution_canvas.rs',
    'crates/noon-web/src/semantic_execution_player/pointer_input.rs',
    'crates/noon-web/src/worker_pointer_presentation.rs',
    'crates/noon/src/execution_session/replay/prefix/reader/input.rs',
    'crates/noon-web/src/retained_execution_resources/image_tests.rs',
]

def blob_sha(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

mode = sys.argv[1]
if mode == 'apply':
    payload = Path('.github/diagnostics/prefix-browser.patch.b64').read_text()
    (ROOT / 'payload.b64').write_text(payload)
    (ROOT / 'base.txt').write_text(BASE + '\n')
    (ROOT / 'diagnostic-head.txt').write_bytes(subprocess.check_output(['git', 'rev-parse', 'HEAD']))
    payload = payload.replace('CW5Xpppz', 'CW5Xppz')
    patch = zlib.decompress(base64.b64decode(payload))
    if hashlib.sha256(patch).hexdigest() != '61b083bdfc6f8a408d3f2b4872079449971ee0a3f4e83349e5525b8bc80e1d63':
        raise RuntimeError('candidate patch checksum mismatch')
    subprocess.run(['git', 'diff', '--exit-code', BASE, '--', *PATHS], check=True)
    subprocess.run(['git', 'apply', '--check', '-'], input=patch, check=True)
    subprocess.run(['git', 'apply', '-'], input=patch, check=True)
    # Existing renderer-dependent test helpers must follow their owner's feature.
    # All three assertions remain selected in the standard/all-features profile.
    path = Path('crates/noon-web/src/retained_execution_resources/image_tests.rs')
    if blob_sha(path.read_bytes()) != '3cd2467730be0c3f49ad97fea99b3c92ae31d5af':
        raise RuntimeError('unexpected image test baseline')
    source = path.read_text()
    for name in ['rejected_family_delta_does_not_run_resident_preparation', 'stale_family_delta_does_not_advance_resident_mesh_history', 'resident_preload_failure_does_not_reject_an_admitted_delta']:
        old = '#[test]\nfn ' + name + '('
        if source.count(old) != 1:
            raise RuntimeError('missing renderer test: ' + name)
        source = source.replace(old, '#[cfg(feature = "renderer")]\n' + old)
    old = 'fn family_delta(retained:'
    if source.count(old) != 1:
        raise RuntimeError('missing renderer test helper')
    path.write_text(source.replace(old, '#[cfg(feature = "renderer")]\n' + old))
    (ROOT / 'paths.json').write_text(json.dumps(PATHS, indent=2))
elif mode == 'save':
    subprocess.run(['git', 'add', '--', *PATHS], check=True)
    subprocess.run(['git', 'diff', '--cached', '--check'], check=True)
    (ROOT / 'candidate.patch').write_bytes(subprocess.check_output(['git', 'diff', '--cached', '--binary', 'HEAD', '--', *PATHS]))
    with tarfile.open(ROOT / 'candidate-files.tar.gz', 'w:gz') as archive:
        for path in PATHS:
            archive.add(path, arcname=path)
    (ROOT / 'file-hashes.json').write_text(json.dumps({p: blob_sha(Path(p).read_bytes()) for p in PATHS}, indent=2))
elif mode == 'store':
    entries = []
    for path in PATHS:
        data = Path(path).read_bytes()
        body = json.dumps({'content': base64.b64encode(data).decode(), 'encoding': 'base64'}).encode()
        request = urllib.request.Request('https://api.github.com/repos/yongkyuns/noon/git/blobs', body, headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json', 'X-GitHub-Api-Version': '2022-11-28'})
        with urllib.request.urlopen(request, timeout=60) as response:
            result = json.load(response)
        if result['sha'] != blob_sha(data):
            raise RuntimeError('stored blob mismatch: ' + path)
        entries.append({'path': path, 'mode': '100644', 'type': 'blob', 'sha': result['sha']})
    (ROOT / 'blobs.json').write_text(json.dumps(entries, indent=2))
else:
    raise SystemExit('unknown mode')
