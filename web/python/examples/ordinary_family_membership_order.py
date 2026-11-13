from noon import *


def assert_nested_membership():
    first, second, third = Square(.2), Circle(.2), Square(.3)
    nested = VGroup(first)
    family = VGroup(nested, second)
    assert nested.add(third) is nested
    assert nested.submobjects == [first, third]
    family.add_to_back(second)
    assert family.submobjects == [second, nested]
    nested.remove(first)
    nested.insert(0, first)
    nested.invert()
    assert nested.submobjects == [third, first]
    nested.submobjects = [second, first]
    assert nested[0] is second and nested[1] is first


class OrdinaryFamilyMembershipOrder(Scene):
    def construct(self):
        assert_nested_membership()
        a = Square(side_length=2, color="#FF0000", fill_opacity=1, stroke_width=0)
        b = Square(side_length=2, color="#0000FF", fill_opacity=1, stroke_width=0).shift(RIGHT)
        nested = VGroup(a, b)
        query_root = Group(nested, a)
        assert query_root.get_family() == [query_root, nested, b, a]
        assert query_root.get_family(recurse=False) == [query_root, nested, b, a]
        assert a.get_family() == [a]
        family = VGroup(a, b, a)
        assert list(family) == [b, a]
        family.add(b)
        assert list(family) == [a, b]
        self.add(family)
        self.wait(0.05)
        assert_nested_membership()
        assert family.get_family() == [family, a, b]
        family.add(a)
        assert list(family) == [b, a]
        assert family.get_family() == [family, b, a]
        self.wait(0.15)
