"""Pinned ordinary-object child editing and alias-preserving copy controls."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import manim as m
from manim_object_members_observation import observe_object_membership


def main():
    assert m.__version__ == '0.21.0'
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    cases = [observe_object_membership(m, {'class': kind, 'ownPoints': count, 'index': index})
             for kind in ['Mobject', 'VMobject', 'Circle', 'Square', 'VectorizedPoint']
             for count in [0, 4] for index in [-100, -1, 0, 1, 100]]
    assert len(cases) == 50
    result = {'schemaVersion': 1, 'manimVersion': m.__version__,
              'scope': 'Ordinary-object add/remove/prepend, unique insertion, container replacement and alias-preserving copies. Raw list aliasing/mutation, duplicate direct occurrences and arbitrary equality/copy hooks remain separate requirements.',
              'generatorSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'observerSourceSha256': hashlib.sha256(Path(__file__).with_name('manim_object_members_observation.py').read_bytes()).hexdigest(),
              'methodSourceSha256': {name: hashlib.sha256(inspect.getsource(getattr(m.Mobject, name)).encode()).hexdigest() for name in ['add', 'remove', 'add_to_back', 'insert', 'copy', '_assert_valid_submobjects_internal']},
              'cases': cases}
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print('Observed 50 pinned ordinary-object membership/copy sequences')


if __name__ == '__main__': main()
