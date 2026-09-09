from noon import *


class OrdinaryFamilyPlacement(Scene):
    def construct(self):
        first = Circle(radius=0.2).set_fill(BLUE, opacity=1).set_stroke(width=0)
        second = Circle(radius=0.2).set_fill(YELLOW, opacity=1).set_stroke(width=0)
        anchor = Square(side_length=1).set_fill(RED, opacity=1).set_stroke(width=0)
        second.shift(0.8 * RIGHT)
        nested = VGroup(second)
        family = VGroup()
        family.add(first, nested, first)
        family.remove(first, nested)
        family.add(first, nested)
        target = VGroup(anchor)
        first.move_to(target, aligned_edge=UP)
        first.align_to(target, DOWN)
        assert abs(first.get_bottom().y - target.get_bottom().y) < 1e-6
        family.next_to(target, direction=2 * RIGHT, buff=0.25,
                       index_of_submobject_to_align=0)
        family.align_to(UP, UP)
        family.move_to(target, coor_mask=(1, 0, 0))
        center = family.get_center()
        family.set_x(center.x + family.width / 2, RIGHT)
        family.set_y(center.y + family.height / 2, UP)
        family.to_corner(2 * RIGHT + UP, buff=0.25)
        assert abs(family.get_right().x - (DEFAULT_FRAME_WIDTH / 2 - 0.5)) < 1e-6
        assert abs(family.get_top().y - 3.75) < 1e-6
        family.move_to(center)
        family.shift(0.2 * UP)
        self.add(first, second, anchor)
        self.wait(1)
        # After the barrier, the same typed target observes live effective bounds.
        center = family.get_center()
        family.set_x(center.x + family.width / 2, RIGHT)
        family.set_y(center.y + family.height / 2, UP)
        family.to_edge(DOWN, buff=0.5)
        assert abs(family.get_bottom().y + 3.5) < 1e-6
        family.move_to(center)
        before = first.get_center()
        first.move_to(target, coor_mask=(0, 0, 0))
        assert abs(first.get_center().x - before.x) < 1e-6
        assert abs(first.get_center().y - before.y) < 1e-6
