"""Exact-source transfer only: never executes product code or changes refs."""
import base64, hashlib, json, os, re, subprocess, urllib.request, zlib
from pathlib import Path
ROOT = 'https://api.github.com/repos/yongkyuns/noon'
PARENT = 'ca12b315db431b0bfe5c15ebad2a5bda0e7ee2cc'
MASTER = 'e0459b34a9db795f28c82066c6b28402b124a476'
BASE = 'b13c09055fd1f837fd9ad10f1f04b5de3af91095'
TREE = '63ad141d23956847d7966d14b7588b0fd407ea65'
PARTS = ['eae469d888a0fdfcafe0b99f1773c9637528be95', 'c36dd7b055e51f6230174aab92fcde8b161bd525']
receipt = dict(parent=PARENT, master=MASTER, tree=TREE, passed=False, entries=[], scope='source blobs only; no tests or product ref updates')
def git(directory, *args):
    return subprocess.check_output(['git', '-C', str(directory), *args])
def api(path, value=None):
    data = None if value is None else json.dumps(value).encode()
    req = urllib.request.Request(ROOT + path, data=data, headers={'Authorization':'Bearer '+os.environ['GH_TOKEN'], 'Accept':'application/vnd.github+json', 'Content-Type':'application/json'}, method='GET' if value is None else 'POST')
    with urllib.request.urlopen(req, timeout=60) as response: return json.load(response)
def sha_blob(data):
    return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
try:
    source=Path.cwd(); base=source.parent/'base'; master=source.parent/'master'
    for directory, commit, tree in [(source,PARENT,'578e719c14d4690558d460c565cee263a3bbcecc'),(base,BASE,'6a2320edb3385903ed7ba410bc9fec141259358a'),(master,MASTER,'0966458e10aca4ddecb8c4c6df65bd18cf8afae3')]:
        assert git(directory,'rev-parse','HEAD').decode().strip()==commit
        assert git(directory,'rev-parse','HEAD^{tree}').decode().strip()==tree
    paths=set(git(base,'ls-files').decode().splitlines())|set(git(master,'ls-files').decode().splitlines())
    for path in sorted(paths):
        assert '..' not in Path(path).parts
        b=base/path; m=master/path; o=source/path
        before=b.read_bytes() if b.exists() else None
        theirs=m.read_bytes() if m.exists() else None
        if before==theirs: continue
        ours=o.read_bytes() if o.exists() else None
        if ours==theirs: continue
        if ours==before:
            if theirs is None: o.unlink()
            else:
                o.parent.mkdir(parents=True,exist_ok=True); o.write_bytes(theirs)
        else:
            assert ours is not None and before is not None and theirs is not None
            result=subprocess.run(['git','merge-file','-p','-L','M1','-L','common-b13','-L','master-e045',str(o),str(b),str(m)],capture_output=True)
            assert 0<=result.returncode<128
            text=result.stdout.decode()
            if result.returncode:
                assert path in ['crates/noon-web/src/retained_execution_transport.rs','web/authoring-render-transition-runtime.test.mjs','web/semantic-engine-endpoint.test.mjs']
                text,count=re.subn(r'<<<<<<< M1\n(.*?)=======\n.*?>>>>>>> master-e045\n',lambda match:match.group(1),text,flags=re.S)
                assert count>0 and '<<<<<<<' not in text
            o.write_text(text)
    git(source,'add','-A')
    assert git(source,'write-tree').decode().strip()=='060087d6d2ccd77875c9c6fb790b362bff1bca7f'
    pieces=[]
    for sha in PARTS:
        blob=api('/git/blobs/'+sha)
        data=base64.b64decode(blob['content'])
        assert sha_blob(data)==sha
        pieces.append(data.strip())
    patch=zlib.decompress(base64.b64decode(b''.join(pieces),validate=True))
    assert len(patch)==48626
    assert hashlib.sha256(patch).hexdigest()=='7bd12c877d0d372ac69bde54e39ac353fd474093ef032d4fc1f0dcaab88231ea'
    subprocess.run(['git','apply','--index','--whitespace=error-all','-'],input=patch,check=True)
    assert git(source,'write-tree').decode().strip()==TREE
    names=git(source,'diff','--cached','--name-only','-z').split(b'\0')
    for encoded in filter(None,names):
        path=encoded.decode()
        assert path.startswith(('.github/workflows/','crates/','scripts/','docs/','web/')) and '..' not in Path(path).parts
        assert Path(path).suffix in ['.rs','.mjs','.js','.md','.py','.sh','.yml']
        fields=git(source,'ls-files','-s','--',path).decode().split()
        mode,sha=fields[0:2]
        assert mode in ['100644','100755']
        content=git(source,'show',':'+path)
        content.decode('utf-8')
        assert sha_blob(content)==sha
        uploaded=api('/git/blobs',{'content':base64.b64encode(content).decode(),'encoding':'base64'})
        assert uploaded['sha']==sha
        receipt['entries'].append(dict(path=path,mode=mode,type='blob',sha=sha))
    assert len(receipt['entries'])==34
    receipt['passed']=True
finally:
    Path(os.environ['RUNNER_TEMP'],'m1-browser-source-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt,indent=2))
