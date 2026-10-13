import unittest
import noon


class ClassQueries(unittest.TestCase):
    def test_base_and_vector_classes_preserve_distinct_family_types(self):
        for cls, group, base in [
            (noon.Mobject, noon.Group, noon.Mobject),
            (noon.Group, noon.Group, noon.Mobject),
            (noon.VMobject, noon.VGroup, noon.VMobject),
            (noon.VGroup, noon.VGroup, noon.VMobject),
            (noon.Circle, noon.VGroup, noon.VMobject),
            (noon.Square, noon.VGroup, noon.VMobject),
        ]:
            instance = object.__new__(cls)
            self.assertIs(instance.get_group_class(), group)
            self.assertIs(instance.get_mobject_type_class(), base)
            self.assertIs(cls.get_mobject_type_class(), base)

    def test_subclass_inheritance_and_override_use_python_dispatch(self):
        class Custom(noon.VGroup):
            pass
        self.assertIs(Custom.get_mobject_type_class(), noon.VMobject)
        self.assertIs(object.__new__(Custom).get_group_class(), noon.VGroup)
        class Override(Custom):
            def get_group_class(self): return Custom
            @staticmethod
            def get_mobject_type_class(): return Custom
        self.assertIs(object.__new__(Override).get_group_class(), Custom)
        self.assertIs(Override.get_mobject_type_class(), Custom)


if __name__ == "__main__": unittest.main()
