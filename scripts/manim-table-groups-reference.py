"""Observe canonical Table query groups in pinned ManimCE, without rendering."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path

import manim as m


def observe_groups(labels):
    cells = [[m.Square(0.4), m.Square(0.5)], [m.Square(0.6), m.Square(0.7)]]
    row_labels = [m.Circle(0.2), m.Circle(0.3)]
    col_labels = [m.Square(0.2), m.Square(0.3)]
    names = {id(value): f"cell{r}{c}" for r, row in enumerate(cells) for c, value in enumerate(row)}
    names.update({id(value): f"row{i}" for i, value in enumerate(row_labels)})
    names.update({id(value): f"col{i}" for i, value in enumerate(col_labels)})
    table = m.MobjectTable(cells, **({"row_labels": row_labels, "col_labels": col_labels} if labels else {}))
    displayed, body = table.get_entries(), table.get_entries_without_labels()

    def name(value):
        # The unpainted missing corner is retained in raw rows/columns.
        return names.get(id(value), "missing-corner")

    def snapshot():
        selections = []
        for method in ("get_entries", "get_entries_without_labels"):
            results = []
            for position in ((1, 1), (1, 2), (2, 2), (0, 0)):
                try:
                    results.append(name(getattr(table, method)(position)))
                except IndexError as error:
                    results.append({"exception": "IndexError", "message": str(error)})
            selections.append(results)
        return {
            "displayedStable": table.get_entries() is displayed,
            "bodyStable": table.get_entries_without_labels() is body,
            "distinctGroups": displayed is not body,
            "displayed": [name(value) for value in table.get_entries()],
            "body": [name(value) for value in table.get_entries_without_labels()],
            "rows": [[name(value) for value in row] for row in table.get_rows()],
            "columns": [[name(value) for value in column] for column in table.get_columns()],
            "selections": selections,
        }

    states = [snapshot()]
    displayed.remove(cells[0][0])
    states.append(snapshot())
    body.remove(cells[0][1])
    states.append(snapshot())
    displayed.add(cells[0][0])
    states.append(snapshot())
    body.add(cells[0][1])
    states.append(snapshot())
    displayed.add(cells[0][1])
    states.append(snapshot())
    clone = table.copy()
    copied_displayed, copied_body = clone.get_entries(), clone.get_entries_without_labels()
    copied_displayed.remove(copied_displayed[0])
    copied_body.remove(copied_body[-1])
    copied = {
        "displayedStable": copied_displayed is clone.get_entries(),
        "bodyStable": copied_body is clone.get_entries_without_labels(),
        "independentGroups": copied_displayed is not displayed and copied_body is not body,
        "independentLeaves": all(value is not original for value in copied_displayed for original in displayed),
        "displayedCount": len(clone.get_entries()),
        "bodyCount": len(clone.get_entries_without_labels()),
        "originalUnchanged": snapshot() == states[-1],
    }
    return {"labels": labels, "states": states, "copy": copied,
            "pythonTypes": {"displayed": type(displayed).__name__, "body": type(body).__name__}}


def main():
    assert m.__version__ == "0.21.0", m.__version__
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report = {
        "schemaVersion": 1, "manimVersion": m.__version__,
        "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "methodSourceSha256": {name: hashlib.sha256(inspect.getsource(getattr(m.Table, name)).encode()).hexdigest()
                               for name in ("get_entries", "get_entries_without_labels")},
        "scope": "Canonical group identity, independent displayed/body membership, ordering, raw rows/columns, position selection and copies.",
        "cases": [observe_groups(labels) for labels in (False, True)],
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
