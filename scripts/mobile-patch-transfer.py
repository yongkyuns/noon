# Temporary connector transport repair; never included in the source-only PR.
import base64
import gzip
import hashlib
from pathlib import Path
p = Path('mobile-repair.patch.gz')
raw = p.read_bytes()
assert hashlib.sha256(raw).hexdigest() == '1c5cd07e9f1c5cd57075d226cb24eae6a23c9a215d1388d94e8c96ed22e11000'
encoded = base64.b64encode(raw).decode()
for start, end, replacement in reversed([(5162, 5163, ''), (7607, 7608, 'y'), (15876, 15876, '=')]):
    encoded = encoded[:start] + replacement + encoded[end:]
raw = base64.b64decode(encoded, validate=True)
assert hashlib.sha256(gzip.decompress(raw)).hexdigest() == 'd9fee41e861ecdd46968388397d90c4e408f66800998b22a588c11a249513e5c'
p.write_bytes(raw)
