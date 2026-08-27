from pathlib import Path

path = Path("scripts/tmp_shared_family_translation_migration.py")
text = path.read_text()
old = '''    if count != 1:\n        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")\n    return text.replace(old, new, 1)\n'''
new = '''    if label == "pin JS family translation class":\n        if count < 1:\n            raise SystemExit(f"{label}: expected at least one anchor, found {count}")\n    elif count != 1:\n        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")\n    return text.replace(old, new, 1)\n'''
if text.count(old) != 1:
    raise SystemExit(f"replace_once patch expected once, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
