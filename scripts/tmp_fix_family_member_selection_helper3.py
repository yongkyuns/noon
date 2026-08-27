from pathlib import Path

path = Path("scripts/tmp_shared_family_member_selection_migration.py")
text = path.read_text()
start = text.index("    old_guard = '''")
end_line = '    python = replace_once(python, old_guard, "", label="remove group next_to selection fallback")\n'
end = text.index(end_line, start) + len(end_line)
replacement = '''    group_next_to = python.index("\\ndef _group_next_to(")
    guard_comment = "    # Selecting a specific wrapper/member remains explicit #61 debt until shared\\n"
    guard_start = python.index(guard_comment, group_next_to)
    guard_end = python.index(
        "    shared = _shared_family_layout_session(self, mutation=True)\\n",
        guard_start,
    )
    python = python[:guard_start] + python[guard_end:]
'''
path.write_text(text[:start] + replacement + text[end:])
