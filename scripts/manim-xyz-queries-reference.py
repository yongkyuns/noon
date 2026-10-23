#!/usr/bin/env python3
"""Pin actual ManimCE query results before admission, after edit and after removal."""
import hashlib
import inspect
import json
from pathlib import Path
import sys

import manim as m
import numpy as np
import manim_xyz_query_observation as observation

ROOT = Path(__file__).resolve().parents[1]
PHASES = ('authored', 'bound', 'edited', 'removed')


def capture():
    assert m.__version__ == '0.21.0'
    cases = []
    # These controls inspect object coordinates, with no camera-dependent query
    # or frame rendering. Keep unused camera allocation small and outside media/.
    with m.tempconfig({'pixel_width': 16, 'pixel_height': 16,
                       'media_dir': '/tmp/noon-manim-xyz-reference-media',
                       'write_to_movie': False, 'save_last_frame': False}):
        for spec in observation.specs():
            root = observation.prepare_root(m, spec)
            phases = [dict(phase='authored', value=observation.collect_queries(root, spec))]
            scene = m.Scene()
            scene.add(root)
            scene.update_mobjects(.001)
            phases.append(dict(phase='bound', value=observation.collect_queries(root, spec)))
            root.shift(np.array([.5, .25, 1.125]))
            phases.append(dict(phase='edited', value=observation.collect_queries(root, spec)))
            scene.remove(root)
            scene.update_mobjects(.001)
            phases.append(dict(phase='removed', value=observation.collect_queries(root, spec)))
            cases.append(dict(spec=spec, phases=phases))
    sha = lambda text: hashlib.sha256(text.encode()).hexdigest()
    methods = ('get_all_points', 'get_points_defining_boundary', 'get_critical_point',
               'get_extremum_along_dim', 'get_center', 'get_zenith', 'get_nadir',
               'get_coord', 'get_z', 'reduce_across_dimension', 'shift')
    return dict(schemaVersion=1, manimVersion=m.__version__, phases=list(PHASES),
                generatorSha256=sha(Path(__file__).read_text()),
                observerSha256=sha(Path(observation.__file__).read_text()),
                helperSha256=sha((ROOT / 'scripts/manim_depth_affine_observation.py').read_text()),
                methodSourceSha256={f'{cls.__name__}.{name}': sha(inspect.getsource(getattr(cls, name)))
                                    for cls in (m.Mobject, m.VMobject, m.Scene)
                                    for name in (('add', 'remove', 'update_mobjects')
                                                 if cls is m.Scene else methods)},
                cases=cases)


if __name__ == '__main__':
    output = Path(sys.argv[1])
    result = capture()
    with output.open('x') as file:
        json.dump(result, file, indent=2)
        file.write('\n')
    print(json.dumps(dict(profiles=len(result['cases']), phases=160, queryObservations=6080)))
