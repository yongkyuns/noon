"""Extract the pinned CE pure functions for same-platform NumPy comparisons."""
import argparse
import hashlib
import importlib
import importlib.metadata
import inspect
import json
from pathlib import Path
import textwrap

import manim as m


def main():
    assert m.__version__ == "0.21.0"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    bezier = importlib.import_module("manim.utils.bezier")
    functions = {name: textwrap.dedent(inspect.getsource(getattr(m.VMobject, name)))
                 for name in ("insert_n_curves_to_point_list",
                              "get_cubic_bezier_tuples_from_points",
                              "gen_cubic_bezier_tuples_from_points")}
    pure_names = ("bezier_remap", "subdivide_bezier", "_get_subdivision_matrix")
    functions.update({name: textwrap.dedent(inspect.getsource(getattr(bezier, name)))
                      for name in pure_names})
    tables = [{str(key): {"dtype": str(value.dtype), "values": value.tolist()}
               for key, value in table.items()} for table in bezier.SUBDIVISION_MATRICES]
    raw_method = inspect.getsource(m.VMobject.insert_n_curves_to_point_list)
    distribution = importlib.metadata.distribution("manim")
    licenses = {Path(file).name: distribution.locate_file(file).read_text()
                for file in distribution.files
                if str(file).endswith(("/LICENSE", "/LICENSE.community"))}
    record = {
        "rawMethodSha256": hashlib.sha256(raw_method.encode()).hexdigest(),
        "subdivisionTables": tables,
        "names": {name: list(getattr(bezier, name).__code__.co_names) for name in pure_names},
        "manimVersion": m.__version__, "sources": functions,
        "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "licenses": licenses,
        "sha256": {name: hashlib.sha256(source.encode()).hexdigest()
                   for name, source in functions.items()},
        "scope": "Exact pinned CE pure method/query/subdivision code for same-runtime platform witness; no CE renderer or alternate Noon engine.",
    }
    args.output.write_text(json.dumps(record, indent=2) + "\n")


if __name__ == "__main__":
    main()
