from pathlib import Path

path = Path("scripts/retained-text-family-layout-smoke.mjs")
text = path.read_text()
old = '''        placement.next_to(placement_target, RIGHT, buff=0.25)
        gap = placement.get_critical_point(LEFT).x - placement_target.get_critical_point(RIGHT).x
        close(gap, 0.25, "next_to retained target gap")
        placement.align_to(placement_target, UP)
        close(
            placement.get_critical_point(UP).y,
            placement_target.get_critical_point(UP).y,
            "align_to retained target top",
        )
'''
new = '''        placement.next_to(placement_target, RIGHT, buff=0.25)
        placement_left = placement.get_center().x - placement.width * 0.5
        gap = placement_left - placement_target.get_critical_point(RIGHT).x
        close(gap, 0.25, "next_to retained target gap")
        placement.align_to(placement_target, UP)
        placement_top = placement.get_center().y + placement.height * 0.5
        close(
            placement_top,
            placement_target.get_critical_point(UP).y,
            "align_to retained target top",
        )
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected one placement smoke API context, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
print("retained family placement smoke API corrected")
