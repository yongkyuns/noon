from pathlib import Path
import ast
import re
import subprocess

SOURCE = '0a69543ed69857ca2de15768f80ca14ddc3ab383'
REFERENCE = 'cd898035a272795eaf0cdbfcd3b267943dc22d49'
TEXT_FEATURES = 'all(feature = "native-text", feature = "bundled-fonts")'


def git(*args: str) -> str:
    return subprocess.check_output(['git', *args], text=True)


def replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise RuntimeError(f'expected one exact anchor, got {text.count(old)}: {old[:100]!r}')
    return text.replace(old, new, 1)


assert git('rev-parse', 'HEAD').strip() == SOURCE
path = Path('crates/noon/src/implicit_plotting.rs')
source = path.read_text()
source = replace_once(source, 'use noon_core::{PathCommand, Vec2, VectorPath};',
                      'use noon_core::{Vec2, VectorPath};\n#[cfg(test)]\nuse noon_core::PathCommand;')
path.write_text(source)

for name, count, geometry_members in [
    ('number_plane', 24, 'scene.add_many(&[(&content).into()])?;'),
    ('implicit_plotting', 16, 'scene.add_many(&[axes.family().into(), (&circle).into(), (&hyperbola).into()])?;'),
]:
    path = Path(f'crates/noon/src/example_scenes/{name}.rs')
    old = git('show', f'{REFERENCE}:{path.as_posix()}')
    assert old.count(', Text,') == 1
    old = old.replace(', Text,', ',', 1)
    marker = '\npub fn scene()'
    old = replace_once(old, marker, f'\n#[cfg({TEXT_FEATURES})]\nuse crate::Text;\n' + marker)
    begin = old.index('    let mut title =')
    end = old.index('    Ok(scene)', begin)
    labels_and_membership = old[begin:end]
    assert labels_and_membership.count('scene.add_many(') == 1
    assert labels_and_membership.count('scene.text(') == 2
    gated = f'    #[cfg({TEXT_FEATURES})]\n    {{\n'
    gated += ''.join('    ' + line if line.strip() else line for line in labels_and_membership.splitlines(keepends=True))
    gated += f'    }}\n    #[cfg(not({TEXT_FEATURES}))]\n    {geometry_members}\n'
    old = old[:begin] + gated + old[end:]
    frame = 'session' if name == 'number_plane' else 'execution'
    old = replace_once(old, f'assert_eq!({frame}.frame().objects.len(), {count});',
        f'assert_eq!({frame}.frame().objects.len(), if cfg!({TEXT_FEATURES}) {{ {count} }} else {{ {count - 2} }});')
    path.write_text(old)

# Bring over only the absent public adapter, not the historical Axes implementation.
path = Path('web/python/_manim_plotting.py')
current = path.read_text()
reference = git('show', f'{REFERENCE}:{path.as_posix()}')


def axes_node(text: str):
    classes = [node for node in ast.parse(text).body if isinstance(node, ast.ClassDef) and node.name == 'Axes']
    assert len(classes) == 1
    return classes[0]


current_axes = axes_node(current)
methods = [node for node in axes_node(reference).body if isinstance(node, ast.FunctionDef) and node.name == 'plot_implicit_curve']
assert len(methods) == 1
if not any(isinstance(node, ast.FunctionDef) and node.name == 'plot_implicit_curve' for node in current_axes.body):
    method = methods[0]
    assert not method.decorator_list
    fragment = ''.join(reference.splitlines(keepends=True)[method.lineno-1:method.end_lineno])
    lines = current.splitlines(keepends=True)
    current = ''.join(lines[:current_axes.end_lineno]) + '\n\n' + fragment + ''.join(lines[current_axes.end_lineno:])
    ast.parse(current)
    path.write_text(current)

# Ensure nearby native adapters retained their own cfg during reconstruction.
source = Path('crates/noon-web/src/lib.rs').read_text()
for module in ('authoring_image', 'authoring_number_plane', 'authoring_implicit_plotting'):
    assert re.search(r'#\[cfg\(target_arch = "wasm32"\)\]\s*mod ' + module + r';', source), module
package = Path('web/python-compat-modules.js').read_text()
for name in ('_manim_number_plane', '_manim_implicit'):
    assert package.count(f'sourcePath: "python/{name}.py"') == 1, name
print('Guarded NumberPlane/implicit repairs prepared.')
