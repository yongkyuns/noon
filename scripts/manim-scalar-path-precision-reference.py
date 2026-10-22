"""Generate pinned public scalar/corner precision observations."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path

import manim as m
from manim_scalar_path_observation import SCALAR_PATH_PRECISION_METHODS, observe_scalar_path_precision


def main():
    assert m.__version__ == '0.21.0'
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    rows = [observe_scalar_path_precision(m, name) for name in SCALAR_PATH_PRECISION_METHODS]
    assert len(rows) == 10 and all(row['returnsSelf'] for row in rows)
    result = {
        'manimVersion': m.__version__,
        'generatorSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'observerSha256': hashlib.sha256(Path(__file__).with_name('manim_scalar_path_observation.py').read_bytes()).hexdigest(),
        'methodSourceSha256': {name: hashlib.sha256(inspect.getsource(getattr(m.VMobject, name)).encode()).hexdigest()
                              for name in SCALAR_PATH_PRECISION_METHODS},
        'cases': rows,
        'scope': 'Pinned scalar/corner edit precision requirements; no Noon implementation/support claim.',
    }
    args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
