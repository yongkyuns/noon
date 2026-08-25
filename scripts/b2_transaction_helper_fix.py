from pathlib import Path

path = Path("scripts/b2_transaction_integration.py")
text = path.read_text()
anchor = "old_tx = '''"
index = text.index(anchor)
head, tail = text[:index], text[index:]
old = "let mut instance = SceneInstance::new(compiled);"
new = "let mut instance = SlottedSceneInstance::new(compiled);"
if old not in tail:
    raise RuntimeError("browser transaction constructor anchor missing")
tail = tail.replace(old, new, 1)
path.write_text(head + tail)
