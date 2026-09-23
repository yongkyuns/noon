"""Apply the worker-only delta after formatting the pinned direct foundation."""
from pathlib import Path
import base64
import gzip
import hashlib
import subprocess

EXPECTED = 'c9f551f121284f3f884055c6f55a82b028fa2bccb61d592903960fc6ef4b8443'
ORACLE_COMMIT = '1ee487af88dc6972bb4b734b4532413861f8bd46'
s = Path('scripts/validation/c5-worker-inspection.patch.gz.b64').read_text().strip()
# Correct one known transport transcription, not source code. Gzip integrity
# and the complete original patch digest must both hold before applying it.
old, new = 'KHmaRD8jlnaWW1x', 'KHmaRD8jlnaW1x'
assert s.count(old) == 1, (old, s.count(old))
s = s.replace(old, new)
patch = gzip.decompress(base64.b64decode(s, validate=True))
actual = hashlib.sha256(patch).hexdigest()
assert actual == EXPECTED, (actual, EXPECTED)
proof = Path('/tmp/c5-worker-proof')
proof.mkdir(exist_ok=True)
p = proof / 'worker-input.patch'
p.write_bytes(patch)
(proof / 'worker-input-sha256.txt').write_text(actual + '\n')
subprocess.run(['git', 'apply', '--check', str(p)], check=True)
subprocess.run(['git', 'apply', str(p)], check=True)
# The same authored pixel oracle is used by direct and worker qualification.
# Fetch all refs at checkout, then execute this exact locally-created revision.
oracle = subprocess.check_output(['git', 'show', ORACLE_COMMIT + ':scripts/validation/c5-inspection-raster-oracle.py'])
(proof / 'oracle.py').write_bytes(oracle)
subprocess.run(['python3', '-'], input=oracle, check=True)
print('Applied exact worker inspection delta:', actual)
