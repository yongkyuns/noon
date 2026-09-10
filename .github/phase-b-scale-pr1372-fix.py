from pathlib import Path

path = Path("parity/manim-v0.21/manifest.json")
text = path.read_text()
old = '''      "expected_duration": 0.4
  ],
  "sample_fractions": [
'''
new = '''      "expected_duration": 0.4
    }
  ],
  "sample_fractions": [
'''
if text.count(old) != 1:
    raise SystemExit("expected exactly one generated scale-pivots manifest closure")
path.write_text(text.replace(old, new, 1))
