"""Build a Python reference from explicit public exports without executing Noon.

This is a source reference, not a claim of runtime/platform support. Re-exports,
annotations, defaults, aliases and declared members are resolved statically.
Dynamic members and unresolved external exports remain visible in coverage.
"""
from __future__ import annotations

import ast
import hashlib
from pathlib import Path
from typing import Any


class ReferenceError(ValueError):
    pass


class PythonReference:
    def __init__(self, directory: Path):
        self.directory = directory.resolve()
        self.modules: dict[str, tuple[str, ast.Module]] = {}

    def module(self, name: str) -> tuple[str, ast.Module]:
        if not name.replace('_', '').isalnum():
            raise ReferenceError(f'Unsupported local module: {name}')
        if name not in self.modules:
            path = self.directory / f'{name}.py'
            source = path.read_text(encoding='utf-8')
            self.modules[name] = (source, ast.parse(source, filename=str(path)))
        return self.modules[name]

    def public_names(self) -> dict[str, str]:
        _, tree = self.module('noon')
        lazy: dict[str, str] = {}
        exports: ast.AST | None = None
        for node in tree.body:
            if isinstance(node, ast.Assign):
                for target in node.targets:
                    if isinstance(target, ast.Name) and target.id == '_PUBLIC_EXPORTS':
                        lazy = ast.literal_eval(node.value)
                    if isinstance(target, ast.Name) and target.id == '__all__':
                        exports = node.value
        if not isinstance(exports, (ast.List, ast.Tuple)):
            raise ReferenceError('noon.__all__ must be an explicit list/tuple')
        names: dict[str, str] = {}
        for element in exports.elts:
            if isinstance(element, ast.Constant) and isinstance(element.value, str):
                names[element.value] = lazy.get(element.value, 'noon')
            elif isinstance(element, ast.Starred) and isinstance(element.value, ast.Name) and element.value.id == '_PUBLIC_EXPORTS':
                names.update(lazy)
            else:
                raise ReferenceError('Unrecognized public export expression; update extractor explicitly')
        return names

    def resolve(self, module: str, name: str, visited=None) -> tuple[str, ast.AST] | None:
        visited = set() if visited is None else visited
        key = module, name
        if key in visited or not (self.directory / f'{module}.py').is_file():
            return None
        visited.add(key)
        _, tree = self.module(module)
        imported_modules: dict[str, str] = {}
        # Last binding wins, as in Python. We never execute a binding.
        binding = None
        for node in tree.body:
            if isinstance(node, ast.Import):
                for alias in node.names:
                    imported_modules[alias.asname or alias.name] = alias.name
            if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name:
                binding = node
            elif isinstance(node, ast.ImportFrom):
                for alias in node.names:
                    if (alias.asname or alias.name) == name:
                        binding = ('import', node.module or '', alias.name)
            elif isinstance(node, ast.Assign):
                if any(isinstance(t, ast.Name) and t.id == name for t in node.targets):
                    binding = node
            elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.target.id == name:
                binding = node
        if isinstance(binding, tuple):
            return self.resolve(binding[1], binding[2], visited)
        if isinstance(binding, (ast.Assign, ast.AnnAssign)):
            value = binding.value
            if isinstance(value, ast.Name):
                return self.resolve(module, value.id, visited) or (module, binding)
            if isinstance(value, ast.Attribute) and isinstance(value.value, ast.Name):
                imported = imported_modules.get(value.value.id)
                if imported:
                    return self.resolve(imported, value.attr, visited) or (module, binding)
        return (module, binding) if binding is not None else None

    @staticmethod
    def signature(node: ast.FunctionDef | ast.AsyncFunctionDef, name: str) -> str:
        prefix = 'async ' if isinstance(node, ast.AsyncFunctionDef) else ''
        result = f'{prefix}{name}({ast.unparse(node.args)})'
        if node.returns is not None:
            result += f' -> {ast.unparse(node.returns)}'
        return result

    @staticmethod
    def parameters(node: ast.FunctionDef | ast.AsyncFunctionDef) -> list[dict[str, str | None]]:
        args = node.args
        positional = [*args.posonlyargs, *args.args]
        defaults = [None] * (len(positional) - len(args.defaults)) + list(args.defaults)
        result = []
        def add(arg, kind, default=None):
            result.append({'name': arg.arg, 'kind': kind,
                'annotation': ast.unparse(arg.annotation) if arg.annotation is not None else None,
                'default': ast.unparse(default) if default is not None else None})
        for index, (arg, default) in enumerate(zip(positional, defaults)):
            add(arg, 'positional-only' if index < len(args.posonlyargs) else 'positional or keyword', default)
        if args.vararg: add(args.vararg, '*args')
        for arg, default in zip(args.kwonlyargs, args.kw_defaults): add(arg, 'keyword-only', default)
        if args.kwarg: add(args.kwarg, '**kwargs')
        return result

    def callable(self, module, node, display_name) -> dict[str, Any]:
        return {'name': display_name, 'signature': self.signature(node, display_name),
            'parameters': self.parameters(node), 'returns': ast.unparse(node.returns) if node.returns else None,
            'doc': ast.get_docstring(node) or '', 'line': node.lineno, 'end_line': node.end_lineno,
            'module': module, 'decorators': [ast.unparse(n) for n in node.decorator_list]}

    def build(self) -> dict[str, Any]:
        exports, entries, unresolved = self.public_names(), [], []
        for name, module in sorted(exports.items(), key=lambda item: item[0].casefold()):
            found = self.resolve(module, name)
            if found is None:
                unresolved.append({'name': name, 'module': module, 'reason': 'External or dynamic export; not statically resolved'})
                continue
            module, node = found
            item = {'name': name, 'module': module, 'line': node.lineno,
                    'end_line': node.end_lineno, 'doc': '', 'members': []}
            if isinstance(node, ast.ClassDef):
                item.update(kind='class', doc=ast.get_docstring(node) or '',
                            bases=[ast.unparse(n) for n in node.bases], decorators=[ast.unparse(n) for n in node.decorator_list])
                # Only declared members: do not pretend inherited/dynamic ones are absent.
                for member in node.body:
                    if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)) and (not member.name.startswith('_') or member.name in ('__init__', '__new__')):
                        item['members'].append(self.callable(module, member, f'{name}.{member.name}'))
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                item.update(kind='function', **{k:v for k,v in self.callable(module, node, name).items() if k not in ('name', 'module', 'line', 'end_line')})
            else:
                item.update(kind='binding', expression=ast.unparse(node))
            entries.append(item)
        hashes = {f'web/python/{name}.py': hashlib.sha256(source.encode()).hexdigest()
                  for name, (source, _) in sorted(self.modules.items())}
        return {'export_count': len(exports), 'resolved_count': len(entries), 'unresolved': unresolved,
                'entries': entries, 'source_sha256': hashes,
                'scope': 'Explicit __all__ and _PUBLIC_EXPORTS; local aliases and declared members. No code execution, inherited-member expansion, or dynamically attached-member inference.'}
