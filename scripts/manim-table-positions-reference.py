"""Observe Table position protocols in the actual pinned ManimCE product."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import sys

import manim as m


METHODS = ("get_cell", "get_highlighted_cell", "get_entries", "get_entries_without_labels")
EXPRESSIONS = (
    "(1, 1)", "(0, 0)", "(-1, -1)", "(0, 2)", "(-2, 1)", "(4, 1)",
    "(1, 1, 7)", "(1,)", "()", "(False, True)", "(1.0, 1)", "('1', 1)",
    "(1, '1')", "(value for value in (1, 1))", "(10**100, 1)", "[1, 2, 3]",
    "[1]", "[]", "(99,)", "Position([1, 1])", "Position([99])",
    "(Operand(1), Operand(2))", "(1, Operand(2))", "(IndexOnly(1), 1)",
    "(1, IndexOnly(1))", "ShiftPosition()",
)


def observe(method, labels, expression):
    cells = [[m.Square(side_length=0.4 + 0.1 * (2 * r + c)) for c in range(2)] for r in range(2)]
    row_labels = [m.Circle(0.2), m.Circle(0.3)]
    col_labels = [m.Square(0.2), m.Square(0.3)]
    names = {id(value): f"cell{r}{c}" for r, row in enumerate(cells) for c, value in enumerate(row)}
    names.update({id(value): f"row{i}" for i, value in enumerate(row_labels)})
    names.update({id(value): f"col{i}" for i, value in enumerate(col_labels)})
    table = m.MobjectTable(cells, **({"row_labels": row_labels, "col_labels": col_labels} if labels else {}))
    events = []

    class Position:
        def __init__(self, values):
            self.values = values

        def __iter__(self):
            raise AssertionError("position was unpacked")

        def __getitem__(self, index):
            events.append(["getitem", index])
            return self.values[index]

    class ShiftPosition(Position):
        def __init__(self):
            super().__init__([1, 1])

        def __getitem__(self, index):
            cells[0][0].shift(m.RIGHT * 0.25 if index == 0 else m.UP * 0.125)
            return super().__getitem__(index)

    class IndexOnly:
        def __init__(self, value):
            self.value = value

        def __index__(self):
            events.append(["index", self.value])
            return self.value

    class Operand(IndexOnly):
        def __sub__(self, other):
            events.append(["sub", other])
            return Operand(self.value - other)

        def __rmul__(self, other):
            events.append(["rmul", other])
            return Operand(other * self.value)

        def __add__(self, other):
            value = other.value if isinstance(other, Operand) else other
            events.append(["add", value])
            return Operand(self.value + value)

        def __radd__(self, other):
            events.append(["radd", other])
            return Operand(other + self.value)

    position = None if expression is None else eval(expression, {}, {
        "Position": Position, "ShiftPosition": ShiftPosition,
        "Operand": Operand, "IndexOnly": IndexOnly,
    })
    integer_position = None
    if type(position) in (list, tuple) and len(position) >= 2 and all(type(v) in (int, bool) for v in position[:2]):
        integer_position = [str(int(v)) for v in position[:2]]
    python_type = None
    try:
        result = getattr(table, method)() if expression is None else getattr(table, method)(position)
        python_type = type(result).__name__
        if method in ("get_cell", "get_highlighted_cell"):
            result = {"center": [float(v) for v in result.get_center()[:2]],
                      "width": float(result.width), "height": float(result.height)}
        else:
            result = names[id(result)]
    except Exception as error:
        kind = next((kind.__name__ for kind in (IndexError, TypeError, ValueError, RuntimeError, AssertionError)
                     if isinstance(error, kind)), type(error).__name__)
        result = {"exception": kind, "message": str(error)}
    return {"method": method, "labels": labels, "expression": expression,
            "integer_position": integer_position, "result": result, "events": events, "python_type": python_type}


def main():
    assert m.__version__ == "0.21.0", m.__version__
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    # This affects only fixture serialization. Product probes keep Python's
    # normal limit and cancel huge operands before converting the selected index.
    sys.set_int_max_str_digits(0)
    cases = []
    for labels in (False, True):
        for method in METHODS:
            for expression in EXPRESSIONS:
                cases.append(observe(method, labels, expression))
            if method in ("get_cell", "get_highlighted_cell"):
                cases.append(observe(method, labels, None))
            else:
                width = 3 if labels and method == "get_entries" else 2
                offset = 2 if labels and method == "get_entries" else 1
                for power in (100, 5000):
                    for sign in (1, -1):
                        cases.append(observe(method, labels,
                            f"({sign} * 10**{power} + 1, {offset} - {width} * {sign} * 10**{power})"))
    report = {
        "schemaVersion": 1, "manimVersion": m.__version__,
        "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "methodSourceSha256": {name: hashlib.sha256(inspect.getsource(getattr(m.Table, name)).encode()).hexdigest()
                               for name in METHODS},
        "scope": "Integer/sequence/operator/error/default position observations; geometric comparisons use a 1e-6 budget. Cell styling, wrapper types and full class compatibility remain separate requirements.",
        "cases": cases,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Observed {len(cases)} actual pinned table-position cases")


if __name__ == "__main__":
    main()
