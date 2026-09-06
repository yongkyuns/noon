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
        return scene, mobject, updaters._CanonicalCallbackContext(frame)

    def test_translation_only_row_preserves_ordered_property_writes(self) -> None:
        scene, mobject, context = self._mobject_and_context()
        updaters._ACTIVE_CONTEXTS[id(scene)] = context
        try:
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

        self.assertIs(compat.VMobject.set_opacity, updaters._canonical_vmobject_set_opacity)
        self.assertEqual(
            [write["kind"] for write in writes],
            ["transform", "style", "style", "transform"],
        )
        self.assertEqual(
            writes[-1]["transform"]["translation"], {"x": 5.0, "y": 3.0}
        )
        self.assertEqual(writes[1]["style"]["opacity"], 1.0)
        self.assertEqual(writes[1]["style"]["stroke"]["alpha"], 0.5)
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
        self.assertEqual(writes[1]["style"]["opacity"], 1.0)
        self.assertEqual(writes[1]["style"]["stroke"]["alpha"], 0.4)
        self.assertEqual(handle.authored_translation, (2.0, -1.0))
        self.assertEqual(handle.authored_opacity, 1.0)
        self.assertEqual(handle.authored_revision, 17)
        self.assertEqual(handle.calls, [])


if __name__ == "__main__":
    unittest.main()
