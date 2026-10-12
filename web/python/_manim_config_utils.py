"""Python dictionary ergonomics matching ManimCE v0.21 config helpers.

These helpers operate on user dictionaries; they do not own engine policy.
"""
from itertools import chain
from typing import Any


def merge_dicts_recursively(*dicts: dict[Any, Any]) -> dict[Any, Any]:
    """Merge dictionaries, giving later inputs priority at each nested key."""
    result = {}
    for key, value in chain(*(d.items() for d in dicts)):
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = merge_dicts_recursively(result[key], value)
        else:
            result[key] = value
    return result


def update_dict_recursively(current_dict: dict[Any, Any], *others: dict[Any, Any]) -> None:
    """Update the original outer dictionary using the recursive merge rules."""
    current_dict.update(merge_dicts_recursively(current_dict, *others))


class DictAsObject:
    """Expose the original dictionary through ordinary Python attributes."""
    def __init__(self, dictin: dict[str, Any]):
        self.__dict__ = dictin
