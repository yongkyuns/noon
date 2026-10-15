"""ManimCE v0.21 Python iterable ergonomics for user data."""
from __future__ import annotations

from collections.abc import Callable, Generator, Iterable
from itertools import chain, cycle
from typing import TypeVar

T = TypeVar("T")
U = TypeVar("U")


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


def batch_by_property(
    items: Iterable[T], property_func: Callable[[T], U]
) -> list[tuple[list[T], U | None]]:
    """Group adjacent user values by their callback result, retaining order."""
    result, batch, current = [], [], None
    for item in items:
        value = property_func(item)
        if value != current:
            if batch:
                result.append((batch, current))
            current, batch = value, [item]
        else:
            batch.append(item)
    if batch:
        result.append((batch, current))
    return result


def uniq_chain(*args: Iterable[T]) -> Generator[T, None, None]:
    """Lazily yield each hashable user value once in first-seen order."""
    seen = set()
    for value in chain(*args):
        if value not in seen:
            seen.add(value)
            yield value


def hash_obj(obj: object) -> int:
    """Hash nested user containers through their own hash protocols."""
    if isinstance(obj, dict):
        return hash(tuple(sorted((hash_obj(key), hash_obj(value)) for key, value in obj.items())))
    if isinstance(obj, set):
        return hash(tuple(sorted(hash_obj(value) for value in obj)))
    if isinstance(obj, (tuple, list)):
        return hash(tuple(hash_obj(value) for value in obj))
    return hash(obj)


def resize_array(nparray, length):
    import numpy as np
    if len(nparray) == length:
        return nparray
    return np.resize(nparray, (length, *nparray.shape[1:]))


def resize_preserving_order(nparray, length):
    import numpy as np
    if len(nparray) == 0:
        return np.zeros((length, *nparray.shape[1:]))
    if len(nparray) == length:
        return nparray
    return nparray[np.arange(length) * len(nparray) // length]


def resize_with_interpolation(nparray, length):
    import numpy as np
    if len(nparray) == length:
        return nparray
    return np.array([
        (1 - alpha) * nparray[left] + alpha * nparray[right]
        for index in np.linspace(0, len(nparray) - 1, length)
        for left, right, alpha in [(int(index), int(np.ceil(index)), index % 1)]
    ])


def stretch_array_to_length(nparray, length):
    import numpy as np
    current = len(nparray)
    if current > length:
        raise Warning("Trying to stretch array to a length shorter than its own")
    indices = np.arange(length) / float(length)
    indices *= current
    return nparray[indices.astype(int)]
