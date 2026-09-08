from pathlib import Path
import base64, gzip, hashlib
s = Path('gallery-python.patch.gz.b64').read_text()
for before, after in [
 ('CvPS9CvPS9', 'CvPS9'),
 ('FBsFBs+EV', 'FBs+EV'),
 ('apr3t3fXn3mwy', 'apr3t3mwy'),
 ('QxtbcQbcQdd', 'QxtbcQdd'),
 ('L8dyHdyHjgq', 'L8dyHjgq'),
 ('JsGX4X4X+XW', 'JsGX4X+XW'),
 ('ppM6G6G9vc', 'ppM6G9vc'),
 ('ok94m94m1p', 'ok94m1p'),
 ('l2JsK3pK3T19', 'l2JsK3T19'),
 ('ukmfkq1a1aMb', 'ukmfkq1aMb'),
]:
    assert before in s, before
    s = s.replace(before, after)
raw = gzip.decompress(base64.b64decode(''.join(s.split()), validate=True))
assert hashlib.sha256(raw).hexdigest() == 'dedd86f8eff78de9fe417a743048612dcd5ac713785cbcb2596782c140bedd61'
Path('/tmp/gallery-python.patch').write_bytes(raw)
