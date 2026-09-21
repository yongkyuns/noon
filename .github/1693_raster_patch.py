from pathlib import Path
import hashlib
import json

fixtures = [
    {
        'id': 'implicit-curves',
        'scene': 'RetainedImplicitCurves',
        'source': 'parity/manim-v0.21/core-examples/implicit_curves.py',
        'expected_duration': 0.2,
        'qualification_status': 'candidate',
        'notes': 'Source-equivalent adaptive contours: smooth closed circle and disconnected unsmoothed hyperbola through shifted nonuniform Axes.',
        'raster_tolerance': {
            'max_bounds_delta_px': 2,
            'max_differing_ratio': 0.006,
            'max_mean_absolute_channel_error': 0.5,
        },
    },
    {
        'id': 'number-plane-grid',
        'scene': 'NumberPlaneGrid',
        'source': 'parity/manim-v0.21/core-examples/number_plane.py',
        'expected_duration': 0.2,
        'qualification_status': 'candidate',
        'notes': 'Ordinary retained grid lines: zero/three subdivisions, asymmetric ranges, explicit lengths/styles, scaling and painter order.',
        'raster_tolerance': {
            'max_bounds_delta_px': 2,
            'max_differing_ratio': 0.006,
            'max_mean_absolute_channel_error': 0.5,
        },
    },
]
expected_sources = {
    fixtures[0]['source']: '5f3f659c65fbd868941011fb393ffe5cadebede6',
    fixtures[1]['source']: '8b697295d5ede2de48e568a3f93441645f494370',
}
for source, expected in expected_sources.items():
    data = Path(source).read_bytes()
    actual = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    assert actual == expected, (source, actual)
p = Path('parity/manim-v0.21/manifest.json')
s = p.read_text()
original = json.loads(s)
ids = {f['id'] for f in original['fixtures']}
assert not ids.intersection(f['id'] for f in fixtures), 'fixture already registered; review instead of duplicating'
anchor = '  "fixtures": [\n'
assert s.count(anchor) == 1
addition = ''.join('\n'.join('    ' + line for line in json.dumps(f, indent=2).splitlines()) + ',\n' for f in fixtures)
p.write_text(s.replace(anchor, anchor + addition, 1))
registered = json.loads(p.read_text())
assert registered['fixtures'][2:] == original['fixtures']
assert {k: v for k, v in registered.items() if k != 'fixtures'} == {k: v for k, v in original.items() if k != 'fixtures'}
focused = dict(registered)
focused['fixtures'] = fixtures
Path('/tmp/noon-1693-focused-manifest.json').write_text(json.dumps(focused, indent=2) + '\n')
print(f'Registered two previously omitted fixtures; preserved all {len(original["fixtures"])} existing fixtures and policies')
