from pathlib import Path
import re
import subprocess

paths = [Path(p) for p in subprocess.check_output(['git', 'ls-files'], text=True).splitlines()]
print('SOURCE', subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip())
for p in paths:
    if p.name == 'AGENTS.md':
        print('AGENT CONTRACT', p)
        if p.as_posix() != 'AGENTS.md':
            print(p.read_text())
print('\nPUBLIC FACILITIES')
modules = ('execution_segment', 'execution_session', 'host_callbacks', 'live_program', 'text_authoring', 'semantic_mobject')
for p in paths:
    if p.suffix == '.rs' and p.as_posix().startswith('crates/noon/src/') and any(p.stem == m or f'/{m}/' in p.as_posix() for m in modules):
        text = p.read_text()
        definitions = re.findall(r'(?m)^pub (?:struct|enum|trait|type|const|fn) ([A-Za-z_][A-Za-z0-9_]*)', text)
        exports = re.findall(r'(?m)^pub use [\s\S]*?;', text)
        if definitions or exports:
            print(p, ':', ', '.join(definitions))
            for e in exports:
                print(' ', ' '.join(e.split()))
print('\nEXTERNAL NOON IMPORTS')
for p in paths:
    if p.suffix != '.rs' or p.as_posix().startswith('crates/noon/src/'):
        continue
    text = p.read_text()
    imports = re.findall(r'(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+noon::[\s\S]*?;', text)
    names = sorted(set(re.findall(r'\bnoon::([A-Za-z_]\w*)', text)))
    if imports or names:
        print(p, ':', ', '.join(names))
        for i in imports:
            print(' ', ' '.join(i.split()))
print('\nCRATE ROOT CORE REFERENCES')
for p in paths:
    if p.suffix != '.rs' or not p.as_posix().startswith('crates/noon/src/') or p.stem == 'lib':
        continue
    text = p.read_text()
    for i in re.findall(r'(?m)^\s*use\s+crate::[\s\S]*?;', text):
        print(p, ':', ' '.join(i.split()))
print('\nRAW STORE CALLS')
for p in paths:
    if p.suffix != '.rs':
        continue
    for n, line in enumerate(p.read_text().splitlines(), 1):
        if '.store()' in line or 'fn store(' in line or 'Scene::with_store' in line:
            print(f'{p}:{n}:{line.strip()}')
