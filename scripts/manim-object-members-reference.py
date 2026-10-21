"""Pinned ordinary-object child editing and alias-preserving copy controls."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import manim as m
import numpy as np

POINTS = [[0., 0., 0.], [.1, 0., 0.], [.2, .3, 0.], [.3, .4, 0.]]


def observe(spec):
    root = getattr(m, spec['class'])()
    root.apply_over_attr_arrays(lambda _: np.asarray(POINTS[:spec['ownPoints']], dtype=float).reshape((-1, 3)))
    a, b, c, d = m.Circle(.1), m.Square(.2), m.Circle(.3), m.Square(.4)
    names = {id(root): 'self', id(a): 'a', id(b): 'b', id(c): 'c', id(d): 'd'}
    def labels(values): return [names[id(value)] for value in values]
    def snapshot():
        return {'points': root.get_num_points(), 'length': len(root), 'members': labels(root.submobjects), 'split': labels(root.split()), 'iteration': labels(root)}
    steps = []
    def record(operation, result, iterator=None):
        row = {'operation': operation, 'returnsSelf': result is root, **snapshot()}
        if iterator is not None: row['capturedIterator'] = labels(iterator)
        steps.append(row)
    record('add_repeated', root.add(a, b, a))
    captured = iter(root)
    record('add_replaces_list', root.add(c), captured)
    captured = iter(root)
    record('remove_edits_list', root.remove(a, d), captured)
    captured = iter(root)
    record('prepend_repeated', root.add_to_back(c, b, c), captured)
    captured = iter(root)
    record('prepend_same_order', root.add_to_back(c, b), captured)
    record('insert', root.insert(spec['index'], d))
    captured = iter(root)
    root.submobjects = [a, c]
    record('replace', root, captured)
    root.aliases = (a, c, a)
    copied = root.copy()
    source_center = np.asarray(a.get_center()).copy()
    copied.submobjects[0].shift(m.RIGHT)
    copy = {'type': type(copied).__name__, 'independentRoot': copied is not root,
            'independentChildren': all(x is not y for x, y in zip(copied.submobjects, root.submobjects)),
            'aliasesPreserved': copied.aliases[0] is copied.aliases[2] is copied.submobjects[0] and copied.aliases[1] is copied.submobjects[1],
            'sourceUnchanged': bool(np.array_equal(np.asarray(a.get_center()), source_center))}
    errors = []
    for value in [root, 3]:
        try: root.add(value)
        except Exception as error: errors.append({'type': type(error).__name__, 'message': str(error)})
        else: errors.append(None)
    return {'spec': spec, 'steps': steps, 'copy': copy, 'errors': errors, 'after': snapshot()}


def main():
    assert m.__version__ == '0.21.0'
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    cases = [observe({'class': kind, 'ownPoints': count, 'index': index})
             for kind in ['Mobject', 'VMobject', 'Circle', 'Square', 'VectorizedPoint']
             for count in [0, 4] for index in [-100, -1, 0, 1, 100]]
    assert len(cases) == 50
    result = {'schemaVersion': 1, 'manimVersion': m.__version__,
              'scope': 'Ordinary-object add/remove/prepend, unique insertion, container replacement and alias-preserving copies. Raw list aliasing/mutation, duplicate direct occurrences and arbitrary equality/copy hooks remain separate requirements.',
              'generatorSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'methodSourceSha256': {name: hashlib.sha256(inspect.getsource(getattr(m.Mobject, name)).encode()).hexdigest() for name in ['add', 'remove', 'add_to_back', 'insert', 'copy', '_assert_valid_submobjects_internal']},
              'cases': cases}
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print('Observed 50 pinned ordinary-object membership/copy sequences')


if __name__ == '__main__': main()
