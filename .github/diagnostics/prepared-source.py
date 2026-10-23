"""Temporary exact-source qualification; cleanup owned by #2211."""
import base64, hashlib, json, lzma, os, subprocess, sys, tarfile, urllib.request
from pathlib import Path
out=Path('/tmp/noon-prepared-source');out.mkdir(exist_ok=True)
payload=Path('.github/diagnostics/prepared-source.xz').read_bytes()
assert hashlib.sha256(payload).hexdigest()=='79c3a8318c92403d1b74519e0ad6081e38e9b54c1883a058f893b427a34267a9'
recipe=json.loads(lzma.decompress(payload));paths=[e['path'] for e in recipe['files']]
assert len(paths)==len(set(paths))
assert all(not Path(p).is_absolute() and '..' not in Path(p).parts and (p.startswith(('crates/','web/','scripts/')) or p in ('docs/architecture.md','.github/workflows/playground-playback-controls.yml')) for p in paths)
def blob(data):return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
mode=sys.argv[1]
if mode=='apply':
 for e in recipe['files']:
  p=Path(e['path']);original=p.read_bytes() if p.exists() else b''
  assert (blob(original) if p.exists() else None)==e['base'],p
  lines=original.decode().splitlines(keepends=True)
  for start,end,text in reversed(e['edits']):lines[start:end]=text.splitlines(keepends=True)
  result=''.join(lines).encode();assert hashlib.sha256(result).hexdigest()==e['result'],p
  p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(result)
 fixes=Path('.github/diagnostics/prepared-source-fixes.json')
 if fixes.exists():
  for e in json.loads(fixes.read_text()):
   assert e['path'] in paths
   p=Path(e['path']);s=p.read_text();assert s.count(e['old'])==1,p;p.write_text(s.replace(e['old'],e['new']))
 (out/'product-base.txt').write_text(recipe['product_base']+'\n')
 (out/'diagnostic-head.txt').write_bytes(subprocess.check_output(['git','rev-parse','HEAD']))
 (out/'paths.json').write_text(json.dumps(paths,indent=2))
elif mode=='save':
 subprocess.run(['git','add','--',*paths],check=True)
 (out/'candidate.patch').write_bytes(subprocess.check_output(['git','diff','--cached','--binary','HEAD','--',*paths]))
 subprocess.run(['git','diff','--cached','--check'],check=True)
 with tarfile.open(out/'candidate-files.tar.gz','w:gz') as t:
  for p in paths:t.add(p,arcname=p)
 (out/'file-hashes.json').write_text(json.dumps({p:blob(Path(p).read_bytes()) for p in paths},indent=2))
elif mode=='store':
 entries=[]
 for p in paths:
  data=Path(p).read_bytes();body=json.dumps({'content':base64.b64encode(data).decode(),'encoding':'base64'}).encode()
  req=urllib.request.Request('https://api.github.com/repos/yongkyuns/noon/git/blobs',body,headers={'Authorization':'Bearer '+os.environ['GH_TOKEN'],'Accept':'application/vnd.github+json','Content-Type':'application/json'})
  with urllib.request.urlopen(req,timeout=60) as response:r=json.load(response)
  assert r['sha']==blob(data),p
  entries.append({'path':p,'sha':r['sha'],'type':'blob','mode':'100644'})
 (out/'blobs.json').write_text(json.dumps(entries,indent=2))
else:raise SystemExit('unknown mode')
