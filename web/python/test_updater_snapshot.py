import math
import unittest
from types import SimpleNamespace

import _manim_compat as compat
import _manim_typst as typst
import _manim_updaters as updaters


def _object(index: int) -> dict:
    return {
        "id": index,
        "geometry": {"circle": {"radius": 0.5}},
        "transform": {
            "translation": {"x": float(index), "y": 0.0},
            "rotation": 0.0,
            "scale": {"x": 1.0, "y": 1.0},
        },
        "style": {
            "fill": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0},
            "stroke": None,
            "stroke_width": 0.0,
            "stroke_join": "round",
            "stroke_cap": "round",
            "opacity": 1.0,
        },
    }


def _frame(count: int) -> dict:
    return {
        "time": 1.0,
        "delta_time": 1.0 / 60.0,
        "objects": [
            {
                "object": item["id"],
                "transform": item["transform"],
                "style": item["style"],
                "presence": True,
                "appearance": 1.0,
                "reveal": 1.0,
                "morph": 0.0,
            }
            for item in (_object(index) for index in range(count))
        ],
        "invocations": [{"callback": 0, "object_indices": list(range(count))}],
    }


class CallbackContextTests(unittest.TestCase):
    def test_large_snapshot_materializes_only_objects_actually_read(self) -> None:
        objects = [_object(index) for index in range(1000)]
        context = updaters._CallbackContext(SimpleNamespace(_objects=objects), _frame(1000))

        self.assertEqual(context._current, {})
        self.assertEqual(context._baseline, {})

        current = context.current_raw(777)
        self.assertEqual(current.transform["translation"]["x"], 777.0)
        self.assertEqual(set(context._current), {777})
        self.assertEqual(set(context._baseline), {777})
        self.assertEqual(context.patch_batch(0).to_document()["patches"], [])

    def test_replace_materializes_baseline_before_recording_mutation(self) -> None:
        objects = [_object(index) for index in range(8)]
        context = updaters._CallbackContext(SimpleNamespace(_objects=objects), _frame(8))
        current = context.current_raw(3)
        moved = type(current)(
            geometry=current.geometry,
            transform={
                **current.transform,
                "translation": {"x": 9.0, "y": -2.0},
            },
            style=current.style,
        )
        context.replace_raw(3, moved)

        batch = context.patch_batch(4).to_document()
        self.assertEqual(len(batch["patches"]), 1)
        self.assertEqual(batch["patches"][0]["set_transform"]["object"], 3)
        self.assertEqual(set(context._current), {3})
        self.assertEqual(set(context._baseline), {3})


class CanonicalCallbackPropertyRowTests(unittest.TestCase):
    def test_style_wire_uses_rust_default_and_preserves_explicit_mode(self) -> None:
        omitted_default = _object(0)["style"]
        self.assertNotIn("stroke_width_mode", omitted_default)

        default_style = updaters._PhaseStyle.from_wire(omitted_default)
        self.assertEqual(default_style.stroke_width_mode, "scale_with_object")
        self.assertEqual(
            default_style.to_wire()["stroke_width_mode"], "scale_with_object"
        )

        explicit_style = updaters._PhaseStyle.from_wire(
            {**omitted_default, "stroke_width_mode": "screen_space"}
        )
        self.assertEqual(explicit_style.stroke_width_mode, "screen_space")
        self.assertEqual(
            explicit_style.to_wire()["stroke_width_mode"], "screen_space"
        )

    @staticmethod
    def _mobject_and_context() -> tuple[object, object, object]:
        compat.install()
        if not updaters._INSTALLED:
            # Mirror the production final method that otherwise bypasses the
            # base Mobject patch. Updater installation must reclaim it.
            import _manim_semantic_handles as semantic_handles

            compat.VMobject.set_opacity = semantic_handles._set_opacity
            updaters.install()
        scene = updaters._base.Scene()
        mobject = compat.Circle(1.0)
        scene.add(mobject)
        mobject._semantic_handle = type(
            "SemanticHandle", (), {"semanticSlot": 11, "semanticGeneration": 3}
        )()
        frame = {
            "time": 0.5,
            "delta_time": 0.25,
            "token": {"runtime": 4, "publication": {}, "sequence": 8},
            "objects": [
                {
                    "node": {"slot": 11, "generation": 3},
                    "transform": {
                        "translation": {"x": 2.0, "y": -1.0},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": {
                        "fill": None,
                        "stroke": {
                            "red": 1.0,
                            "green": 1.0,
                            "blue": 1.0,
                            "alpha": 1.0,
                        },
                        "stroke_width": 1.0,
                        "stroke_width_mode": "scale_with_object",
                        "stroke_join": "miter",
                        "stroke_cap": "butt",
                        "opacity": 1.0,
                    },
                    "bounds": {
                        "min": {"x": 1.0, "y": -2.0},
                        "max": {"x": 3.0, "y": 0.0},
                    },
                }
            ],
        }
        class CallbackOperations:
            def __init__(self) -> None:
                self.rotations: list[tuple[float, ...]] = []
                self.paint_edits: list[tuple[str, tuple[object, ...]]] = []

            def callbackRotateTransformAboutPoint(self, *values: float):
                self.rotations.append(values)
                return SimpleNamespace(
                    translationX=1.0,
                    translationY=2.0,
                    rotation=math.pi / 2.0,
                    scaleX=1.0,
                    scaleY=1.0,
                )

            def callbackPaintSetColor(self, *values: object):
                self.paint_edits.append(("color", values))
                fill = values[:4]
                stroke = values[4:8]
                color = values[8:12]
                has_fill = fill[0] is not None
                has_stroke = stroke[0] is not None
                if not has_fill and not has_stroke:
                    fill = color
                    has_fill = True
                elif has_fill:
                    fill = (*color[:3], fill[3])
                if has_stroke:
                    stroke = (*color[:3], stroke[3])
                return self._paint_result(fill, stroke, has_fill, has_stroke)

            def callbackPaintSetFill(self, *values: object):
                self.paint_edits.append(("fill", values))
                fill = values[:4]
                stroke = values[4:8]
                color = values[8:12]
                opacity = values[12]
                has_fill = fill[0] is not None
                has_stroke = stroke[0] is not None
                has_color = color[0] is not None
                if has_color:
                    alpha = opacity if opacity is not None else (fill[3] if has_fill else color[3])
                    fill = (*color[:3], alpha)
                    has_fill = True
                elif opacity is not None:
                    fill = (*(fill[:3] if has_fill else (1.0, 1.0, 1.0)), opacity)
                    has_fill = True
                return self._paint_result(fill, stroke, has_fill, has_stroke)

            def callbackPaintSetStroke(self, *values: object):
                self.paint_edits.append(("stroke", values))
                fill = values[:4]
                stroke = values[4:8]
                color = values[8:12]
                has_fill = fill[0] is not None
                has_stroke = stroke[0] is not None
                stroke = (*color[:3], stroke[3] if has_stroke else color[3])
                return self._paint_result(fill, stroke, has_fill, True)

            @staticmethod
            def _paint_result(fill, stroke, has_fill: bool, has_stroke: bool):
                return SimpleNamespace(
                    hasFill=has_fill,
                    fillRed=fill[0],
                    fillGreen=fill[1],
                    fillBlue=fill[2],
                    fillAlpha=fill[3],
                    hasStroke=has_stroke,
                    strokeRed=stroke[0],
                    strokeGreen=stroke[1],
                    strokeBlue=stroke[2],
                    strokeAlpha=stroke[3],
                )

        operations = CallbackOperations()
        return scene, mobject, updaters._CanonicalCallbackContext(frame, operations)

    def test_callback_paint_uses_shared_rust_results_without_clobbering_domains(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        item = next(iter(context._frame_items.values()))
        item["style"]["fill"] = {
            "red": 0.1,
            "green": 0.2,
            "blue": 0.3,
            "alpha": 0.25,
        }
        item["style"]["stroke"]["alpha"] = 0.75
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            mobject.set_color(updaters._base.Color(0.8, 0.4, 0.2, 0.9))
            mobject.set_fill(opacity=0.4)
            row = next(iter(context._rows.values()))
            self.assertEqual(row.style.fill, (0.8, 0.4, 0.2, 0.4))
            self.assertEqual(row.style.stroke, (0.8, 0.4, 0.2, 0.75))
            self.assertEqual(row.style.opacity, 1.0)
            mobject.set_opacity(0.5)
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

        row = next(iter(context._rows.values()))
        self.assertEqual(row.style.fill, (0.8, 0.4, 0.2, 0.4))
        self.assertEqual(row.style.stroke, (0.8, 0.4, 0.2, 0.75))
        self.assertEqual(row.style.opacity, 0.5)
        self.assertEqual(
            [kind for kind, _ in context._operations.paint_edits],
            ["color", "fill"],
        )

    def test_unsupported_callback_stroke_opacity_fails_before_row_mutation(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            _, row = context.row(mobject)
            before = row.style
            with self.assertRaises(NotImplementedError):
                mobject.set_stroke(width=2.0, opacity=0.5)
            self.assertEqual(row.style, before)
            self.assertEqual(context.effective_batch()["writes"], [])
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

    def test_callback_stroke_color_uses_shared_rust_result(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        item = next(iter(context._frame_items.values()))
        item["style"]["fill"] = {
            "red": 0.1,
            "green": 0.2,
            "blue": 0.3,
            "alpha": 0.25,
        }
        item["style"]["stroke"]["alpha"] = 0.75
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            mobject.set_stroke(updaters._base.Color(0.2, 0.7, 0.4, 0.9))
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

        row = next(iter(context._rows.values()))
        self.assertEqual(row.style.fill, (0.1, 0.2, 0.3, 0.25))
        self.assertEqual(row.style.stroke, (0.2, 0.7, 0.4, 0.75))
        self.assertEqual(
            [kind for kind, _ in context._operations.paint_edits], ["stroke"]
        )

    def test_unsupported_callback_stroke_width_fails_before_row_mutation(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            _, row = context.row(mobject)
            before = row.style
            with self.assertRaises(NotImplementedError):
                mobject.set_stroke(width=2.0)
            self.assertEqual(row.style, before)
            self.assertEqual(context.effective_batch()["writes"], [])
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

    def test_callback_opacity_only_fill_enables_shared_white_paint(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            mobject.set_fill(opacity=0.35)
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

        row = next(iter(context._rows.values()))
        self.assertEqual(row.style.fill, (1.0, 1.0, 1.0, 0.35))
        self.assertEqual(row.style.opacity, 1.0)

    def test_translation_only_row_preserves_ordered_property_writes(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            self.assertEqual(updaters._canonical_callback_time(mobject), 0.5)
            self.assertEqual(mobject.get_center(), updaters._base.Vec2(2.0, -1.0))
            mobject.move_to((4.0, 3.0))
            mobject.set_opacity(0.5)
            mobject.set_color(updaters._base.BLUE)
            self.assertEqual(mobject.get_center(), updaters._base.Vec2(4.0, 3.0))
            mobject.shift((1.0, 0.0))
            self.assertEqual(mobject.get_center(), updaters._base.Vec2(5.0, 3.0))
            writes = context.effective_batch()["writes"]
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

        with self.assertRaises(RuntimeError):
            updaters._canonical_callback_time(mobject)

        self.assertIs(compat.VMobject.set_opacity, updaters._canonical_vmobject_set_opacity)
        self.assertEqual(
            [write["kind"] for write in writes],
            ["transform", "style", "style", "transform"],
        )
        self.assertEqual(
            writes[-1]["transform"]["translation"], {"x": 5.0, "y": 3.0}
        )
        row = next(iter(context._rows.values()))
        self.assertIsInstance(row, updaters._PhasePropertyRow)
        self.assertFalse(hasattr(row, "geometry"))

    def test_spatial_transform_and_raw_operations_fail_explicitly(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            with self.assertRaises(NotImplementedError):
                mobject.rotate(0.5)
            with self.assertRaises(NotImplementedError):
                mobject.scale(2.0)
            with self.assertRaises(NotImplementedError):
                mobject.width
            with self.assertRaises(NotImplementedError):
                mobject.geometry
            with self.assertRaises(NotImplementedError):
                mobject.copy()
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

    def test_explicit_pivot_rotation_dispatches_to_shared_rust_operation(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            updaters._canonical_rotate(
                mobject,
                math.pi / 2.0,
                (0.0, 0.0, 1.0),
                about_point=(0.0, 0.0, 0.0),
            )
            writes = context.effective_batch()["writes"]
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

        self.assertEqual(len(context._operations.rotations), 1)
        self.assertEqual(
            context._operations.rotations[0],
            (2.0, -1.0, 0.0, 1.0, 1.0, math.pi / 2.0, 0.0, 0.0),
        )
        self.assertEqual([write["kind"] for write in writes], ["transform"])
        self.assertEqual(writes[0]["transform"]["translation"], {"x": 1.0, "y": 2.0})
        self.assertAlmostEqual(writes[0]["transform"]["rotation"], math.pi / 2.0)
        self.assertFalse(next(iter(context._rows.values())).bounds_translation_only)

    def test_shared_line_rotate_about_origin_uses_the_callback_property_row(self) -> None:
        import _manim_shared_geometry

        scene, _, context = self._mobject_and_context()
        line = compat.Line((0.0, 0.0, 0.0), (1.0, 0.0, 0.0))
        scene.add(line)
        line._semantic_handle = type(
            "SemanticHandle", (), {"semanticSlot": 11, "semanticGeneration": 3}
        )()
        line._semantic_handle_fresh = True
        # Bound callback registrations deliberately make the ordinary handle
        # unavailable; the public shared-geometry wrapper must still reach the
        # phase property row rather than touching raw Line geometry.
        line._noon_updaters = []
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            _manim_shared_geometry._rotate_about_origin(line, math.pi / 2.0)
            writes = context.effective_batch()["writes"]
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

        self.assertEqual(len(context._operations.rotations), 1)
        self.assertEqual([write["kind"] for write in writes], ["transform"])
        self.assertEqual(writes[0]["transform"]["translation"], {"x": 1.0, "y": 2.0})
        self.assertAlmostEqual(writes[0]["transform"]["rotation"], math.pi / 2.0)

    def test_native_text_uses_the_same_effective_overlay_without_authored_writes(self) -> None:
        scene, _, context = self._mobject_and_context()

        class SemanticTextHandle:
            semanticSlot = 11
            semanticGeneration = 3

            def __init__(self) -> None:
                self.authored_revision = 17
                self.authored_translation = (2.0, -1.0)
                self.authored_opacity = 1.0
                self.calls: list[tuple[object, ...]] = []

            def shift(self, x: float, y: float) -> None:
                self.calls.append(("shift", x, y))
                self.authored_translation = (
                    self.authored_translation[0] + x,
                    self.authored_translation[1] + y,
                )
                self.authored_revision += 1

            def setObjectOpacity(self, opacity: float) -> None:
                self.calls.append(("setObjectOpacity", opacity))
                self.authored_opacity = opacity
                self.authored_revision += 1

        handle = SemanticTextHandle()
        text = object.__new__(typst.Text)
        text._scene = scene
        text._object = SimpleNamespace(id=0)
        text._semantic_handle = handle
        text._semantic_handle_fresh = True
        text._retained_handle = handle

        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
            self.assertEqual(text.get_center(), updaters._base.Vec2(2.0, -1.0))
            text.shift((0.25, 0.5))
            text.set_opacity(0.4)
            self.assertEqual(text.get_center(), updaters._base.Vec2(2.25, -0.5))
            with self.assertRaises(NotImplementedError):
                text.scale(2.0)
            with self.assertRaises(NotImplementedError):
                text.width
            writes = context.effective_batch()["writes"]
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

        self.assertEqual([write["kind"] for write in writes], ["transform", "style"])
        self.assertEqual(
            writes[0]["transform"]["translation"], {"x": 2.25, "y": -0.5}
        )
        self.assertEqual(writes[1]["style"]["opacity"], 0.4)
        self.assertEqual(handle.authored_translation, (2.0, -1.0))
        self.assertEqual(handle.authored_opacity, 1.0)
        self.assertEqual(handle.authored_revision, 17)
        self.assertEqual(handle.calls, [])


if __name__ == "__main__":
    unittest.main()
