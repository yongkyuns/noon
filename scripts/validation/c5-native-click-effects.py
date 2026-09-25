from pathlib import Path
import base64
import hashlib
import subprocess
import zlib

payload = ''.join(Path(f'scripts/validation/c5-native-effects-{i}.b64').read_text().strip() for i in range(3))
assert len(payload) == 11280
patch = zlib.decompress(base64.b64decode(payload, validate=True))
assert hashlib.sha256(patch).hexdigest() == '46a3c2aefcf523c34a6bddf036f2abf63d2d6cec943b53642d90164c2ff87e02'
proof = Path('/tmp/c5-native-click-proof')
proof.mkdir(exist_ok=True)
patch_path = proof / 'input.patch'
patch_path.write_bytes(patch)
subprocess.run(['git', 'apply', '--check', str(patch_path)], check=True)
subprocess.run(['git', 'apply', str(patch_path)], check=True)
paths = sorted(line.split(' b/', 1)[1] for line in patch.decode().splitlines() if line.startswith('diff --git '))
assert len(paths) == 10
assert all(path.startswith('crates/') or path == '.github/workflows/native-host-smoke.yml' for path in paths)
(proof / 'paths.txt').write_text('\n'.join(paths) + '\n')
