import base64, gzip, hashlib, json, os, pathlib, subprocess, sys, tarfile, urllib.request

ROOT = pathlib.Path.cwd()
OUT = pathlib.Path('/tmp/noon-source-unwind')
OUT.mkdir(exist_ok=True)
raw = gzip.decompress(base64.b64decode((ROOT / '.github/diagnostics/source-unwind.b64').read_bytes()))
assert hashlib.sha256(raw).hexdigest() == 'd3723f56d71a00914fd9bbb9a0072c0618b60b8751160297d703ef90da4487ae'
ops = json.loads(raw)

def blob_sha(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

mode = sys.argv[1]
if mode in ('tests', 'fix'):
    for op in ops:
        if op['phase'] != mode:
            continue
        path = ROOT / op['path']
        if op['base'] is None:
            assert not path.exists(), path
            text = op['content']
        else:
            data = path.read_bytes()
            assert blob_sha(data) == op['base'], (path, blob_sha(data), op['base'])
            text = data.decode()
            start = op['offset']
            assert text[start:start + len(op['old'])] == op['old'], path
            text = text[:start] + op['new'] + text[start + len(op['old']):]
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
elif mode == 'save':
    paths = [op['path'] for op in ops]
    subprocess.run(['git', 'add', '--', *paths], check=True)
    changed = subprocess.check_output(['git', 'diff', '--name-only', 'HEAD']).decode().splitlines()
    assert set(changed) == set(paths), changed
    subprocess.run(['git', 'diff', '--cached', '--check'], check=True)
    (OUT / 'candidate.patch').write_bytes(subprocess.check_output(['git', 'diff', '--cached', '--binary']))
    (OUT / 'product-base.txt').write_text('9d4f75dfea5acdbafd0425b3b91ca5a34141ecee\n')
    (OUT / 'file-hashes.json').write_text(json.dumps({p: blob_sha((ROOT / p).read_bytes()) for p in paths}, indent=2))
    (OUT / 'paths.json').write_text(json.dumps(paths, indent=2))
    with tarfile.open(OUT / 'candidate-files.tar.gz', 'w:gz') as archive:
        for path in paths:
            archive.add(ROOT / path, arcname=path)
elif mode == 'upload':
    hashes = json.loads((OUT / 'file-hashes.json').read_text())
    result = []
    for path, expected in hashes.items():
        data = (ROOT / path).read_bytes()
        assert blob_sha(data) == expected
        body = json.dumps({'encoding': 'base64', 'content': base64.b64encode(data).decode()}).encode()
        request = urllib.request.Request('https://api.github.com/repos/yongkyuns/noon/git/blobs', data=body, headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json'})
        with urllib.request.urlopen(request) as response:
            sha = json.load(response)['sha']
        assert sha == expected
        result.append({'path': path, 'mode': '100644', 'type': 'blob', 'sha': sha})
    (OUT / 'blobs.json').write_text(json.dumps(result, indent=2))
else:
    raise SystemExit('unknown operation')
