#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Semantic store and reactive declarations use ordinary module ownership.
# No organizational path/include indirection remains permitted.
# Use the same baseline grep dependency as the other architecture guards.
# Scan the working tree (including untracked Rust files), and distinguish an
# empty match set from a tool/read failure so the guard cannot pass unchecked.
if module_indirections="$(
  grep -rnE --include='*.rs' \
    '^[[:space:]]*#\[[[:space:]]*path[[:space:]]*=|(^|[^[:alnum:]_])include![[:space:]]*(\(|\{|\[)' \
    crates/noon-core/src
)"; then
  :
else
  scan_status=$?
  if (( scan_status != 1 )); then
    echo "noon-core module ownership ratchet: source scan failed" >&2
    exit "$scan_status"
  fi
fi

if [[ -n "$module_indirections" ]]; then
  printf 'noon-core module ownership ratchet: unexpected indirection:\n%s\n' "$module_indirections" >&2
  echo 'noon-core ownership requires ordinary modules, without #[path] or include! indirection.' >&2
  exit 1
fi

# Protect the current Phase A ownership, not a permanent target crate map.
# Read real Rust tokens/scopes: comments, literals, nested modules and macro
# bodies cannot stand in for private out-of-line crate-root declarations.
# The repository architecture entrypoint already requires Python 3.10+.
if python3 -I -S - <<'PYTHON'
# Token-level module ownership only; this is not a Rust type checker or macro
# expander. Direct, unconditional root declarations are required for these owners.
# Literal/comment boundaries follow the Rust Reference's tokens/comments chapters.
import os
from pathlib import Path
import re
import sys

CORE = Path('crates/noon-core/src')
OWNERS = {'animation', 'publication', 'reactive', 'resources', 'semantic_store'}
RAW_STRING = re.compile(r'(?:br|cr|r)(#*)"')
QUOTED_STRING = re.compile(r'(?:b|c)?"(?:\\[\s\S]|[^"\\])*"')
CHAR = re.compile(r"b?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\r\n])|[^'\\\r\n])'")
IDENTIFIER = re.compile(r'(?:r#)?[^\W\d]\w*')
TRIVIA = re.compile(r'\s+|//[^\n]*')


def tokens(source):
    """Keep delimiters/identifiers; comments vanish and literals stay opaque."""
    result = []
    i = 0
    while i < len(source):
        match = TRIVIA.match(source, i)
        if match:
            i = match.end()
            continue
        if source.startswith('/*', i):
            depth = 1
            i += 2
            while depth:
                if i == len(source):
                    raise ValueError('unterminated block comment')
                if source.startswith('/*', i):
                    depth += 1
                    i += 2
                elif source.startswith('*/', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        raw = RAW_STRING.match(source, i)
        if raw:
            closing = '"' + raw[1]
            end = source.find(closing, raw.end())
            if end < 0:
                raise ValueError('unterminated raw string')
            i = end + len(closing)
            result.append('<literal>')
            continue
        quoted = QUOTED_STRING.match(source, i) or CHAR.match(source, i)
        if quoted:
            i = quoted.end()
            result.append('<literal>')
            continue
        if source[i] == '"' or source.startswith(('b"', 'c"', "b'"), i):
            raise ValueError('unterminated quoted literal')
        # An apostrophe not beginning a character literal is a lifetime/label.
        # Its name has no structural delimiters; keep the apostrophe opaque too.
        if source[i] == "'":
            lifetime = IDENTIFIER.match(source, i + 1)
            if not lifetime:
                raise ValueError('invalid character literal or lifetime')
            i = lifetime.end()
            result.append('<lifetime>')
            continue
        identifier = IDENTIFIER.match(source, i)
        if identifier:
            result.append(identifier[0])
            i = identifier.end()
        else:
            result.append(source[i])
            i += 1
    return result


def structure(code):
    """Return depth and closing-to-opening indices, rejecting malformed groups."""
    depths, openings, stack = [], {}, []
    pairs = {')': '(', ']': '[', '}': '{'}
    for i, token in enumerate(code):
        depths.append(len(stack))
        if token in ('(', '[', '{'):
            stack.append(i)
        elif token in pairs:
            if not stack or code[stack[-1]] != pairs[token]:
                raise ValueError('unbalanced Rust token delimiters')
            openings[i] = stack.pop()
    if stack:
        raise ValueError('unclosed Rust token delimiter')
    return depths, openings


def private_unconditional(code, index, openings):
    prefix = index
    if prefix and code[prefix - 1] == 'pub':
        return False
    if prefix and code[prefix - 1] == ')':
        opening = openings[prefix - 1]
        if opening and code[opening - 1] == 'pub':
            return False
    # Do not let a disabled/redirected root declaration stand in for an owner.
    # Inner crate attributes (#![...]) are not module attributes.
    while prefix and code[prefix - 1] == ']':
        opening = openings[prefix - 1]
        if not opening or code[opening - 1] != '#':
            break
        if code[opening + 1].removeprefix('r#') in ('cfg', 'cfg_attr', 'path'):
            return False
        prefix = opening - 1
    return True


def read_code(path):
    code = tokens(path.read_text(encoding='utf-8'))
    depths, openings = structure(code)
    return code, depths, openings


def reject(message):
    print('noon-core module ownership ratchet: ' + message, file=sys.stderr)
    raise SystemExit(1)


def check():
    code, depths, openings = read_code(CORE / 'lib.rs')
    declarations = {owner: [] for owner in OWNERS}
    for i in range(len(code) - 2):
        if code[i] != 'mod' or depths[i] != 0:
            continue
        owner = code[i + 1].removeprefix('r#')
        if owner in declarations:
            declarations[owner].append(
                code[i + 2] == ';' and private_unconditional(code, i, openings)
            )
    for owner in sorted(OWNERS):
        if declarations[owner] != [True]:
            reject(f'{owner} must remain an ordinary private root module '
                   '(one unconditional out-of-line crate-root declaration)')
        if sum(path.is_file() for path in (CORE / f'{owner}.rs', CORE / owner / 'mod.rs')) != 1:
            reject(f'{owner} must resolve to exactly one ordinary module file')

    files = [CORE / 'reactive.rs'] if (CORE / 'reactive.rs').is_file() else []
    def walk_error(error):
        raise error
    if (CORE / 'reactive').is_dir():
        for directory, _, names in os.walk(CORE / 'reactive', onerror=walk_error):
            files.extend(Path(directory) / name for name in names if name.endswith('.rs'))
    for path in sorted(files):
        code, _, _ = read_code(path)
        for i in range(len(code) - 2):
            if (code[i] == 'mod' and code[i + 1].removeprefix('r#') in OWNERS - {'reactive'}
                    and code[i + 2] in (';', '{')):
                reject(f'unrelated domain declared under reactive: {path}: {code[i + 1]}')


try:
    check()
except (OSError, UnicodeError, ValueError) as error:
    print(f'noon-core module ownership ratchet: token ownership scan failed: {error}', file=sys.stderr)
    raise SystemExit(2) from error
PYTHON
then
  :
else
  scan_status=$?
  echo "noon-core module ownership ratchet: token ownership check failed" >&2
  exit "$scan_status"
fi

echo "noon-core module ownership ratchet passed"
