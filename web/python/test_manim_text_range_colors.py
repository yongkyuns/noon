import unittest

import noon
import _manim_typst as typst


class _FillBatch:
    def __init__(self):
        self.selectors = []
        self.freed = False

    def appendSelector(self, selector, red, green, blue, alpha):
        self.selectors.append((selector, red, green, blue, alpha))

    def free(self):
        self.freed = True


class _Handle:
    def __init__(self):
        self.batches = []

    def textSourceFillBatch(self):
        batch = _FillBatch()
        self.batches.append(batch)
        return batch

    def applyTextSourceFills(self, batch):
        assert batch is self.batches[-1]


def _color_tuple(color):
    return (
        float(color.red),
        float(color.green),
        float(color.blue),
        float(color.alpha),
    )


def _install_text_constructor_stub(handle):
    original_new = typst._new_native_text_handle
    original_live = typst._live_text_context
    original_initialize = typst._RetainedTextMobject._initialize_text
    typst._new_native_text_handle = lambda *args: handle
    typst._live_text_context = lambda: None

    def initialize(instance, source, font_size, semantic_handle, color, opacity, **kwargs):
        del color, opacity, kwargs
        instance._raw = None
        instance._scene = None
        instance._object = None
        instance._source = source
        instance._font_size = font_size
        instance._semantic_handle = semantic_handle
        instance._semantic_handle_fresh = True

    typst._RetainedTextMobject._initialize_text = initialize
    return original_new, original_live, original_initialize


def _restore_text_constructor_stub(originals):
    (
        typst._new_native_text_handle,
        typst._live_text_context,
        typst._RetainedTextMobject._initialize_text,
    ) = originals


class ManimTextRangeColorTests(unittest.TestCase):
    def test_t2c_forwards_selectors_and_colors_to_one_typed_rust_batch(self):
        handle = _Handle()
        originals = _install_text_constructor_stub(handle)
        try:
            label = typst.Text(
                "source text deliberately irrelevant to Python selector handling",
                t2c={"[:4]": noon.RED, "opaque-selector": "#58C4DD"},
            )
        finally:
            _restore_text_constructor_stub(originals)

        self.assertIs(label._semantic_handle, handle)
        self.assertEqual(len(handle.batches), 1)
        selectors = handle.batches[0].selectors
        self.assertEqual([entry[0] for entry in selectors], ["[:4]", "opaque-selector"])
        self.assertEqual(selectors[0][1:], _color_tuple(noon.RED))
        self.assertEqual(selectors[1][1:], _color_tuple(noon.BLUE))
        self.assertTrue(handle.batches[0].freed)

    def test_text2color_long_form_overrides_t2c_without_python_selector_resolution(self):
        handle = _Handle()
        originals = _install_text_constructor_stub(handle)
        try:
            typst.Text(
                "abcdef",
                t2c={"[0:1]": noon.RED},
                text2color={"[-1:]": noon.BLUE},
            )
        finally:
            _restore_text_constructor_stub(originals)

        self.assertEqual(
            [entry[0] for entry in handle.batches[0].selectors],
            ["[-1:]"],
        )

    def test_explicit_none_text2color_matches_manim_non_none_assertion(self):
        handle = _Handle()
        originals = _install_text_constructor_stub(handle)
        try:
            with self.assertRaises(AssertionError):
                typst.Text(
                    "abcdef",
                    t2c={"[1:3]": noon.RED},
                    text2color=None,
                )
        finally:
            _restore_text_constructor_stub(originals)

        self.assertEqual(handle.batches, [])

    def test_styled_live_text_creation_rejects_before_bypassing_live_publication(self):
        class LiveContext:
            def __init__(self):
                self.created = False

            def liveCreateManimText(self, *args):
                self.created = True
                return _Handle()

        context = LiveContext()
        original_live = typst._live_text_context
        typst._live_text_context = lambda: context
        try:
            with self.assertRaisesRegex(NotImplementedError, "after live execution"):
                typst.Text("late", t2c={"late": noon.RED})
        finally:
            typst._live_text_context = original_live
        self.assertFalse(context.created)

    def test_t2c_rejects_invalid_python_color_shape_without_resolving_selector(self):
        handle = _Handle()
        originals = _install_text_constructor_stub(handle)
        try:
            with self.assertRaises(TypeError):
                typst.Text("abcdef", t2c={"[1:3]": object()})
        finally:
            _restore_text_constructor_stub(originals)
        self.assertEqual(handle.batches, [])


if __name__ == "__main__":
    unittest.main()
