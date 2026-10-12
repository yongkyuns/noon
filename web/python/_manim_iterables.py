"""ManimCE v0.21 Python iterable ergonomics for user data."""
from collections.abc import Iterable
from itertools import cycle


def adjacent_n_tuples(objects, n):
    return zip(*([*objects[k:], *objects[:k]] for k in range(n)), strict=True)


def adjacent_pairs(objects):
    return adjacent_n_tuples(objects, 2)


def all_elements_are_instances(iterable, Class):
    return all(isinstance(element, Class) for element in iterable)


def concatenate_lists(*list_of_lists):
    return [item for items in list_of_lists for item in items]


def list_difference_update(l1, l2):
    return [element for element in l1 if element not in l2]


def list_update(l1, l2):
    return list_difference_update(l1, l2) + list(l2)


def listify(obj):
    if isinstance(obj, str):
        return [obj]
    return list(obj) if isinstance(obj, Iterable) else [obj]


def tuplify(obj):
    if isinstance(obj, str):
        return (obj,)
    return tuple(obj) if isinstance(obj, Iterable) else (obj,)


def make_even(iterable_1, iterable_2):
    first, second = list(iterable_1), list(iterable_2)
    length = max(len(first), len(second))
    return ([first[(index * len(first)) // length] for index in range(length)],
            [second[(index * len(second)) // length] for index in range(length)])


def make_even_by_cycling(iterable_1, iterable_2):
    length = max(len(iterable_1), len(iterable_2))
    first, second = cycle(iterable_1), cycle(iterable_2)
    return ([next(first) for _ in range(length)], [next(second) for _ in range(length)])


def remove_list_redundancies(lst):
    result, used = [], set()
    for value in reversed(lst):
        if value not in used:
            result.append(value)
            used.add(value)
    result.reverse()
    return result


def remove_nones(sequence):
    """Preserve the pinned function's removal of all falsey values."""
    return [value for value in sequence if value]
