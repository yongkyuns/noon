"""Pinned CE split-iterator capture, mutation and fused-exhaustion controls."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import warnings
import manim as m

POINTS = [[float(i), float(i % 2), 0.] for i in range(5)]
ACTIONS = ['none', 'remove_first', 'clear_children', 'append_extra', 'add_empty',
           'prepend_extra', 'insert_extra', 'reverse', 'set_points', 'clear_points']


def observe(spec):
    a, b, extra, post = m.Circle(.2), m.Square(.3), m.Circle(.4), m.Circle(.5)
    root = getattr(m, spec['class'])(a, b)
    root.set_points(POINTS[:spec['ownPoints']])
    names = {id(root): 'self', id(a): 'a', id(b): 'b', id(extra): 'extra', id(post): 'post'}
    iterator = iter(root)
    consumed = []
    for _ in range(spec['consumed']):
        try:
            consumed.append(names[id(next(iterator))])
        except StopIteration:
            break
    action = spec['action']
    if action == 'remove_first': root.remove(a)
    elif action == 'clear_children': root.remove(*root.submobjects)
    elif action == 'append_extra': root.add(extra)
    elif action == 'add_empty': root.add()
    elif action == 'prepend_extra': root.add_to_back(extra)
    elif action == 'insert_extra': root.insert(0, extra)
    elif action == 'reverse': root.invert()
    elif action == 'set_points': root.set_points(POINTS[:4])
    elif action == 'clear_points': root.set_points([])
    elif action != 'none': raise AssertionError(action)
    remaining = [names[id(value)] for value in iterator]
    after = {'points': root.get_num_points(), 'length': len(root),
             'split': [names[id(value)] for value in root.split()],
             'children': [names[id(value)] for value in root.submobjects]}
    root.insert(0, post)
    return {'spec': spec, 'consumed': consumed, 'remaining': remaining,
            'after': after, 'staysExhausted': next(iterator, None) is None}


def main():
    assert m.__version__ == '0.21.0'
    warnings.simplefilter('ignore', DeprecationWarning)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    cases = [observe({'class': kind, 'ownPoints': count, 'consumed': consumed, 'action': action})
             for kind in ['Group', 'VGroup'] for count in [0, 1, 4, 5]
             for consumed in [0, 1, 2, 3] for action in ACTIONS]
    assert len(cases) == 320 and all(row['staysExhausted'] for row in cases)
    def find(count, consumed, action):
        return next(row for row in cases if row['spec'] == {'class':'Group','ownPoints':count,'consumed':consumed,'action':action})
    assert find(0,0,'append_extra')['remaining'] == ['a','b']
    assert find(0,0,'remove_first')['remaining'] == ['b']
    assert find(4,0,'clear_points')['remaining'] == ['self','a','b']
    assert find(0,0,'set_points')['remaining'] == ['a','b']
    assert find(0,2,'insert_extra')['remaining'] == ['b']
    result = {'schemaVersion':1,'manimVersion':m.__version__,
              'scope':'Iterator prefix and member-list capture at creation; removal/insert/reorder in place versus add/prepend replacement, point edits, and fused exhaustion. Raw list writes, copy/become, callbacks and slicing qualify separately.',
              'generatorSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'methodSourceSha256':{name:hashlib.sha256(inspect.getsource(getattr(m.Mobject,name)).encode()).hexdigest()
                                    for name in ['__iter__','__len__','split','add','add_to_back','remove','insert','invert']},
              'cases':cases}
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print('Observed 320 pinned CE iterator controls; capture, in-place edit, replacement and exhaustion controls verified')


if __name__ == '__main__':
    main()
