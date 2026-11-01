import unittest
from pathlib import Path
from textwrap import dedent

from _manim_source_execution import source_is_scene_only


class SourceEffectsTests(unittest.TestCase):
    def test_closed_noon_operations_and_owned_helper_alias_remain_admissible(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            import math
            def offset(value):
                return math.sin(value) * RIGHT
            class Example(Scene):
                def construct(self):
                    circle = Circle(radius=0.5)
                    self.add(circle)
                    self.wait(1)
                    calculate = offset
                    for index in range(3):
                        circle.shift(calculate(index))
                    self.wait(1)
        ''')))

    def test_unknown_host_work_and_dispatch_cannot_be_proven(self):
        for statement in (
            'print("external work")', 'sink = print; sink("external work")',
            'getattr(circle, "shift")(RIGHT)', 'circle.__class__',
            'circle.shift = lambda x: None', 'RIGHT[0] = 5',
            'from js import postMessage', 'import os', 'from noon import np',
            'str.format("{0.__class__}", circle)',
            'def generic[T: print("external work")](): pass',
        ):
            with self.subTest(statement=statement):
                self.assertFalse(source_is_scene_only('from noon import *\n'
                    'class Example(Scene):\n    def construct(self):\n'
                    '        circle = Circle()\n        self.wait(1)\n'
                    + '\n'.join('        ' + line for line in statement.splitlines())))

    def test_nested_bindings_do_not_hide_an_outer_builtin_effect(self):
        for hidden in ('def helper(print):\n    pass',
                       'unused = [print for print in ()]',
                       'class Unrelated(Scene):\n    def helper(self, print):\n        pass'):
            with self.subTest(hidden=hidden):
                self.assertFalse(source_is_scene_only('from noon import *\n' + hidden
                    + '\nclass Example(Scene):\n    def construct(self):\n'
                    '        self.wait(1)\n        print("external work")\n'))
        self.assertFalse(source_is_scene_only('print("external work")\nprint = Circle\n'))
        self.assertFalse(source_is_scene_only('from noon import *\n'
            'version("package")\nversion = None\n'))

    def test_user_destructors_and_definition_hooks_keep_observed_execution(self):
        for method in ('__del__', '__getattribute__', '__getattr__', '__init_subclass__'):
            with self.subTest(method=method):
                self.assertFalse(source_is_scene_only(f'from noon import *\n'
                    f'class Example(Scene):\n    def {method}(self):\n        pass\n'
                    '    def construct(self):\n        self.wait(1)\n'))

    def test_code_file_input_is_not_hidden_by_the_noon_constructor_or_alias(self):
        for expression in ('Code("input.py")', 'Code(code_file="input.py")',
                           'Code(code_string="x", **options)', 'factory = Code',
                           'Display(code_file="input.py")'):
            with self.subTest(expression=expression):
                self.assertFalse(source_is_scene_only('from noon import *\n'
                    'from noon import Code as Display\noptions = {}\n' + expression))
        self.assertTrue(source_is_scene_only('from noon import Code as Display\n'
            'display = Display(code_string="print(1)", language="python")\n'))

    def test_later_special_namespace_binding_cannot_admit_the_original_builtins(self):
        self.assertFalse(source_is_scene_only(dedent('''
            from noon import *
            saved = __builtins__
            __builtins__ = saved
            class Example(Scene):
                def construct(self):
                    self.wait(1)
                    saved["print"]("opaque work")
        ''')))
        self.assertFalse(source_is_scene_only('from noon import Code as __builtins__\n'))

    def test_unchanged_basic_text_and_glow_gallery_use_the_closed_subset(self):
        examples = Path(__file__).parent / 'examples'
        for name in ('manim_parity_create_circle.py', 'showcase_code_blocks.py', 'showcase_gpu_glow.py'):
            with self.subTest(name=name):
                self.assertTrue(source_is_scene_only((examples / name).read_text()))


if __name__ == '__main__':
    unittest.main()
