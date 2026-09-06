#!/usr/bin/env python3
"""Validate the bounded #959 relocation and canonical authoring ownership.

Print approved path/token pairs for the migration-growth check. This is not a
legacy-directory exemption: every location, imported symbol, and initial budget
is explicit, budgets shrink with the comparison base, and the canonical modules
are checked from the working tree even when a regression predates that base.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONFIG = Path(__file__).with_suffix('.json')
CONFIG_REPO_PATH = CONFIG.relative_to(ROOT).as_posix()
FORBIDDEN_CANONICAL = re.compile(
    r'SceneDefinition|SceneSpec|SceneDocument|ObjectDefinition|ObjectSnapshot|'
    r'from_legacy|\blegacy\b|noon_ir|noon-ir|'
    r'(?:import|export|replace)_mobject_snapshot'
)


def git(*args: str) -> str:
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True, stderr=subprocess.DEVNULL)


def at_base(base: str, path: str) -> str:
    try:
        return git('show', f'{base}:{path}')
    except subprocess.CalledProcessError:
        return ''


def normalized_namespaces(source: str) -> str:
    source = re.sub(r'\br#([A-Za-z_][A-Za-z0-9_]*)', r'\1', source)
    return re.sub(r'\s*::\s*', '::', source)


def token_count(source: str, token: str) -> int:
    if '::' in token:
        source = normalized_namespaces(source)
    return source.count(token)


def legacy_namespace(leaf: str, roots: tuple[str, ...]) -> bool:
    return any(leaf == root or leaf.startswith(root + '::') or leaf.startswith(root + ' as ') for root in roots)


def imports(source: str) -> list[tuple[bool, str]]:
    """Expand Rust use trees, preserving aliases and glob identities.

    Comments are removed before parsing. Unsupported use syntax fails closed;
    allowances are never inferred from a substring in a grouped declaration.
    """
    source = re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S)
    result: list[tuple[bool, str]] = []
    for match in re.finditer(r'\b(pub(?:\s*\([^)]*\))?\s+)?use\s+([^;]+);', source):
        tokens = re.findall(r'::|[A-Za-z_][A-Za-z0-9_]*|[{},*]', match[2])
        if re.sub(r'\s+', '', match[2]) != ''.join(tokens):
            if re.search(r'\blegacy\b', match[2]):
                raise ValueError('unsupported legacy use declaration: ' + match[0])
            continue
        position = 0
        leaves: list[str] = []

        def tree(prefix: list[str]) -> None:
            nonlocal position
            path = list(prefix)
            while position < len(tokens):
                token = tokens[position]
                position += 1
                if token == '{':
                    while position < len(tokens) and tokens[position] != '}':
                        tree(path)
                        if position < len(tokens) and tokens[position] == ',':
                            position += 1
                    if position == len(tokens):
                        raise ValueError('unclosed import group')
                    position += 1
                    return
                if token in {',', '}', '::', 'as'}:
                    raise ValueError('invalid import tree')
                path.append(token)
                if position < len(tokens) and tokens[position] == '::':
                    position += 1
                    continue
                alias = ''
                if position < len(tokens) and tokens[position] == 'as':
                    position += 1
                    if position == len(tokens):
                        raise ValueError('missing import alias')
                    alias = ' as ' + tokens[position]
                    position += 1
                leaves.append('::'.join(path) + alias)
                return
            raise ValueError('incomplete import tree')

        try:
            tree([])
            if position != len(tokens):
                raise ValueError('trailing import tokens')
        except ValueError as error:
            if re.search(r'\blegacy\b', match[2]):
                raise ValueError('unsupported legacy use declaration: ' + match[0]) from error
            continue
        result.extend((bool(match[1]), leaf) for leaf in leaves)
    return result


def main() -> int:
    if len(sys.argv) != 2:
        print('usage: architecture_migration_relocations.py BASE', file=sys.stderr)
        return 2
    base = sys.argv[1]
    try:
        git('cat-file', '-e', f'{base}^{{commit}}')
    except subprocess.CalledProcessError:
        print(f'architecture relocation base is unavailable: {base}', file=sys.stderr)
        return 2
    config = json.loads(CONFIG.read_text())
    if config['deletion_issue'] != '#959':
        raise ValueError('relocation allowances require their deletion owner')
    paths = git('ls-files', '--cached', '--others', '--exclude-standard', '--', '*.rs', '*.py', '*.js', '*.mjs', '*.ts', '*.tsx').splitlines()
    errors: list[str] = []
    sources = {path: (ROOT / path).read_text() for path in paths if (ROOT / path).is_file()}
    previous_config_source = at_base(base, CONFIG_REPO_PATH)
    previous_config = json.loads(previous_config_source) if previous_config_source else {}
    retired_reactive_symbols = re.compile(r'\b(?:TimedScenePlayer|ReactiveScenePlayer|ReactiveCanvasPlayer|WasmReactiveScenePlayer|WasmReactiveCanvasPlayer|NativeInputRouter)\b')
    for path, source in sources.items():
        if path not in {'scripts/architecture_migration_relocations.py', 'scripts/check-web-package.mjs'} and retired_reactive_symbols.search(source):
            errors.append(f'{path}: deleted reactive runtime symbol returned; use the canonical execution session')
        if not path.endswith('.rs'):
            continue
        if re.search(r'\bFrontendMobjectHandle\b', source):
            errors.append(f'{path}: deleted FrontendMobjectHandle authority returned')
        if path in {'crates/noon/src/scene.rs', 'crates/noon/src/semantic_mobject.rs'} or path.startswith(('crates/noon/src/scene/', 'crates/noon/src/semantic_mobject/')):
            if FORBIDDEN_CANONICAL.search(source) or any('legacy' in leaf.split(' as ', 1)[0].split('::') for _, leaf in imports(source)):
                errors.append(f'{path}: canonical authoring regained a migration dependency')
        canonical_execution = path in {
            'crates/noon-compile/src/semantic_lowering.rs',
            'crates/noon/src/execution_session.rs',
            'crates/noon/src/live_session.rs',
        } or path.startswith((
            'crates/noon-compile/src/semantic_lowering/',
            'crates/noon/src/execution_session/',
        ))
        if canonical_execution:
            code = re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S)
            if re.search(r'\b(?:ScenePatch|MutationTransaction|ObjectDefinition)\b', code):
                errors.append(f'{path}: canonical execution depends on the external scene patch codec')
    codec = sources.get('crates/noon/src/legacy/semantic_snapshot.rs', '')
    if re.search(r'\b(?:struct|enum|impl|static)\s+[A-Za-z_]', re.sub(r'/\*.*?\*/|//[^\n]*', '', codec, flags=re.S)):
        errors.append('crates/noon/src/legacy/semantic_snapshot.rs: codec may contain only free adapters and tests, not another state owner or inherent API')
    root = sources.get('crates/noon/src/lib.rs', '')
    if re.search(r'\blegacy\s*::', root) or any('legacy' in leaf.split(' as ', 1)[0].split('::') for _, leaf in imports(root)):
        errors.append('crates/noon/src/lib.rs: legacy public reexport bypasses the canonical namespace')

    # The one-time larger caps require deletion of the old authority from the
    # same comparison. Once migrated files exist at base, the base is the cap.
    initial = bool(re.search(r'\bstruct\s+FrontendMobjectHandle\b', at_base(base, 'crates/noon-web/src/authoring_mobject.rs')))
    permissions: set[tuple[str, str]] = set()
    for path, limits in config['token_budgets'].items():
        source = sources.get(path, '')
        previous = at_base(base, path)
        for token, reviewed_cap in limits.items():
            cap = reviewed_cap if initial else min(token_count(previous, token), reviewed_cap)
            if token_count(source, token) > cap:
                errors.append(f'{path}: {token} count {token_count(source, token)} exceeds relocation budget {cap}')
            else:
                permissions.add((path, token))

    previous_fixtures = previous_config.get('regression_fixtures', {})
    regression_fixtures = config.get('regression_fixtures', {})
    for path in previous_fixtures.keys() - regression_fixtures.keys():
        errors.append(f'{path}: regression fixture budget entry was removed before #959 cleanup')
    for path, fixture in regression_fixtures.items():
        source = sources.get(path, '')
        previous = at_base(base, path)
        rust_module = fixture.get('rust_test_module')
        if rust_module is not None:
            parent_path = rust_module['parent']
            module = rust_module['module']
            parent = sources.get(parent_path, '')
            declaration = f'#[cfg(test)]\nmod {module};'
            module_declarations = re.findall(
                rf'(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+{re.escape(module)}\s*;',
                parent,
            )
            if source:
                if source.splitlines()[0] != '#![cfg(test)]':
                    errors.append(f'{path}: regression fixture must start with #![cfg(test)]')
                if parent.count(declaration) != 1 or len(module_declarations) != 1:
                    errors.append(
                        f'{parent_path}: regression fixture requires exact test-only declaration for {module}'
                    )
            elif module_declarations:
                errors.append(f'{parent_path}: deleted regression fixture module {module} remains declared')
        for token, reviewed_cap in fixture['tokens'].items():
            cap = (
                min(token_count(previous, token), reviewed_cap)
                if path in previous_fixtures
                else reviewed_cap
            )
            count = token_count(source, token)
            if count > cap:
                errors.append(f'{path}: {token} count {count} exceeds regression fixture budget {cap}')
            elif count:
                permissions.add((path, token))

    for path, allowed in config['rewritten_imports'].items():
        source = sources.get(path, '')
        current = Counter(leaf for _, leaf in imports(source) if legacy_namespace(leaf, ('noon::legacy',)))
        previous = Counter(leaf.replace('noon::legacy::', 'noon::', 1) for _, leaf in imports(at_base(base, path)))
        for leaf, count in current.items():
            normalized = leaf.replace('noon::legacy::', 'noon::', 1)
            if leaf not in allowed or count > min(allowed[leaf], previous[normalized]):
                errors.append(f'{path}: new legacy import {leaf}')
        # No raw namespace references are covered by this import permission.
        import_occurrences = len(re.findall(r'\buse\s+noon::legacy::', normalized_namespaces(re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S))))
        code = normalized_namespaces(re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S))
        if code.count('noon::legacy') != import_occurrences and path not in config['adapter_call_sites']:
            errors.append(f'{path}: legacy namespace use outside an approved import')
        if current and not any(error.startswith(path + ':') for error in errors):
            permissions.add((path, 'noon::legacy'))

    # Existing bridge call sites may only name the three explicit value codecs.
    for path in config['adapter_call_sites']:
        source = sources.get(path, '')
        calls = re.findall(r'noon::legacy::([A-Za-z_][A-Za-z0-9_]*)', normalized_namespaces(re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S)))
        if any(name not in {'import_mobject_snapshot', 'export_mobject_snapshot', 'replace_mobject_snapshot'} for name in calls):
            errors.append(f'{path}: new legacy adapter API')
    permitted_namespaces = set(config['rewritten_imports']) | set(config['adapter_call_sites'])
    for path, source in sources.items():
        if path.endswith('.rs') and path not in permitted_namespaces and ('noon::legacy' in normalized_namespaces(source) or any(legacy_namespace(leaf, ('noon::legacy',)) for _, leaf in imports(source))):
            errors.append(f'{path}: legacy namespace spread outside reviewed files')
    if errors:
        for error in errors:
            print('architecture relocation: ' + error, file=sys.stderr)
        return 1
    for path, token in sorted(permissions):
        print(path + '\t' + token)
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except ValueError as error:
        print('architecture relocation: ' + str(error), file=sys.stderr)
        raise SystemExit(1) from error
