"""Own points become copied children through the shared promotion operation."""
from noon import *


def merged_snapshot(root):
    try:
        return {"kind": "returned", "points": list(map(tuple, root.get_all_points()))}
    except ValueError as error:
        return {"kind": "raised", "type": type(error).__name__, "message": str(error)}


def detached_profiles():
    points = [[float(i), float(i % 3), i * .5 - 1.125] for i in range(8)]
    for cls in (Mobject, VMobject, Group, VGroup):
        for count in (0, 1, 4, 5, 8):
            root = cls().set_points(points[:count])
            child = VMobject().set_points(points[:4])
            root.add(child)
            root.metadata_alias = {"root": root, "child": child, "repeat": [child, child]}
            old_members, old_iterator = root.submobjects, iter(root)
            assert root.push_self_into_submobjects() is root
            clone = root.submobjects[-1]
            assert root.get_num_points() == 0
            assert root.submobjects[0] is child and len(root.submobjects) == 2
            assert list(old_members) == [child]
            assert list(old_iterator) == ([root, child] if count else [child])
            assert clone.submobjects == []
            assert list(map(tuple, clone.get_all_points())) == list(map(tuple, points[:count]))
            assert clone.metadata_alias["root"] is clone
            alias = clone.metadata_alias["child"]
            assert alias is not child
            assert clone.metadata_alias["repeat"] == [alias, alias]
            assert list(map(tuple, alias.get_all_points())) == list(map(tuple, points[:4]))
            peer = VMobject().set_points(points[:4])
            assert root.null_point_align(peer) is root
            assert peer.get_num_points() == 0
            assert peer.submobjects[0].get_num_points() == 4
            before_members = list(root.submobjects), list(peer.submobjects)
            assert root.null_point_align(peer) is root
            assert (list(root.submobjects), list(peer.submobjects)) == before_members
            before_points = merged_snapshot(root)
            if count == 0:
                assert before_points == {
                    "kind": "raised", "type": "ValueError",
                    "message": "all the input arrays must have same number of dimensions, "
                    "but the array at index 0 has 2 dimension(s) and the array at index 1 has 1 dimension(s)",
                }, before_points
            else:
                assert before_points["kind"] == "returned"
            assert root.push_self_into_submobjects() is root
            second = root.submobjects[-1]
            assert second.get_num_points() == 0 and second.submobjects == []
            assert second.metadata_alias["root"] is second
            assert merged_snapshot(root) == before_points


class PointPromotion(Scene):
    async def construct(self):
        detached_profiles()
        left = Circle(.7, color="#FF0000", fill_opacity=1, stroke_width=0).shift(LEFT * 1.2)
        right = Circle(.7, color="#FF0000", fill_opacity=1, stroke_width=0).shift(RIGHT * 1.2)
        left_child = Square(.8, color="#0000FF", fill_opacity=1, stroke_width=0).shift(LEFT * 1.2)
        right_child = Square(.8, color="#00FF00", fill_opacity=1, stroke_width=0).shift(RIGHT * 1.2)
        left.shift(OUT * 1.125)
        right.shift(IN * 3.5)
        left.add(left_child)
        right.add(right_child)
        left.metadata_alias = left_child
        self.add(left, right)
        await self.wait(.25)
        before = list(map(tuple, left.get_all_points()[:left.get_num_points()]))
        captured = iter(left)
        assert left.push_self_into_submobjects() is left
        assert list(captured) == [left, left_child]
        copied = left.submobjects[1]
        assert left.get_num_points() == 0 and copied.get_num_points() == 32
        assert copied.submobjects == []
        assert list(map(tuple, copied.get_all_points())) == before
        assert copied.metadata_alias is not left_child
        assert copied.metadata_alias.get_num_points() == 16
        await self.wait(.25)
        assert left.null_point_align(right) is left
        assert left.get_num_points() == right.get_num_points() == 0
        assert right.submobjects[1].get_num_points() == 32
        assert right.submobjects[1].submobjects == []
        assert left.null_point_align(right) is left
        await self.wait(.25)
