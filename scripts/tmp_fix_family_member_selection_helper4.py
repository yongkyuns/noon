from pathlib import Path

path = Path("scripts/tmp_shared_family_member_selection_migration.py")
text = path.read_text()
old_start = text.index("    old_target = '''")
new_start = text.index("    new_target = '''", old_start)
text = text[:old_start] + text[new_start:]
old_apply = '    python = replace_once(python, old_target, new_target, label="route selected group next_to")\n'
new_apply = '''    group_next_to = python.index("\\ndef _group_next_to(")
    dispatch_start = python.index(
        "    if _alignment_is_mobject(mobject_or_point):\\n",
        group_next_to,
    )
    dispatch_end = python.index("    if translation is None:\\n", dispatch_start)
    python = python[:dispatch_start] + new_target + python[dispatch_end:]
'''
if old_apply not in text:
    raise RuntimeError("selected target apply block not found")
path.write_text(text.replace(old_apply, new_apply, 1))
