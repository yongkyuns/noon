#!/usr/bin/env python3
"""Keep retired models and namespaces absent from the complete working tree.

No migration allowances remain. These source checks apply even when a forbidden
consumer predates the current change; the shell entrypoint owns diff validation.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_CANONICAL = re.compile(
    r'SceneDefinition|SceneSpec|SceneDocument|ObjectDefinition|ObjectSnapshot|'
    r'from_legacy|\blegacy\b|noon_ir|noon-ir|'
    r'(?:import|export|replace)_mobject_snapshot'
)


def git(*args: str) -> str:
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True, stderr=subprocess.DEVNULL)


def normalized_namespaces(source: str) -> str:
    source = re.sub(r'\br#([A-Za-z_][A-Za-z0-9_]*)', r'\1', source)
    return re.sub(r'\s*::\s*', '::', source)


def legacy_namespace(leaf: str, roots: tuple[str, ...]) -> bool:
    return any(leaf == root or leaf.startswith(root + '::') or leaf.startswith(root + ' as ') for root in roots)


def imports(source: str) -> list[tuple[bool, str]]:
    """Expand Rust use trees, preserving aliases and glob identities.

    Comments are removed before parsing. Unsupported use syntax fails closed;
    grouped declarations cannot hide a forbidden namespace.
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
    if len(sys.argv) != 1:
        print('usage: architecture_retired_models.py', file=sys.stderr)
        return 2
    paths = git('ls-files', '--cached', '--others', '--exclude-standard', '--', '*.rs', '*.py', '*.js', '*.mjs', '*.ts', '*.tsx').splitlines()
    errors: list[str] = []
    sources = {path: (ROOT / path).read_text() for path in paths if (ROOT / path).is_file()}
    retired_reactive_symbols = re.compile(r'\b(?:TimedScenePlayer|ReactiveScenePlayer|ReactiveCanvasPlayer|WasmReactiveScenePlayer|WasmReactiveCanvasPlayer|NativeInputRouter)\b')
    for path, source in sources.items():
        if path not in {'scripts/architecture_retired_models.py', 'scripts/check-web-package.mjs'} and retired_reactive_symbols.search(source):
            errors.append(f'{path}: deleted reactive runtime symbol returned; use the canonical execution session')
        if not path.endswith('.rs'):
            continue
        if re.search(r'\b(?:SlottedSceneInstance|FrameSlotId|RetiredSlotCompactionPolicy|ExecutionCompactionStats|ExecutionCompactionError|TimedSceneInstance|TimedSceneRuntimeError|TimedSemanticScene|SignalTimelineDefinition|SignalTrackDefinition|SignalTimelineError|SemanticScene|SceneBuildError|RetainedFamilySceneInstance|RetainedTextFamilySceneInstance|RetainedFamilyPlanSceneInstance|RetainedFamilyRuntimeError|RetainedTextFamilyRuntimeError|RetainedFamilyPlanRuntimeError|RetainedTextFamilyFrame|FamilyAnimationRequest|FamilyAnimationRequestError|RetainedFamilyAnimationRequestPlanError)\b', source):
            errors.append(f'{path}: retired runtime wrapper returned; use ExecutionSession and shared runtime slots')
        if re.search(r'\bFrontendMobjectHandle\b', source):
            errors.append(f'{path}: deleted FrontendMobjectHandle authority returned')
        code = re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S)
        if re.search(r'\b(?:SceneDefinition|ObjectDefinition|ObjectSnapshot|ScenePatch|MutationTransaction|PatchError|MutationImpact|RetainedObjectDefinition|Easing|TextFamilyAnimationMode|TextFamilyAnimationDefinition|TextFamilyAnimationState|TextFamilyAnimationError|RetainedTextFamilyTransportState|RetainedTextFamilyTransportError|RetainedGraphTopology|MathLayoutArtifact)\b', code):
            errors.append(f'{path}: retired scene/patch model returned; use shared semantic operations and typed execution data')
        if path == 'crates/noon-web/src/retained_family_transport.rs' and re.search(r'\bSemanticStore\b', code):
            errors.append(f'{path}: family transport must preserve source identities without a receiver-owned semantic store')
        if path in {'crates/noon/src/scene.rs', 'crates/noon/src/semantic_mobject.rs'} or path.startswith(('crates/noon/src/scene/', 'crates/noon/src/semantic_mobject/')):
            if FORBIDDEN_CANONICAL.search(source) or any('legacy' in leaf.split(' as ', 1)[0].split('::') for _, leaf in imports(source)):
                errors.append(f'{path}: canonical authoring regained a migration dependency')
    root = sources.get('crates/noon/src/lib.rs', '')
    if re.search(r'\blegacy\s*::|\bmod\s+legacy\b', root) or any('legacy' in leaf.split(' as ', 1)[0].split('::') for _, leaf in imports(root)):
        errors.append('crates/noon/src/lib.rs: legacy public reexport bypasses the canonical namespace')

    for path, source in sources.items():
        if path.endswith('.rs') and ('noon::legacy' in normalized_namespaces(source) or any(legacy_namespace(leaf, ('noon::legacy',)) for _, leaf in imports(source))):
            errors.append(f'{path}: retired legacy namespace returned')
    if errors:
        for error in errors:
            print('architecture retired models: ' + error, file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except ValueError as error:
        print('architecture retired models: ' + str(error), file=sys.stderr)
        raise SystemExit(1) from error
