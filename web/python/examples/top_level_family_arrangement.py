"""Top-level host form of ordinary_family_arrangement.

The paired Rust program is noon::example_scenes::family_arrangement::program.
Top-level play/wait suspends this Python stack using the existing JSPI host path.
"""
from noon import *

result = Scene()
first = Circle(radius=0.2).set_fill(BLUE, opacity=1).set_stroke(width=0)
second = Circle(radius=0.2).set_fill(YELLOW, opacity=1).set_stroke(width=0)
second.shift(2 * RIGHT)
# One semantic leaf participates in both a direct and a nested member.
nested = VGroup(first, second)
family = VGroup(first, nested)
assert abs(family.width - 2.4) < 1e-6
assert abs(family.height - 0.4) < 1e-6
family.arrange(RIGHT, buff=0.2)
result.add(first, second)
result.wait(0.5)
family = VGroup(first, nested)
family.remove(nested)
family.add(nested)
family.arrange(UP, buff=0.3, center=False, aligned_edge=LEFT)
assert abs(nested.width - 2.4) < 1e-5
assert abs(nested.get_center().x) < 1e-5
nested.move_to(ORIGIN, coor_mask=(1, 0, 0))
nested.next_to(ORIGIN, direction=2 * UP, buff=0.25, coor_mask=(0, 1, 0),
               index_of_submobject_to_align=-1)
nested.align_to(first, UP)
first.next_to(second, LEFT, buff=0.25)
assert abs(first.get_center().x - 0.35) < 1e-6
result.wait(0.5)
