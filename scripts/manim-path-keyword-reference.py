"""Generate pinned public path keyword admission and planar-value controls."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path

import manim as m
from manim_path_keyword_observation import PATH_KEYWORD_METHODS, observe_path_keyword


def main():
    assert m.__version__ == "0.21.0"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    cases = [observe_path_keyword(m, method) for method in PATH_KEYWORD_METHODS]
    assert all(case["returnsSelf"] for case in cases)
    methods = {name: getattr(m.Mobject if name == "move_to" else m.VMobject, name)
               for name in PATH_KEYWORD_METHODS}
    result = {"schemaVersion": 1, "manimVersion": m.__version__,
              "scope": "Five valid keyword calls, self returns and public planar coordinate values. Full signatures/options, invalid inputs and NumPy buffer ownership remain separate requirements.",
              "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "observerSourceSha256": hashlib.sha256(Path(__file__).with_name("manim_path_keyword_observation.py").read_bytes()).hexdigest(),
              "methodSourceSha256": {name: hashlib.sha256(inspect.getsource(method).encode()).hexdigest()
                                     for name, method in methods.items()},
              "cases": cases}
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print("Observed all five pinned CE path keyword calls")


if __name__ == "__main__":
    main()
