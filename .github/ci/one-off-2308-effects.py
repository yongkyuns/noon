"""Pinned ordered-effect reconciliation; preserve historical click and seek semantics."""
from pathlib import Path
import hashlib, re, subprocess, json
CONFLICT = re.compile(r'(?ms)^<<<<<<< [^\n]+\n(.*?)^=======\n(.*?)^>>>>>>> [^\n]+\n')
PATHS = {
 'crates/noon-compile/src/semantic_lowering/compiled_scene.rs': 1,
 'crates/noon-compile/src/semantic_lowering/projection.rs': 4,
 'crates/noon-compile/src/transaction_preflight.rs': 5,
 'crates/noon-runtime/src/lib.rs': 1,
}
LIB='crates/noon-runtime/src/lib.rs'
SEEK='crates/noon-runtime/src/seek_storage_tests.rs'
STORAGE='crates/noon-compile/src/replay_revision/prefix/storage.rs'

def tests(text):
 return set(re.findall(r'#\[test\]\s*(?:#\[[^\]]+\]\s*)*fn\s+(\w+)',text))

def apply(work):
 work=Path(work)
 original={p:p.read_text() for p in work.rglob('*.rs')}
 decisions=[]
 for path,count in PATHS.items():
  f=work/path; raw=f.read_text(); assert len(list(CONFLICT.finditer(raw)))==count,path
  def resolve(m):
   ours,theirs=m.group(1),m.group(2)
   if path==LIB:
    assert ours.strip()=='let mut frame = FrameState {'
    assert 'let appearances = initial_scalar_property(' in theirs and 'effects: object.effects.clone(),' in theirs
    result=ours
   else:
    assert 'click_indicate' in ours and 'effects' in theirs and len(theirs.splitlines())==1,path
    keep=[line for line in ours.splitlines(keepends=True) if not re.match(r'^\s*(?:pub(?:\(super\))?\s+)?effect[:,]',line)]
    assert len(keep)==len(ours.splitlines())-1,(path,ours)
    assert 'click_indicate' in ''.join(keep)
    result=theirs+''.join(keep)
   decisions.append({'file':path,'ours':hashlib.sha256(ours.encode()).hexdigest(),'theirs':hashlib.sha256(theirs.encode()).hexdigest(),'resolution':hashlib.sha256(result.encode()).hexdigest()})
   return result
  resolved=CONFLICT.sub(resolve,raw); assert '<<<<<<<' not in resolved; f.write_text(resolved)
 def replace(path,before,after,n=1):
  f=work/path; raw=f.read_text(); assert raw.count(before)==n,(path,before,raw.count(before)); f.write_text(raw.replace(before,after))
 replace(LIB,'effect: object.effect.clone(),','effects: object.effects.clone(),')
 replace(SEEK,'effect: object.effect.clone(),','effects: object.effects.clone(),')
 replace('crates/noon-compile/src/transaction_preflight.rs','effect: row.effect.clone(),','effects: row.effects.clone(),')
 replace('crates/noon-compile/src/replay_revision/prefix.rs','|| object.effect.is_some()','|| object.effects.is_some()')
 replace('crates/noon-runtime/src/replay/prefix/storage.rs','effect: None,','effects: None,')
 replace(SEEK,'use noon_compile::CompiledEffect;','use noon_compile::{CompiledEffect, CompiledEffectStack};')
 replace(SEEK,'EffectDefinition, Glow, GradientStop, LinearGradient, SemanticNodeId, SemanticPaint,','EffectDefinition, EffectUpdate, Glow, GradientStop, LinearGradient, SemanticNodeId, SemanticPaint,')
 replace(SEEK,'''    let effect = Arc::new(CompiledEffect {
        attachment: SemanticNodeId::new(7, 1),
        definition: EffectDefinition::Glow(Glow::default()),
    });''','''    let identities = [SemanticNodeId::new(7, 1), SemanticNodeId::new(8, 2)];
    let definitions = [
        EffectDefinition::Glow(Glow::new(EffectUpdate::default().color(Color::RED).intensity(0.25)).unwrap()),
        EffectDefinition::Glow(Glow::new(EffectUpdate::default().color(Color::BLUE).intensity(0.75)).unwrap()),
    ];
    let effect = Arc::new(CompiledEffectStack::new([
        CompiledEffect { attachment: identities[0], definition: definitions[0] },
        CompiledEffect { attachment: identities[1], definition: definitions[1] },
    ]).unwrap());''')
 replace(SEEK,'row.effect = Some(effect.clone());','row.effects = Some(effect.clone());')
 replace(SEEK,'                .effect\n','                .effects\n')
 replace(SEEK,'''        assert!(
            runtime.frame().objects[0]
                .effects''','''        if let Some(stack) = runtime.frame().objects[0].effects.as_ref() {
            assert_eq!(stack.attachments(), &identities, "seek reordered retained effect bindings");
            assert_eq!(stack.definitions(), &definitions, "seek changed retained effect definitions");
        }
        assert!(
            runtime.frame().objects[0]
                .effects''')
 extra='''
    #[test]
    fn ordered_effect_rows_remain_outside_prefix_suffix_admission() {
        use crate::{CompiledEffect, CompiledEffectStack, CompiledObject, ExecutionPatch};
        use noon_core::{Color, EffectDefinition, GeometryRef, Glow, ObjectId, SemanticNodeId, Style, Transform2D};
        let id = ObjectId::new(0);
        let effects = std::sync::Arc::new(CompiledEffectStack::new([
            CompiledEffect { attachment: SemanticNodeId::new(7, 1), definition: EffectDefinition::Glow(Glow::default()) },
            CompiledEffect { attachment: SemanticNodeId::new(8, 2), definition: EffectDefinition::Glow(Glow::default()) },
        ]).unwrap());
        let mut row = CompiledObject::new(id, GeometryRef::circle(0.5), Transform2D::IDENTITY, Style::default());
        row.effects = Some(effects.clone());
        let mut scene = CompiledScene::compile_objects(vec![row], &[]).unwrap();
        let patch = ExecutionPatch::SetStyle { object: id, style: Style { fill: Some(Color::RED), ..Style::default() }, fill_paint: None };
        let inverse = scene.prepare_replay_revision(&patch).unwrap();
        scene.apply_execution_patch(&patch).unwrap();
        let rows = scene.objects().to_vec();
        for _ in 0..2 {
            assert!(scene.forward_prefix_revisions(&[&inverse]).is_none(), "effect stacks must not enter unqualified prefix suffixes");
            assert_eq!(scene.objects(), rows.as_slice());
            assert!(std::sync::Arc::ptr_eq(scene.objects()[0].effects.as_ref().unwrap(), &effects));
        }
    }
'''
 f=work/STORAGE;raw=f.read_text(); assert raw.rstrip().endswith('}'); assert 'ordered_effect_rows_remain_outside_prefix_suffix_admission' not in raw
 at=raw.rfind('}'); f.write_text(raw[:at]+extra+raw[at:])
 assert 'reset_base_frame(&self.compiled, &mut self.frame, time);' in (work/LIB).read_text()
 assert 'self.frame = base_frame(' not in (work/LIB).read_text()
 for p,old in original.items():
  new=p.read_text()
  assert tests(old)<=tests(new),(p,'lost tests')
  assert new.count('#[ignore')<=old.count('#[ignore'),(p,'added ignored tests')
  assert not CONFLICT.search(new),p
 return decisions


SOURCE_TREE = '1ef901ef0d649bd04e4054211f1e715ca87fac03'
SOURCE_HASH = '2139a31eeab3da11296fe1373c32f2593a9e7c65795e2f58d0493c35d9422df7'
EXPECTED_TREE = '71f3ded3a0b2e64dc8e1e05bc625fe0f75f01ad9'
BASE = '0351da54754cd2d5f95a7624aacd10872a5b8499'
LATEST = 'bc25e92bb741607f678f6adea528c9e894c6335c'
FEATURE = 'b3a4b7f484e607a4b14bc57e70f4947f8520ca42'
MEMORY = '472a61fadfdabba44756b75caaa184cee8c4be57'
SEEK_CASE = 'seek_storage_tests::merged_seek_preserves_paint_effect_and_reserved_storage'
GUARD_CASE = 'replay_revision::prefix::storage::tests::ordered_effect_rows_remain_outside_prefix_suffix_admission'

def git(work, *args, data=None, check=True):
 result = subprocess.run(['git', *args], cwd=work, input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
 if check and result.returncode:
  raise RuntimeError(f'git {args}: {result.stderr.decode(errors="replace")[-2000:]}')
 return result

def text(work, *args):
 return git(work, *args).stdout.decode().strip()

def stage(root):
 import shutil, tarfile
 repo=Path.cwd(); work=root/'noon2308-effects'; out=root/'noon2308-effects-evidence'; out.mkdir(parents=True,exist_ok=True)
 inp=root/'qualified-input'; archive=inp/'combined-source.tar.gz'; m=json.loads((inp/'manifest.json').read_text())
 assert hashlib.sha256(archive.read_bytes()).hexdigest()==SOURCE_HASH
 assert m['tested_tree']==SOURCE_TREE and m['tracked_source_unchanged']
 assert set(m['command_exit_codes'])=={'fast','prepared','controls','wasm','web'} and all(v==0 for v in m['command_exit_codes'].values())
 git(repo,'fetch','--no-tags','--depth=256','origin',FEATURE,BASE,LATEST,MEMORY)
 for sha in (FEATURE,BASE,LATEST,MEMORY): assert text(repo,'rev-parse',sha+'^{commit}')==sha
 git(repo,'worktree','add','--detach',str(work),FEATURE)
 git(work,'config','user.name','Noon isolated qualification');git(work,'config','user.email','qualification@example.invalid')
 for p in work.iterdir():
  if p.name=='.git':continue
  if p.is_dir() and not p.is_symlink():shutil.rmtree(p)
  else:p.unlink()
 with tarfile.open(archive) as tar:
  assert all(not (p.name.startswith('/') or '..' in Path(p.name).parts or p.issym() or p.islnk()) for p in tar.getmembers())
  tar.extractall(work,filter='data')
 git(work,'add','--all');assert text(work,'write-tree')==SOURCE_TREE
 candidate=git(work,'commit-tree',SOURCE_TREE,'-p',FEATURE,'-p',BASE,'-p',MEMORY,data=b'Authenticated qualified combined source; local only\n').stdout.decode().strip()
 git(work,'checkout','--detach','--force',candidate)
 assert text(work,'merge-base',candidate,LATEST)==BASE
 merged=git(work,'merge','--no-commit','--no-ff',LATEST,check=False)
 (out/'merge.log').write_bytes(merged.stdout+merged.stderr)
 (out/'conflicts.diff').write_bytes(git(work,'diff','--cc').stdout)
 (out/'stages.txt').write_bytes(git(work,'ls-files','-u').stdout)
 assert merged.returncode==1 and set(text(work,'diff','--name-only','--diff-filter=U').splitlines())==set(PATHS)
 decisions=apply(work)
 git(work,'add','--all');assert not text(work,'ls-files','-u')
 git(work,'diff','--cached','--check')
 tree=text(work,'write-tree');assert tree==EXPECTED_TREE,(tree,EXPECTED_TREE)
 git(work,'commit','-m','Integrate qualified prepared memory stack with current ordered effects; local only')
 combined=text(work,'rev-parse','HEAD')
 assert text(work,'show','-s','--format=%P','HEAD').split()==[candidate,LATEST]
 for sha in (FEATURE,BASE,LATEST,MEMORY):git(work,'merge-base','--is-ancestor',sha,combined)
 record={'scope':'Isolated qualification; no remote feature/master writes','source_run':40571552550,'source_tree':SOURCE_TREE,'source_sha256':SOURCE_HASH,'feature':FEATURE,'previous_master':BASE,'latest_master':LATEST,'memory':MEMORY,'unformatted_tree':tree,'combined_local':combined,'conflict_resolutions':decisions}
 (out/'manifest.json').write_text(json.dumps(record,indent=2)+'\n')
 (out/'candidate-vs-master.patch').write_bytes(git(work,'diff','--binary',LATEST,'HEAD').stdout)
 (out/'candidate-vs-qualified.patch').write_bytes(git(work,'diff','--binary',candidate,'HEAD').stdout)
 print(json.dumps(record,indent=2),flush=True)

def controls(root):
 work=root/'noon2308-effects';out=root/'noon2308-effects-evidence';lib=work/LIB;guard=work/'crates/noon-compile/src/replay_revision/prefix.rs'
 originals={lib:lib.read_bytes(),guard:guard.read_bytes()};outcomes={}
 def run(label,case,crate):
  args=['cargo','test','--locked','-p',crate,'--features','prepared-prefix','--lib',case,'--','--exact','--test-threads=1']
  r=subprocess.run(args,cwd=work,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);log=r.stdout.decode(errors='replace')
  (out/(label+'.log')).write_bytes(r.stdout);outcomes[label]=r.returncode;print(log,flush=True);return r.returncode,log
 def positive(label,case,crate):
  code,log=run(label,case,crate);assert code==0 and 'test result: ok. 1 passed; 0 failed' in log
 try:
  positive('seek-positive',SEEK_CASE,'noon-runtime');positive('guard-positive',GUARD_CASE,'noon-compile')
  original=originals[lib];start=original.index(b'fn reset_base_frame(');end=original.index(b'\nconst PROPERTY_ORDER:');block=original[start:end]
  mutations=[('paint',b'fill_paint: object.fill_paint.clone(),',b'fill_paint: None,','seek lost the retained paint handle'),('effects',b'effects: object.effects.clone(),',b'effects: None,','seek lost the retained effect handle'),('order',b'effects: object.effects.clone(),',b'effects: object.effects.as_ref().map(|stack| Arc::new(noon_compile::CompiledEffectStack::new(stack.iter().rev()).unwrap())),','seek reordered retained effect bindings')]
  for name,needle,replacement,message in mutations:
   assert block.count(needle)==1
   lib.write_bytes(original[:start]+block.replace(needle,replacement)+original[end:])
   try:
    code,log=run('seek-negative-'+name,SEEK_CASE,'noon-runtime')
    assert code!=0 and 'error[E' not in log and 'test '+SEEK_CASE+' ... FAILED' in log and message in log
   finally:lib.write_bytes(original)
  needle=b'|| object.effects.is_some()';assert originals[guard].count(needle)==1
  guard.write_bytes(originals[guard].replace(needle,b'|| false'))
  try:
   code,log=run('guard-negative',GUARD_CASE,'noon-compile')
   assert code!=0 and 'error[E' not in log and 'test '+GUARD_CASE+' ... FAILED' in log and 'effect stacks must not enter unqualified prefix suffixes' in log
  finally:guard.write_bytes(originals[guard])
  positive('seek-restored',SEEK_CASE,'noon-runtime');positive('guard-restored',GUARD_CASE,'noon-compile')
 finally:
  for path,source in originals.items():path.write_bytes(source)
  (out/'mutation-outcomes.json').write_text(json.dumps(outcomes,indent=2)+'\n')
 git(work,'diff','--exit-code')

if __name__=='__main__':
 import os, sys
 root=Path(os.environ['RUNNER_TEMP'])
 if len(sys.argv)==2 and sys.argv[1]=='controls':controls(root)
 else:stage(root)
