from pathlib import Path
import sys


def replace_once(path, old, new):
    path = Path(path)
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f'{path}: expected one anchor, got {text.count(old)}: {old[:100]!r}')
    path.write_text(text.replace(old, new, 1))


if sys.argv[1] == '--regressions':
    replace_once(
        'crates/noon/tests/foreground_admission.rs',
        'noon::AffineLifecycleEndpoint::EffectiveCenter',
        'noon::AffineLifecycleEndpoint::Point { x: -1.0, y: 0.0, rotation_offset: 0.0, point_color: None }',
    )
elif sys.argv[1] == '--production':
    path = 'crates/noon/src/execution_session.rs'
    replace_once(
        path,
        '                family_replacement.map(|(root, _, _)| root),',
        '''                // Admissions can reorder the root even without a matching
                // family transform. Use the lifecycle's already validated scope;
                // the unrooted lowering guard remains unchanged.
                family_replacement
                    .map(|(root, _, _)| root)
                    .or_else(|| lifecycle.as_ref().map(PreparedAnimationLifecycle::root)),''',
    )
    replace_once(
        path,
        '#[derive(Clone)]\n/// Request-local order and duplicate detection; the core planner owns membership.\n#[derive(Default)]',
        '/// Request-local order and duplicate detection; the core planner owns membership.\n#[derive(Default)]',
    )
    replace_once(path, '\nenum PreparedAnimationLifecycle {', '\n#[derive(Clone)]\nenum PreparedAnimationLifecycle {')
    p = Path(path)
    text = p.read_text()
    for indent, condition, target in [
        (' ' * 16, 'admitted_target', '(*target).into()'),
        (' ' * 8, 'introducer', 'target.into()'),
    ]:
        old = indent + 'if ' + condition + ' {\n' + indent + '    if !admitted.insert(' + target + ') {\n'
        assert text.count(old) == 1
        start = text.index(old)
        end = text.index('\n' + indent + '}', start) + len('\n' + indent + '}')
        lines = text[start:end].splitlines()
        assert lines[-2:] == [indent + '    }', indent + '}']
        new = indent + 'if ' + condition + ' && !admitted.insert(' + target + ') {\n'
        new += '\n'.join(line[4:] for line in lines[2:-2]) + '\n' + indent + '}'
        text = text[:start] + new + text[end:]
    p.write_text(text)
else:
    raise SystemExit('expected --regressions or --production')
