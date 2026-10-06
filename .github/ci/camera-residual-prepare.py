"""Temporary #1653 missing-pass controls; never merge into the product."""
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

MODES = {'normal', 'skip-rectangle', 'skip-circle', 'skip-backend-present'}

def replace_one(text, old, new):
    if text.count(old) != 1:
        raise ValueError(f'expected exactly one diagnostic edit anchor: {old!r}')
    return text.replace(old, new)

def upgrade_probe(source):
    source = replace_one(source,
        "  if (/srgb_to_linear/.test(main)) return 'present';",
        "  if (/linear_to_srgb/.test(main)) return 'backend-present';\n"
        "  if (/srgb_to_linear/.test(main)) return 'present';")
    return replace_one(source,
        "const modes = ['normal', 'skip-line', 'skip-path', 'skip-text', 'skip-present', 'clip-all'];",
        "const modes = ['normal', 'skip-line', 'skip-path', 'skip-text', 'skip-present', 'clip-all', 'skip-rectangle', 'skip-circle', 'skip-backend-present'];")

def validate(tools):
    probe = upgrade_probe((tools / 'camera-draw-probe.js').read_text())
    tests = (tools / 'camera-draw-probe.test.mjs').read_text()
    tests = replace_one(tests,
        "present:'srgb_to_linear();'}[kind]",
        "present:'srgb_to_linear();',rectangle:'float half_size=1.;',circle:'float head_angle=1.;','backend-present':'linear_to_srgb();'}[kind]")
    tests = replace_one(tests,
        "for (const kind of ['line','path','text','present']) {",
        "for (const kind of ['line','path','text','present','rectangle','circle','backend-present']) {")
    tests += "\ntest('backend encode and Noon decode remain distinct', () => {\n"
    tests += "  assert.equal(classifyFragment('void main() {frag=linear_to_srgb(texture(present_texture,uv));}'), 'backend-present');\n"
    tests += "  assert.equal(classifyFragment('void main() {frag=vec4(srgb_to_linear(encoded.rgb),encoded.a);}'), 'present');\n});\n"
    with tempfile.TemporaryDirectory(prefix='noon-residual-probe-') as temporary:
        target = pathlib.Path(temporary)
        (target / 'package.json').write_text('{"type":"module"}\n')
        (target / 'camera-draw-probe.js').write_text(probe)
        (target / 'camera-draw-probe.test.mjs').write_text(tests)
        subprocess.run(['node', '--test', str(target / 'camera-draw-probe.test.mjs')], check=True)
    for invalid in ['', (tools / 'camera-draw-probe.js').read_text() * 2]:
        try:
            upgrade_probe(invalid)
        except ValueError:
            pass
        else:
            raise AssertionError('missing or duplicated edit anchor was accepted')

def prepare(root, site, evidence, tools, mode):
    if mode not in MODES:
        raise ValueError(f'unknown residual control: {mode}')
    # Reuse the existing exact-source/package verification and original harness.
    subprocess.run([sys.executable, str(tools / 'camera-draw-prepare.py'),
                    str(root), str(site), str(evidence), str(tools), 'normal'], check=True)
    probe = site / 'web/camera-draw-probe.js'
    probe.write_text(upgrade_probe(probe.read_text()))
    install = site / 'web/camera-draw-install.js'
    install.write_text('import {installDrawProbe} from "./camera-draw-probe.js";\n'
        + 'globalThis.__noonCameraDrawProbe=installDrawProbe(globalThis,' + json.dumps(mode) + ');\n')
    harness = root / 'scripts/camera-draw-run.mjs'
    harness.write_text(replace_one(harness.read_text(),
        "!['clip-all','skip-present'].includes(process.env.NOON_CAMERA_DRAW_MODE)",
        "!['clip-all','skip-present','skip-backend-present'].includes(process.env.NOON_CAMERA_DRAW_MODE)"))
    for module in [probe, install, harness]:
        subprocess.run(['node', '--check', str(module)], check=True)
    metadata = json.loads((evidence / 'overlay.json').read_text())
    metadata.update(mode=mode, classifierVersion=2, cohort='previously-unisolated-passes')
    for item in metadata['overrides']:
        item['diagnosticSha256'] = hashlib.sha256((site / 'web' / item['path']).read_bytes()).hexdigest()
    (evidence / 'overlay.json').write_text(json.dumps(metadata, indent=2))

if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--test':
        validate(pathlib.Path(sys.argv[2]))
    elif len(sys.argv) == 6:
        prepare(*map(pathlib.Path, sys.argv[1:5]), sys.argv[5])
    else:
        raise SystemExit('expected --test <tools>, or <source> <site> <evidence> <tools> <mode>')
