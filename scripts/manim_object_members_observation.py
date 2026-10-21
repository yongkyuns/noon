"""Identical public observations for pinned CE and real Noon authoring.

This module observes external API behavior; it implements no scene semantics.
"""
import numpy as np

POINTS = [[0., 0., 0.], [.1, 0., 0.], [.2, .3, 0.], [.3, .4, 0.]]


def observe_object_membership(m, spec):
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
