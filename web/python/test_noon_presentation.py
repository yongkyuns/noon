"""Presentation facade routing; Rust owns all preset values and role admission."""
import copy
import sys
import types
import unittest
from unittest.mock import Mock, patch

import noon
from _noon_presentation import Theme


def color_handle():
    return types.SimpleNamespace(red=0.125, green=0.25, blue=0.5, alpha=0.75, free=Mock())


class PresentationFacadeTests(unittest.TestCase):
    def setUp(self):
        self.definition = Mock()
        self.definition.categoricalCount = 7
        self.make_theme = Mock(return_value=self.definition)
        js = types.ModuleType("js")
        js.noonTheme = self.make_theme
        context = patch.dict(sys.modules, {"js": js})
        context.__enter__()
        self.addCleanup(context.__exit__, None, None, None)

    def style(self):
        value = types.SimpleNamespace(
            fontFamily="A shared fixture font", fontSize=71.25, lineSpacing=0.375,
            color=color_handle(), free=Mock(),
        )
        self.definition.textStyle.return_value = value
        return value

    def test_public_export_is_lazy_and_presets_delegate_to_shared_rust(self):
        self.assertIs(noon.Theme, Theme)
        self.assertIn("Theme", noon.__all__)
        Theme.dark()
        Theme.light()
        self.assertEqual([call.args for call in self.make_theme.call_args_list], [("dark",), ("light",)])

    def test_theme_is_immutable_and_copy_preserves_its_value_handle(self):
        theme = Theme.dark()
        self.assertIs(copy.copy(theme), theme)
        self.assertIs(copy.deepcopy(theme), theme)
        for name in ("color", "_definition", "custom"):
            with self.subTest(name=name), self.assertRaises(AttributeError):
                setattr(theme, name, object())
        with self.assertRaises(AttributeError):
            del theme._definition
        with self.assertRaises(AttributeError):
            theme.__init__("light")

    def test_colors_are_value_snapshots_and_spacing_comes_from_rust(self):
        theme = Theme.dark()
        color = color_handle()
        self.definition.color.return_value = color
        self.assertEqual(theme.color("foreground"), noon.Color(0.125, 0.25, 0.5, 0.75))
        self.definition.color.assert_called_once_with("foreground")
        color.free.assert_called_once_with()
        self.definition.spacing.return_value = 0.345
        self.assertEqual(theme.spacing("normal"), 0.345)
        self.definition.spacing.assert_called_once_with("normal")

    def test_category_index_coercion_does_not_cycle_or_validate_palette_in_python(self):
        theme = Theme.dark()
        color = color_handle()
        self.definition.categorical.return_value = color
        self.assertEqual(theme.categorical_count, 7)
        self.assertEqual(theme.categorical(3), noon.Color(0.125, 0.25, 0.5, 0.75))
        self.definition.categorical.assert_called_once_with(3.0)
        color.free.assert_called_once_with()
        self.definition.categorical.side_effect = IndexError("shared finite-palette rejection")
        with self.assertRaises(IndexError):
            theme.categorical(7)
        self.definition.categorical.assert_called_with(7.0)

    def test_python_argument_shapes_reject_before_optional_boundary(self):
        theme = Theme.dark()
        for name in (None, 12, object()):
            for operation in (theme.color, theme.spacing):
                with self.subTest(name=name, operation=operation), self.assertRaises(TypeError):
                    operation(name)
            with self.assertRaises(TypeError):
                theme.text("source", role=name)
        for index in (True, False, 1.25, "2"):
            with self.subTest(index=index), self.assertRaises(TypeError):
                theme.categorical(index)
        self.definition.color.assert_not_called()
        self.definition.spacing.assert_not_called()
        self.definition.textStyle.assert_not_called()
        self.definition.categorical.assert_not_called()

    def test_unrepresentable_python_integer_is_a_stable_input_error(self):
        theme = Theme.dark()
        for value in (10 ** 1000, -(10 ** 1000)):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "host numeric range"):
                theme.categorical(value)
        self.definition.categorical.assert_not_called()

    def test_unknown_names_and_numeric_range_failures_come_from_shared_rust(self):
        failure = ValueError("shared presentation admission")
        self.make_theme.side_effect = failure
        with self.assertRaises(ValueError) as caught:
            Theme("future-preset")
        self.assertIs(caught.exception, failure)
        self.make_theme.side_effect = None
        theme = Theme.dark()
        for operation in ("color", "spacing", "textStyle"):
            getattr(self.definition, operation).side_effect = failure
        for call in (lambda: theme.color("future-color"), lambda: theme.spacing("future-space"),
                     lambda: theme.text("source", role="future-role")):
            with self.assertRaises(ValueError) as caught:
                call()
            self.assertIs(caught.exception, failure)
        self.definition.categorical.side_effect = failure
        with self.assertRaises(ValueError) as caught:
            theme.categorical(-1)
        self.assertIs(caught.exception, failure)
        self.definition.categorical.assert_called_once_with(-1.0)

    def test_text_uses_returned_defaults_and_returns_the_existing_text_object(self):
        theme = Theme.dark()
        style = self.style()
        text = object()
        with patch.object(noon, "Text", return_value=text) as constructor:
            self.assertIs(theme.text("meaning", role="caption"), text)
        self.definition.textStyle.assert_called_once_with("caption")
        constructor.assert_called_once_with(
            "meaning", font="A shared fixture font", font_size=71.25,
            line_spacing=0.375, color=noon.Color(0.125, 0.25, 0.5, 0.75),
        )
        style.free.assert_called_once_with()
        style.color.free.assert_called_once_with()

    def test_explicit_text_fields_override_only_the_given_defaults(self):
        theme = Theme.dark()
        style = self.style()
        face = object()
        colors = {"meaning": noon.RED}
        with patch.object(noon, "Text") as constructor:
            theme.text("meaning", font=face, font_size=19, color=noon.RED,
                       t2c=colors, opacity=0.4)
        self.definition.textStyle.assert_called_once_with("body")
        constructor.assert_called_once_with(
            "meaning", font=face, font_size=19, line_spacing=0.375,
            color=noon.RED, t2c=colors, opacity=0.4,
        )
        style.free.assert_called_once_with()
        style.color.free.assert_called_once_with()

    def test_text_validation_is_delegated_and_temporary_style_values_are_released(self):
        theme = Theme.dark()
        style = self.style()
        failure = ValueError("ordinary Text validation")
        with patch.object(noon, "Text", side_effect=failure) as constructor:
            with self.assertRaises(ValueError) as caught:
                theme.text("meaning", font_size=None, unsupported_option=True)
        self.assertIs(caught.exception, failure)
        self.assertIsNone(constructor.call_args.kwargs["font_size"])
        self.assertTrue(constructor.call_args.kwargs["unsupported_option"])
        style.free.assert_called_once_with()
        style.color.free.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
