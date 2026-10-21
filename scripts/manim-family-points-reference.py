"""Pinned CE observations for subsequent shared family-content qualification."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import warnings
import manim as m

POINTS = [[0.0, 0.0, 0.0], [0.1, 0.0, 0.0], [0.2, 0.3, 0.0], [0.3, 0.4, 0.0], [0.5, 0.6, 0.0]]


def observe(spec):
    a, b, extra = m.Circle(0.2), m.Square(0.3), m.Square(0.4)
    root = getattr(m, spec['class'])(a, b)
    root.set_points(POINTS[:spec['ownPoints']])
    names = {id(root): 'self', id(a): 'a', id(b): 'b', id(extra): 'extra'}
    def snapshot():
        return {'points': root.get_num_points(), 'length': len(root),
                'iteration': [names[id(value)] for value in root],
                'split': [names[id(value)] for value in root.split()],
                'children': [names[id(value)] for value in root.submobjects]}
    before = snapshot()
    events = []
    class Index:
        def __index__(self):
            events.append({'length': len(root), 'points': root.get_num_points()})
            action = spec['action']
            if action == 'set_points':
                root.set_points(POINTS[:4])
            elif action == 'clear_points':
                root.set_points([])
            elif action == 'remove_first':
                root.remove(a)
            elif action == 'clear_children':
                root.remove(*root.submobjects)
            elif action == 'append_extra':
                root.add(extra)
            elif action != 'none':
                raise AssertionError(action)
            return spec['index']
    try:
        selected = names[id(root[Index()])]
    except Exception as error:
        selected = {'exception': type(error).__name__, 'message': str(error)}
    return {'spec': spec, 'before': before, 'result': selected, 'events': events, 'after': snapshot()}


def main():
    assert m.__version__ == '0.21.0'
    warnings.simplefilter('ignore', DeprecationWarning)
    cases = [observe({'class': kind, 'ownPoints': count, 'action': action, 'index': index})
             for kind in ('Group', 'VGroup') for count in (0, 1, 4, 5)
             for action in ('none', 'set_points', 'clear_points', 'remove_first', 'clear_children', 'append_extra')
             for index in (0, 1, 2, -1, -3, 3, 10)]
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output',type=Path)
    args=parser.parse_args()
    result = {'schemaVersion':1,'manimVersion': m.__version__, 'scope': 'Own-point counts, self inclusion and current scalar selection after index-protocol edits. Slicing, iterators under edits, class capability, and rendering are separate qualification requirements.',
              'generatorSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'methodSourceSha256': {name: hashlib.sha256(inspect.getsource(getattr(m.Mobject, name)).encode()).hexdigest()
                                     for name in ('__getitem__', '__len__', '__iter__', 'split', 'add', 'remove', '__getattr__')},
              'cases': cases}
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    assert len(cases) == 336
    # Negative indices normalize against the pre-callback length, whereas the
    # own-point branch and current member rank are read after the callback.
    control = next(row for row in cases if row['spec'] == {'class':'Group','ownPoints':4,'action':'clear_points','index':-1})
    assert control['before']['length'] == 3 and control['after']['length'] == 2
    assert control['result'] == {'exception':'IndexError','message':'list index out of range'}
    control = next(row for row in cases if row['spec'] == {'class':'VGroup','ownPoints':0,'action':'set_points','index':0})
    assert control['result'] == 'self'
    print(f'Observed {len(cases)} pinned CE own-point/index-protocol cases; mutation controls verified')


if __name__ == '__main__':
    main()
