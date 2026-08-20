from pathlib import Path

path = Path("crates/noon-compile/src/lib.rs")
text = path.read_text()
old = "DynamicProperties {\n                position: false,"
new = "DynamicProperties {\n                transform: false,\n                position: false,"
count = text.count(old)
if count != 2:
    raise SystemExit(f"expected two DynamicProperties test initializers, found {count}")
path.write_text(text.replace(old, new))
