import unittest
from pathlib import Path
from textwrap import dedent

from _manim_source_execution import source_execution_proof, source_is_scene_only


class SourceEffectsTests(unittest.TestCase):
    def test_matching_shapes_showcase_uses_only_proven_scene_data_calls(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_shape_matching.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        unsafe = dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    source, target = VGroup(Circle()), VGroup(Square())
                    self.play(TransformMatchingShapes(source, target, key_map=load_keys()))
        ''')
        proof = source_execution_proof(unsafe)
        self.assertFalse(proof["admitted"])
        self.assertEqual(proof["reason"]["code"], "unknown_name")

    def test_ordered_reveal_showcase_uses_typed_subset_lifecycle_constructors(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_ordered_reveal.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        unsafe = dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    group = VGroup(Circle())
                    self.play(ShowIncreasingSubsets(group, int_func=load_int_func()))
        ''')
        proof = source_execution_proof(unsafe)
        self.assertFalse(proof["admitted"])
        self.assertEqual(proof["reason"]["code"], "unknown_name")

    def test_source_proof_reports_the_rejected_construct_and_exact_span(self):
        for expression, code, construct in (
            ('print("effect")', "unknown_name", "print"),
            ('circle.set_x(1)', "unsupported_member", "circle.set_x"),
            ('Code(code_file="input.py")', "unproven_resource_input", 'Code(code_file="input.py")'),
            ('__builtins__', "private_name", "__builtins__"),
        ):
            with self.subTest(expression=expression):
                source = f'from noon import *\ncircle = Circle()\n{expression}\n'
                proof = source_execution_proof(source)
                self.assertFalse(proof["admitted"])
                reason = proof["reason"]
                self.assertEqual(reason["code"], code)
                span = reason["span"]
                self.assertEqual(span["line"], 3)
                self.assertEqual(span["endLine"], 3)
                self.assertEqual(source.splitlines()[2].encode()[span["column"]:span["endColumn"]].decode(), construct)
        self.assertEqual(source_execution_proof('from noon import *\nCircle()'),
                         {"admitted": True, "reason": None})
        syntax = source_execution_proof('π =\n')["reason"]
        self.assertEqual(syntax["code"], "invalid_syntax")
        self.assertEqual(syntax["span"]["line"], 1)
        self.assertGreaterEqual(syntax["span"]["column"], len('π'.encode()))

    def test_module_binding_and_owned_write_failures_keep_their_origin(self):
        for source, code, line in (
            ('print = None\n', "unproven_binding", 1),
            ('from noon import *\nversion = 3\n', "unproven_wildcard_binding", 2),
            ('from noon import *\nvalues = [1]\nvalues[0] = 2\n', "unproven_ownership", 3),
            ('from noon import *\nclass Example(MovingCameraScene):\n    pass\n', "unsupported_class_contract", 2),
        ):
            with self.subTest(source=source):
                reason = source_execution_proof(source)["reason"]
                self.assertEqual(reason["code"], code)
                self.assertEqual(reason["span"]["line"], line)

    def test_lesson_navigation_retains_observed_source_until_completed_replay(self):
        # Prefix transport does not carry navigation metadata; ordinary source
        # execution still compiles its barriers and retains completed replay.
        self.assertFalse(source_is_scene_only(dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    self.wait(1)
                    self.next_section("Detail")
                    self.wait(1)
        ''')))

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

    def test_divmod_is_an_ordinary_builtin_with_module_rebinding_rejected(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    row, column = divmod(17, 4)
                    self.wait(row + column)
        ''')))
        for source, code in (
            ('from noon import *\ndivmod = print\n', "unproven_binding"),
            ('from noon import *\nfrom math import divmod\n', "unproven_binding"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        divmod = print\n', "unknown_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        divide = print\n        divide(17, 4)\n',
             "unknown_name"),
        ):
            with self.subTest(source=source):
                proof = source_execution_proof(source)
                self.assertFalse(proof["admitted"])
                self.assertEqual(proof["reason"]["code"], code)

    def test_discard_store_is_local_and_never_read_or_mutated(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    quotient, _ = divmod(17, 4)
                    self.wait(quotient)
        ''')))
        for source, code in (
            ('from noon import *\n_ = 1\n', "unproven_binding"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        _ = 1\n        self.wait(_)\n', "private_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        _other = 1\n', "private_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        _ += 1\n', "private_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    _ = 1\n    def construct(self):\n        pass\n', "unsupported_class_contract"),
            ('from noon import *\ndef helper():\n    global _\n    _ = 1\n',
             "unproven_scope_effect"),
            ('from noon import *\ndef outer():\n    _ = 1\n    def inner():\n'
             '        nonlocal _\n        _ = 2\n', "unproven_scope_effect"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        values = context\n        values[0] = 2\n',
             "unproven_ownership"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        values = [1]\n        alias = values\n'
             '        alias[0] = 2\n', "unproven_ownership"),
        ):
            with self.subTest(source=source):
                proof = source_execution_proof(source)
                self.assertFalse(proof["admitted"])
                self.assertEqual(proof["reason"]["code"], code)

    def test_subscript_writes_require_fresh_local_container_bindings(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            def trace():
                tile, loads = {}, []
                tile[0] = 7
                output = [None] * 4
                output[0] = tile[0]
                counts = [0 for _ in range(4)]
                counts[0] += 1
                return output
            class Example(Scene):
                def construct(self):
                    self.wait(trace()[0])
        ''')))
        for body in (
            'data = context; data[0] = 1',
            'data = []; alias = data; alias[0] = 1',
            'data = []; data = context; data[0] = 1',
            'data = [];\n        if True: data = context\n        data[0] = 1',
            'data = [[]]; data[0][0] = 1',
            'data = [0] * self; data[0] = 1',
            'data = [];\n        match context:\n            case data: data[0] = 1',
            'data = [];\n        try: self.wait(0)\n        except ValueError as data: data[0] = 1',
        ):
            with self.subTest(body=body):
                proof = source_execution_proof('from noon import *\nclass Example(Scene):\n'
                    '    def construct(self):\n        ' + body + '\n')
                self.assertFalse(proof['admitted'])
                self.assertEqual(proof['reason']['code'], 'unproven_ownership')
        for source in (
            'from noon import *\ndata = []; data[0] = 1\n',
            'from noon import *\ndef edit(data):\n    data[0] = 1\n',
            'from noon import *\ndef edit(data):\n    data[0] = 1\n    data = []\n',
            'from noon import *\ndef outer():\n    data = []\n'
            '    def edit(data):\n        data[0] = 1\n',
        ):
            with self.subTest(source=source):
                self.assertFalse(source_is_scene_only(source))

    def test_list_repetition_requires_a_dominating_builtin_integer_guard(self):
        template = 'from noon import *\ndef histogram(bins):\n%s\n    counts = [0] * bins\n    counts[0] += 1\n'
        self.assertTrue(source_is_scene_only(template %
            '    if isinstance(bins, bool) or not isinstance(bins, int) or bins < 1:\n        raise ValueError("bins")'))
        for guard in (
            '    if bins < 1 or not isinstance(bins, int):\n        raise ValueError("bins")',
            '    if bins.count() or not isinstance(bins, int):\n        raise ValueError("bins")',
            '    if not isinstance(bins, int) and bins < 1:\n        raise ValueError("bins")',
            '    if not isinstance(bins, int):\n        pass',
            '    if not isinstance(bins, int):\n        counts = [0] * bins\n        raise ValueError("bins")',
            '    bins = context\n    if not isinstance(bins, int):\n        raise ValueError("bins")',
            '    isinstance = lambda value, kind: False\n    if not isinstance(bins, int):\n        raise ValueError("bins")',
        ):
            with self.subTest(guard=guard):
                self.assertFalse(source_is_scene_only(template % guard))
        self.assertFalse(source_is_scene_only('from noon import *\n'
            'def outer(isinstance):\n    def histogram(bins):\n'
            '        if not isinstance(bins, int):\n            raise ValueError("bins")\n'
            '        counts = [0] * bins\n        counts[0] = 1\n'))

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

    def test_compound_suite_imports_keep_observed_execution(self):
        for statement in ('from noon import Code as Display', 'import math'):
            with self.subTest(statement=statement):
                expression = 'Display(code_file="input.py")' if statement.startswith('from') else 'math.sin(1)'
                self.assertFalse(source_is_scene_only('from noon import Scene\n'
                    'if True:\n    ' + statement + '\n'
                    'class Example(Scene):\n    def construct(self):\n'
                    '        self.wait(1)\n        ' + expression + '\n'))

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

    def test_unchanged_gallery_and_stress_sources_use_the_closed_subset(self):
        examples = Path(__file__).parent / 'examples'
        for name in ('manim_parity_create_circle.py', 'showcase_code_blocks.py',
                     'showcase_gpu_glow.py', 'showcase_dynamic_scene.py',
                     'manim_parity_stress_grid.py', 'showcase_layered_effects.py'):
            with self.subTest(name=name):
                self.assertTrue(source_is_scene_only((examples / name).read_text()))

    def test_affine_lifecycle_wrappers_are_trusted_as_exact_closed_imports(self):
        examples = Path(__file__).parent / 'examples'
        for name in ('manim_parity_affine_lifecycle.py', 'showcase_entrances_exits.py'):
            with self.subTest(name=name):
                source = (examples / name).read_text()
                self.assertTrue(source_is_scene_only(source))

        alias = dedent('''
            from noon import *
            intro = SpinInFromNothing
            class Example(Scene):
                def construct(self):
                    square = Square()
                    self.play(intro(square))
                    self.play(ShrinkToCenter(square))
        ''')
        self.assertTrue(source_is_scene_only(alias))

        for source, code, span_text in (
            ('from noon import *\nSpinInFromNothing = print\n',
             'unknown_name', 'print'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        SpinInFromNothing.__call__ = Circle\n',
             'unproven_member_write', 'SpinInFromNothing.__call__'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        SpinInFromNothing(context.target)\n',
             'unsupported_member', 'context.target'),
        ):
            with self.subTest(source=source):
                reason = source_execution_proof(source)['reason']
                self.assertEqual(reason['code'], code)
                span = reason['span']
                self.assertEqual(source.splitlines()[span['line'] - 1].encode()
                                 [span['column']:span['endColumn']].decode(), span_text)

    def test_lagged_start_map_requires_proven_factories_and_scene_only_callbacks(self):
        examples = Path(__file__).parent / 'examples'
        timing = (examples / 'showcase_timing.py').read_text()
        self.assertTrue(source_is_scene_only(timing))

        alias = dedent('''
            from noon import *
            animation_factory = FadeIn
            class Example(Scene):
                def construct(self):
                    group = VGroup(Square(), Square())
                    self.play(LaggedStartMap(
                        animation_factory, group,
                        arg_creator=lambda member: (member,),
                    ))
        ''')
        alias_proof = source_execution_proof(alias)
        self.assertFalse(alias_proof["admitted"])
        self.assertEqual(alias_proof["reason"]["code"], "unproven_callback_factory")

        identity = dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    group = VGroup(Square(), Square())
                    self.play(LaggedStartMap(FadeIn, group, arg_creator=lambda member: member))
        ''')
        self.assertTrue(source_is_scene_only(identity))

        blocked = (
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(Code, group))\n',
             'unproven_resource_reference', 'Code'),
            ('from noon import *\ndef make(member):\n'
             '    return Code(code_file="input.py")\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=make))\n',
             'unproven_resource_input', 'Code(code_file="input.py")'),
            ('from noon import *\nimport os as host\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(host.system, group))\n',
             'unsupported_import', 'import os as host'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, '
             'arg_creator=lambda member: print(member)))\n',
             'unproven_callback_argument', 'lambda member: print(member)'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, '
             'arg_creator=lambda member: (self.wait(0.1) and member,)))\n',
             'unproven_callback_argument',
             'lambda member: (self.wait(0.1) and member,)'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=self.wait))\n',
             'unproven_callback_argument', 'self.wait'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=self.play))\n',
             'unproven_callback_argument', 'self.play'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, '
             'lambda member: (member, Code(code_file="input.py"))))\n',
             'unproven_callback_argument', 'lambda member: (member, Code(code_file="input.py"))'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, self.wait))\n',
             'unproven_callback_argument', 'self.wait'),
            ('from noon import *\nfactory = LaggedStartMap\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(factory(FadeIn, group, arg_creator=self.wait))\n',
             'unproven_callback_factory', 'LaggedStartMap'),
            ('from noon import *\nfactory = LaggedStartMap\nwrapped = factory\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(wrapped(FadeIn, group, arg_creator=self.play))\n',
             'unproven_callback_factory', 'LaggedStartMap'),
            ('from noon import LaggedStartMap as Make\nfrom noon import *\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(Make(FadeIn, group, arg_creator=self.wait))\n',
             'unproven_callback_factory', 'from noon import LaggedStartMap as Make'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(LaggedStartMap, group))\n',
             'unproven_callback_factory', 'LaggedStartMap'),
            ('from noon import *\nanimation_factory = FadeIn\n'
             'def helper(animation_factory, group):\n'
             '    return LaggedStartMap(animation_factory, group, '
             'arg_creator=lambda member: (member,))\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(helper(self.wait, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(*[FadeIn, group]))\n',
             'unproven_callback_arguments', 'LaggedStartMap(*[FadeIn, group])'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(self.wait, group))\n',
             'unproven_callback_factory', 'self.wait'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self, LaggedStartMap):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group))\n',
             'unproven_shadowed_constructor', 'LaggedStartMap'),
            ('from noon import *\n'
             'def FadeIn(member):\n    member.wait(0.1)\n    return Circle()\n'
             'animation_factory = FadeIn\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(animation_factory, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\nanimation_factory = FadeIn\n'
             'match {"x": ValueTracker}:\n'
             '    case {"x": animation_factory}:\n        pass\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(animation_factory, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(Scene, group))\n',
             'unproven_callback_factory', 'Scene'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        animation_factory = self.play\n'
             '        self.play(LaggedStartMap(animation_factory, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\n'
             'def make(scene, member):\n    scene.wait(0.1)\n    return (member,)\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=make))\n',
             'unproven_callback_argument', 'make'),
        )
        for source, code, span_text in blocked:
            with self.subTest(source=source):
                reason = source_execution_proof(source)['reason']
                self.assertEqual(reason['code'], code)
                span = reason['span']
                self.assertEqual(source.splitlines()[span['line'] - 1].encode()
                                 [span['column']:span['endColumn']].decode(), span_text)

        # This paired showcase uses only already-qualified map/composition
        # structure; its separate tutorial pair remains blocked at Add.
        paired = (examples / 'ordinary_timed_composition.py').read_text()
        reason = source_execution_proof(paired)['reason']
        self.assertEqual(reason['code'], 'unknown_name')
        self.assertEqual(reason['span']['line'], 11)


if __name__ == '__main__':
    unittest.main()
