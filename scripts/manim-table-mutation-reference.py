"""Observe membership edits during indexing in actual pinned ManimCE."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path

import manim as m


def observe_mutation(spec):
    labels, method = spec["labels"], spec["method"]
    cells = [[m.Square(0.4), m.Square(0.5)], [m.Square(0.6), m.Square(0.7)]]
    rows, columns = [m.Circle(0.2), m.Circle(0.3)], [m.Square(0.2), m.Square(0.3)]
    extra = m.Square(0.25)
    names = {id(value): f"cell{r}{c}" for r, row in enumerate(cells) for c, value in enumerate(row)}
    names.update({id(value): f"row{i}" for i, value in enumerate(rows)})
    names.update({id(value): f"col{i}" for i, value in enumerate(columns)})
    names[id(extra)] = "extra"
    table = m.MobjectTable(cells, **({"row_labels": rows, "col_labels": columns} if labels else {}))
    group = getattr(table, method)()
    if spec["access"] == "group":
        group = m.Group(*list(group))
    before = [names[id(value)] for value in group]
    events, normalization = [], {}

    def emit(phase, value):
        events.append([phase, value])
        if phase != spec["phase"]:
            return
        action = spec["action"]
        if action == "remove_first":
            group.remove(group[0])
        elif action == "clear":
            group.remove(*list(group))
        elif action == "append_first":
            group.add(group[0])
        elif action == "append_extra":
            group.add(extra)
        else:
            raise AssertionError(action)

    class Operand:
        def __init__(self, value, phase):
            self.value, self.phase = value, phase

        def __sub__(self, other):
            emit(self.phase, other)
            return Operand(self.value - other, self.phase)

        def __rmul__(self, other):
            emit("mul", other)
            return Operand(other * self.value, "product")

        def __add__(self, other):
            emit("add", other.value)
            return Operand(self.value + other.value, "sub_flat")

        def __index__(self):
            normalization.update({"length": len(group), "index": self.value})
            emit("index", self.value)
            return self.value

    class Position:
        def __getitem__(self, index):
            emit("row" if index == 0 else "column", index)
            return Operand(spec["position"][index], "sub_row" if index == 0 else "unused")

    try:
        position = Position()
        if spec["access"] == "table":
            value = getattr(table, method)(position)
        else:
            stride = 3 if labels and method == "get_entries" else 2
            offset = 2 if labels and method == "get_entries" else 1
            value = group[stride * (position[0] - 1) + position[1] - offset]
        result = names.get(id(value), {"unknownWrapperType": type(value).__name__})
    except Exception as error:
        kind = next((kind.__name__ for kind in (IndexError, TypeError, ValueError, RuntimeError)
                     if isinstance(error, kind)), type(error).__name__)
        result = {"exception": kind, "message": str(error)}
    return {"spec": spec, "before": before, "result": result, "events": events,
            "normalization": normalization, "after": [names[id(value)] for value in group],
            "displayed_after": [names[id(value)] for value in table.get_entries()],
            "body_after": [names[id(value)] for value in table.get_entries_without_labels()]}


def main():
    assert m.__version__ == "0.21.0", m.__version__
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    cases = []
    for labels in (False, True):
        for method in ("get_entries", "get_entries_without_labels"):
            for access in ("table", "view", "group"):
                phases = ("row", "sub_row", "mul", "column", "add", "sub_flat", "index") if access == "table" else ("index",)
                for phase in phases:
                    for action in ("remove_first", "clear", "append_first", "append_extra"):
                        first_new_row = 4 if labels and method == "get_entries" else 3
                        for position in ([1, 1], [2, 2], [0, 0], [first_new_row, 1]):
                            cases.append(observe_mutation({"labels": labels, "method": method,
                                "access": access, "phase": phase, "action": action, "position": position}))
    report = {"schemaVersion": 1, "manimVersion": m.__version__,
              "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "methodSourceSha256": {f"{kind.__name__}.{name}": hashlib.sha256(inspect.getsource(getattr(kind, name)).encode()).hexdigest()
                  for kind, name in ((m.Table, "get_entries"), (m.Table, "get_entries_without_labels"), (m.Mobject, "__getitem__"))},
              "scope": "Membership edits during position arithmetic and __index__, including current member reads after normalization, independent query views and wrapper identity. Groups owning points and slicing remain separate requirements.",
              "cases": cases}
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Observed {len(cases)} pinned mutation-position cases")


if __name__ == "__main__":
    main()
