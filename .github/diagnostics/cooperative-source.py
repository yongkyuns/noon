"""Temporary exact-source qualification; deletion owned by #2211."""
import base64, hashlib, json, lzma, os, subprocess, sys, tarfile, urllib.request
from pathlib import Path
out = Path('/tmp/cooperative-source')
out.mkdir(exist_ok=True)
payload = Path('.github/diagnostics/cooperative-source.b64').read_bytes()
assert hashlib.sha256(payload).hexdigest() == '56c519b2b8165578dd16921571022136ed1e74ab2a69f5658c0595e6bb33e819'
recipe = json.loads(lzma.decompress(base64.b64decode(payload)))
paths = [e['path'] for e in recipe['files']]
assert len(paths) == len(set(paths))
assert all(not Path(p).is_absolute() and '..' not in Path(p).parts and (p.startswith(('crates/', 'web/', 'scripts/')) or p in ('docs/architecture.md', '.github/workflows/playground-playback-controls.yml')) for p in paths)
def blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
mode = sys.argv[1]
if mode == 'apply':
    for entry in recipe['files']:
        p = Path(entry['path'])
        original = p.read_bytes() if p.exists() else b''
        assert (blob(original) if p.exists() else None) == entry['base'], p
        lines = original.decode().splitlines(keepends=True)
        for start, end, text in reversed(entry['edits']):
            lines[start:end] = text.splitlines(keepends=True)
        data = ''.join(lines).encode()
        assert hashlib.sha256(data).hexdigest() == entry['result'], p
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
    fixes = Path('.github/diagnostics/cooperative-source-fixes.json')
    if fixes.exists():
        for fix in json.loads(fixes.read_text()):
            assert fix['path'] in paths
            p = Path(fix['path']); text = p.read_text()
            assert text.count(fix['old']) == 1, p
            p.write_text(text.replace(fix['old'], fix['new']))
    (out / 'product-base.txt').write_text(recipe['base'] + '\n')
    (out / 'diagnostic-head.txt').write_bytes(subprocess.check_output(['git','rev-parse','HEAD']))
    (out / 'paths.json').write_text(json.dumps(paths,indent=2))
elif mode == 'save':
    subprocess.run(['git','add','--',*paths],check=True)
    subprocess.run(['git','diff','--cached','--check'],check=True)
    (out/'candidate.patch').write_bytes(subprocess.check_output(['git','diff','--cached','--binary','HEAD','--',*paths]))
    with tarfile.open(out/'candidate-files.tar.gz','w:gz') as archive:
        for p in paths: archive.add(p,arcname=p)
elif mode == 'store':
    manifest = []
    for path in paths:
        data = Path(path).read_bytes()
        body = json.dumps({'encoding':'base64','content':base64.b64encode(data).decode()}).encode()
        request = urllib.request.Request('https://api.github.com/repos/yongkyuns/noon/git/blobs',body,headers={'Authorization':'Bearer '+os.environ['GH_TOKEN'],'Accept':'application/vnd.github+json','Content-Type':'application/json','X-GitHub-Api-Version':'2022-11-28'})
        with urllib.request.urlopen(request,timeout=60) as response: result=json.load(response)
        assert result['sha'] == blob(data), path
        manifest.append({'path':path,'sha':result['sha'],'mode':'100644','type':'blob'})
    (out/'blobs.json').write_text(json.dumps(manifest,indent=2))
else: raise SystemExit('unknown mode')
