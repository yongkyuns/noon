from pathlib import Path

p = Path("scripts/b2-browser-local-transactions.py")
text = p.read_text()
old = '            Self::Evaluation(error) => write!(formatter, \\"{error}\\"),'
new = '            Self::Evaluation(error) => write!(formatter, \\"scene evaluation failed: {error}\\"),'
if old not in text:
    raise SystemExit("stale PlayerError anchor not found in bootstrap helper")
p.write_text(text.replace(old, new, 2))
