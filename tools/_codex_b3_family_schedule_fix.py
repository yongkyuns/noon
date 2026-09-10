from pathlib import Path

path = Path("tools/_codex_b3_family_schedule_patch.py")
text = path.read_text()
marker = "# Published lowering collects the third internal leaf kind separately."
marker_pos = text.index(marker)
start = text.index("r1(p,", marker_pos)
next_call = text.index("r1(p,", start + 1)
replacement = '''text = Path(p).read_text()
needle = "pub fn lower_semantic_animation_schedule("
pos = text.index(needle)
tail = text[pos:]
old = """    let mut leaves = Vec::new();
    let mut scalar_leaves = Vec::new();
    for leaf in projection.leaves {"""
if tail.count(old) != 2:
    raise RuntimeError(f"published lowering expected published+prepared declarations, found {tail.count(old)}")
tail = tail.replace(old, """    let mut leaves = Vec::new();
    let mut scalar_leaves = Vec::new();
    let mut family_transforms = Vec::new();
    for leaf in projection.leaves {""", 1)
Path(p).write_text(text[:pos] + tail)
'''
path.write_text(text[:start] + replacement + text[next_call:])
