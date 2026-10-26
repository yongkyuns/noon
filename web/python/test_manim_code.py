import pathlib
import tempfile
import types
import unittest
from unittest.mock import patch

import noon
import _manim_code as code


class Handle:
    def __init__(self, slot, source=""):
        self.semanticSlot, self.semanticGeneration = slot, 1
        self.source = source

    def textSource(self):
        return self.source


class Family(Handle):
    def __init__(self, slot, members=()):
        super().__init__(slot)
        self._semantic_member_wrappers = {code._handle_key(member._semantic_handle): member
                                          for member in members}
        self._member_keys = None

    @property
    def submobjects(self):
        if self._member_keys is None:
            return list(self._semantic_member_wrappers.values())
        return [self._semantic_member_wrappers[key] for key in self._member_keys]


class CodeHandle:
    def __init__(self, numbers=True):
        self.sources = ["def demo():", "    return 1", ""]
        self.lines = Family(2, [types.SimpleNamespace(_semantic_handle=Handle(10 + i, source))
                                for i, source in enumerate(self.sources)])
        self.lines._member_keys = list(self.lines._semantic_member_wrappers)
        self.panel = Family(3)
        self.regions = Family(5)
        self.numbers = Family(4, [types.SimpleNamespace(_semantic_handle=Handle(20 + i)) for i in range(3)]) if numbers else None
        self.root = Handle(1)
        self.root.members = {code._handle_key(family): family
                            for family in (self.panel, self.regions, self.lines, self.numbers) if family is not None}
        self.source_reads, self.number_reads = 0, 0

    def family(self):
        return self.root

    def codeLines(self):
        return self.lines

    def background(self):
        return self.panel

    def highlights(self):
        return self.regions

    def lineNumbers(self):
        return self.numbers

    def lineSources(self):
        self.source_reads += 1
        return self.sources.copy()

    def lineNumberSources(self):
        self.number_reads += 1
        return ["9", "10", "11"] if self.numbers else []

    def lineSource(self, _index):
        raise AssertionError("wrapper construction must observe sources in one batch")


class Options:
    def __init__(self, *args):
        self.args, self.calls, self.freed = args, {}, False
        self.call_sequence = []

    def __getattr__(self, name):
        if name.startswith("set"):
            def setter(*args):
                self.calls[name] = args
                self.call_sequence.append((name, args))
            return setter
        raise AttributeError(name)

    def free(self):
        self.freed = True


class Replacement:
    def __init__(self, previous, current):
        self._previous, self._current = previous, current
        self.freed = False

    def previous(self):
        return self._previous

    def current(self):
        return self._current

    def free(self):
        self.freed = True


class CodeFacadeTests(unittest.TestCase):
    def setUp(self):
        self.handle = CodeHandle()
        self.context = object()
        self.options = []
        self.highlight_options = []
        self.region_handles = []
        self.replacements = []

        def options(*args):
            result = Options(*args)
            self.options.append(result)
            return result

        def attach(wrapper, handle, context):
            wrapper._semantic_family_handle = handle
            wrapper._semantic_member_wrappers = handle.members
            wrapper._canonical_live_target_context = context

        def highlight_options(*args):
            result = Options(*args)
            self.highlight_options.append(result)
            return result

        def region(*_args):
            result = Handle(100 + len(self.region_handles))
            self.region_handles.append(result)
            return result

        def replace(_context, handle, index, source):
            index = int(index)
            previous = handle.lines.submobjects[index]._semantic_handle
            current = Handle(200 + len(self.replacements), source)
            handle.lines._member_keys[index] = code._handle_key(current)
            handle.sources[index] = source
            result = Replacement(previous, current)
            self.replacements.append(result)
            return result

        self.patches = [
            patch.object(code, "_new_code_options", side_effect=options),
            patch.object(code, "_create_code", side_effect=lambda *_: self.handle),
            patch.object(code, "_code_from_family", side_effect=lambda *_: self.handle),
            patch.object(code, "_live_constructor_context", return_value=self.context),
            patch.object(code, "_attach_shared_family", side_effect=attach),
            patch.object(code, "_code_styles_list", return_value=["vim", "monokai"]),
            patch.object(code, "_code_languages_list", return_value=["Python", "Rust"]),
            patch.object(code, "_new_highlight_options", side_effect=highlight_options),
            patch.object(code, "_get_highlighted_lines", side_effect=region),
            patch.object(code, "_highlight_lines", side_effect=region),
            patch.object(code, "_replace_line", side_effect=replace),
        ]
        for patcher in self.patches:
            patcher.start()
        self.addCleanup(lambda: [patcher.stop() for patcher in reversed(self.patches)])

    def test_source_is_forwarded_unchanged_and_rows_are_native_text_wrappers(self):
        source = "def demo():\n\treturn 1\n"
        value = code.Code(code_string=source, language="python", line_numbers_from=9,
                          paragraph_config={"color": noon.RED})
        code._create_code.assert_called_once_with(source, self.options[0], self.context)
        self.assertEqual(self.options[0].args, ("python", None, "vim", 4, True, 9, "rectangle"))
        self.assertEqual(self.options[0].calls["setParagraph"], ("Monospace", 24.0, 0.5, True, None))
        self.assertEqual(self.options[0].calls["setBackgroundStroke"][-2:], (0.01, 1.0))
        self.assertFalse(self.options[0].freed)
        self.assertIs(value.code_lines, self.handle.lines)
        self.assertIs(value.background, self.handle.panel)
        self.assertIs(value.highlights, self.handle.regions)
        self.assertTrue(all(isinstance(line, code.Text) for line in value.code_lines.submobjects))
        self.assertEqual([line.text for line in value.code_lines.submobjects], self.handle.lineSources())
        self.assertEqual(value.line_numbers.submobjects[1].text, "10")
        self.assertEqual(value.code_string, "def demo():\n    return 1\n")

    def test_file_takes_precedence_and_passes_a_filename_hint_to_rust(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "example.py"
            path.write_text("π = 3\n\tprint(π)\n", encoding="utf-8")
            code.Code(code_file=path, code_string=object())
            self.assertEqual(code._create_code.call_args.args[0], "π = 3\n\tprint(π)\n")
            self.assertEqual(self.options[0].args[:2], (None, str(path)))

    def test_disabled_numbers_are_absent_and_source_queries_are_batched(self):
        self.handle = CodeHandle(numbers=False)
        value = code.Code(code_string="", add_line_numbers=False)
        self.assertFalse(hasattr(value, "line_numbers"))
        self.assertEqual(self.handle.source_reads, 1)
        self.assertEqual(self.handle.number_reads, 0)

    def test_bad_python_option_shapes_fail_before_native_admission(self):
        for options, error in [
            ({"tab_width": True}, TypeError), ({"tab_width": 1.5}, TypeError),
            ({"line_numbers_from": "4"}, TypeError), ({"add_line_numbers": 1}, TypeError),
            ({"language": 4}, TypeError), ({"formatter_style": object}, NotImplementedError),
            ({"paragraph_config": {"disable_ligatures": 1}}, TypeError),
            ({"paragraph_config": {"font": 3}}, TypeError),
            ({"paragraph_config": {"unknown": 3}}, NotImplementedError),
            ({"background_config": {"unknown": 3}}, NotImplementedError),
            ({"reserved_source": ["line"]}, TypeError),
            ({"syntax_colors": []}, TypeError),
            ({"token_colors": "tile"}, TypeError),
            ({"syntax_colors": {1: noon.RED}}, TypeError),
            ({"token_colors": {False: noon.RED}}, TypeError),
        ]:
            with self.subTest(options=options), self.assertRaises(error):
                code.Code(code_string="x", **options)
        with self.assertRaises(ValueError):
            code.Code()
        code._create_code.assert_not_called()

    def test_configuration_failure_frees_only_unconsumed_options(self):
        with patch.object(code._compat, "_as_color", return_value=noon.WHITE), \
                self.assertRaises(ValueError):
            code.Code(code_string="x", background_config={"buff": "not a number"})
        self.assertTrue(self.options[0].freed)
        code._create_code.assert_not_called()
        with patch.object(code, "_create_code", side_effect=RuntimeError("native admission rejected")), \
                self.assertRaisesRegex(RuntimeError, "native admission rejected"):
            code.Code(code_string="x")
        self.assertFalse(self.options[1].freed, "consumed WASM input must not be freed twice")

    def test_rehydration_preserves_text_identity_and_host_metadata(self):
        value = code.Code(code_string="x", paragraph_config={"font_size": 20})
        retained = value.code_lines.submobjects[0]
        retained._font_size = 37
        retained.custom_metadata = "author annotation"
        value._rehydrate_semantic_family_handle()
        self.assertIs(value.code_lines.submobjects[0], retained)
        self.assertEqual(retained.font_size, 37)
        self.assertEqual(retained.custom_metadata, "author annotation")
        self.assertEqual(retained.font, "Monospace")
        self.assertEqual(retained.line_spacing, 0.5)

    def test_supported_names_come_from_native_registry_and_export_is_available(self):
        self.assertIs(noon.Code, code.Code)
        self.assertEqual(code.Code.get_styles_list(), ["vim", "monokai"])
        self.assertEqual(code.Code.get_languages_list(), ["Python", "Rust"])
        with self.assertRaisesRegex(NotImplementedError, "native named themes"):
            code.Code.get_pygments_style("vim")

    def test_reservation_and_color_maps_forward_without_python_lexing(self):
        reserved = "def demo():\n    return 10000\n"
        code.Code(code_string="x", reserved_source=reserved,
                  syntax_colors={"foreground": noon.WHITE, "comment": noon.GRAY},
                  token_colors={"tile": noon.BLUE, "threadIdx.x": noon.GREEN})
        self.assertEqual(self.options[0].calls["setReservedSource"], (reserved,))
        selected = [(name, args) for name, args in self.options[0].call_sequence
                    if name in {"setSyntaxColor", "setTokenColor"}]
        self.assertEqual(selected, [
            ("setSyntaxColor", ("foreground", noon.WHITE.red, noon.WHITE.green, noon.WHITE.blue, noon.WHITE.alpha)),
            ("setSyntaxColor", ("comment", noon.GRAY.red, noon.GRAY.green, noon.GRAY.blue, noon.GRAY.alpha)),
            ("setTokenColor", ("tile", noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha)),
            ("setTokenColor", ("threadIdx.x", noon.GREEN.red, noon.GREEN.green, noon.GREEN.blue, noon.GREEN.alpha)),
        ])

    def test_replacement_preserves_previous_alias_and_unaffected_wrappers(self):
        value = code.Code(code_string="x", paragraph_config={"font_size": 20})
        previous = value.code_lines.submobjects[1]
        previous.annotation = "old source"
        unaffected = value.code_lines.submobjects[0]
        result = value.replace_line(1, "    return 10000")
        self.assertIs(result[0], previous)
        self.assertEqual(result[0].text, "    return 1")
        self.assertEqual(result[0].annotation, "old source")
        self.assertIs(value.code_lines.submobjects[0], unaffected)
        self.assertIs(value.code_lines.submobjects[1], result[1])
        self.assertIsInstance(result[1], code.Text)
        self.assertEqual(result[1].text, "    return 10000")
        self.assertEqual(result[1].font_size, 20)
        self.assertIs(result[1]._canonical_live_target_context, self.context)
        self.assertEqual(value.code_string, "def demo():\n    return 10000\n")
        self.assertTrue(self.replacements[0].freed)
        code._replace_line.assert_called_once_with(self.context, self.handle, 1.0, "    return 10000")
        again_old, again_current = value.replace_line(1, "    return 1")
        self.assertIs(again_old, result[1])
        self.assertIs(value.code_lines.submobjects[1], again_current)
        self.assertEqual(again_old.text, "    return 10000")
        self.assertEqual(value.code_string, "def demo():\n    return 1\n")
        value._rehydrate_semantic_family_handle()
        self.assertIs(value.code_lines.submobjects[1], again_current)
        self.assertIs(value.code_lines.submobjects[0], unaffected)

    def test_replacement_keeps_old_binding_for_ordinary_scene_reentry(self):
        value = code.Code(code_string="x")
        previous = value.code_lines.submobjects[0]
        previous._scene = scene = object()
        previous._object = derived_object = object()
        original_handle = previous._semantic_handle
        reconciled = []

        def reconcile(owner, operation, values):
            self.assertIs(owner, scene)
            self.assertEqual(operation, "remove")
            self.assertEqual(values, (previous,))
            previous._scene = None
            reconciled.append(values)

        with patch.dict("sys.modules", {"_manim_scene": types.SimpleNamespace(
                _sync_membership_wrapper_attachments=reconcile)}):
            old, _ = value.replace_line(0, "other")
        self.assertEqual(reconciled, [(previous,)])
        self.assertIs(old, previous)
        self.assertIs(old._semantic_handle, original_handle)
        self.assertIs(old._object, derived_object)
        self.assertIsNone(old._scene)

    def test_replacement_python_shapes_and_native_failure_do_not_change_aliases(self):
        value = code.Code(code_string="x")
        original = value.code_lines.submobjects
        for index, source, error in [(True, "x", TypeError), (1.5, "x", TypeError),
                                     ("1", "x", TypeError), (10 ** 1000, "x", ValueError),
                                     (1, None, TypeError)]:
            with self.subTest(index=index, source=source), self.assertRaises(error):
                value.replace_line(index, source)
        code._replace_line.assert_not_called()
        with patch.object(code, "_replace_line", side_effect=ValueError("source does not fit")), \
                self.assertRaisesRegex(ValueError, "does not fit"):
            value.replace_line(-1, "not a native row")
        self.assertEqual(value.code_lines.submobjects, original)
        self.assertEqual(value.code_string, "def demo():\n    return 1\n")

    def test_line_regions_forward_native_defaults_and_preserve_attached_alias(self):
        value = code.Code(code_string="x")
        detached = value.get_highlighted_lines(2)
        self.assertIsInstance(detached, code._compat.Rectangle)
        self.assertIs(detached._canonical_live_target_context, self.context)
        self.assertEqual(self.highlight_options[0].args, (False,))
        self.assertEqual(self.highlight_options[0].calls, {})
        code._get_highlighted_lines.assert_called_once_with(
            self.context, self.handle, 2.0, None, self.highlight_options[0])
        self.assertEqual(value.highlights.submobjects, [])

        attached = value.highlight_lines(0, 2, include_line_numbers=True,
                                         padding=0.08, color=noon.BLUE, fill_opacity=0.35)
        self.assertIs(value.highlights.submobjects[0], attached)
        self.assertEqual(self.highlight_options[1].args, (True,))
        self.assertEqual(self.highlight_options[1].calls, {
            "setPadding": (0.08,), "setFillOpacity": (0.35,),
            "setColor": (noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha),
        })
        code._highlight_lines.assert_called_once_with(
            self.context, self.handle, 0.0, 2.0, self.highlight_options[1])
        attached.annotation = "retained ordinary rectangle"
        value._rehydrate_semantic_family_handle()
        self.assertIs(value.highlights.submobjects[0], attached)
        self.assertEqual(attached.annotation, "retained ordinary rectangle")

    def test_line_region_argument_shapes_fail_before_native_admission(self):
        value = code.Code(code_string="x")
        cases = [
            ((True,), {}, TypeError), ((1.5,), {}, TypeError), (("1",), {}, TypeError),
            ((0, False), {}, TypeError), ((0, 2.5), {}, TypeError),
            ((0,), {"include_line_numbers": 1}, TypeError),
            ((0,), {"padding": True}, TypeError),
            ((0,), {"fill_opacity": "0.2"}, TypeError),
            ((10 ** 1000,), {}, ValueError), ((0, 10 ** 1000), {}, ValueError),
            ((0,), {"padding": 10 ** 1000}, ValueError),
        ]
        for args, kwargs, expected in cases:
            with self.subTest(args=args, kwargs=kwargs), self.assertRaises(expected):
                value.highlight_lines(*args, **kwargs)
        code._highlight_lines.assert_not_called()
        code._new_highlight_options.assert_not_called()

    def test_line_region_bounds_and_style_validation_remain_native(self):
        value = code.Code(code_string="x")
        value.get_highlighted_lines(-1, 2 ** 32, padding=-1, fill_opacity=float("inf"))
        self.assertEqual(code._get_highlighted_lines.call_args.args[2:4], (-1.0, float(2 ** 32)))
        self.assertEqual(self.highlight_options[0].calls["setPadding"], (-1.0,))
        self.assertEqual(self.highlight_options[0].calls["setFillOpacity"], (float("inf"),))

    def test_region_configuration_frees_only_unconsumed_options(self):
        value = code.Code(code_string="x")
        rejected = Options()

        def reject_color(*_args):
            raise ValueError("invalid native color")

        rejected.setColor = reject_color
        with patch.object(code, "_new_highlight_options", return_value=rejected), \
                self.assertRaisesRegex(ValueError, "invalid native color"):
            value.highlight_lines(0, color=noon.BLUE)
        self.assertTrue(rejected.freed)
        code._highlight_lines.assert_not_called()
        with patch.object(code, "_highlight_lines", side_effect=ValueError("invalid native range")), \
                self.assertRaisesRegex(ValueError, "invalid native range"):
            value.highlight_lines(0)
        self.assertFalse(self.highlight_options[0].freed)
        self.assertEqual(value.highlights.submobjects, [])


if __name__ == "__main__":
    unittest.main()
