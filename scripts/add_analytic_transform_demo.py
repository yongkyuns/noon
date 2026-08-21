from pathlib import Path

path = Path("web/main.js")
text = path.read_text()
entry = '''  {\n    name: "Analytic primitive Transform",\n    path: "./python/examples/analytic_transform.py",\n    summary:\n      "Circle radius, rectangle size and line endpoints interpolate directly without path conversion or tessellation.",\n    features: "Transform · analytic geometry · zero tessellation",\n  },\n'''
if entry not in text:
    marker = '''  {\n    name: "Path morph / Transform",\n'''
    if marker not in text:
        raise SystemExit("scene example insertion marker not found")
    text = text.replace(marker, entry + marker, 1)
    path.write_text(text)
