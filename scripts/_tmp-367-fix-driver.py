from pathlib import Path

path = Path("scripts/_tmp-367-canonical-scene-context.py")
text = path.read_text()
start = text.index('replace(\n    "web/python/_manim_typst.py",')
end = text.index('\n\nreplace(\n    "web/python/_manim_retained_state.py",', start)
replacement = r"""replace(
    "web/python/_manim_typst.py",
    '''    def _bind_to_scene(\n        self, scene: _compat.Scene, *, key: str | None = None\n    ) -> object:\n        if self._scene is scene and self._object is not None:\n            return self._object\n        if self._scene is not None:\n            raise ValueError("retained text Mobject already belongs to another Scene")\n        _ensure_scene_state(scene)\n        obj, order = scene._allocate_object(key)\n        self._bind_retained(scene, obj, order)\n        scene._retained_text_objects.append(self)\n        return obj\n''',
    '''    def _bind_to_scene(\n        self, scene: _compat.Scene, *, key: str | None = None\n    ) -> object:\n        if self._scene is scene and self._object is not None:\n            return self._object\n        if self._scene is not None:\n            raise ValueError("retained text Mobject already belongs to another Scene")\n        _ensure_scene_state(scene)\n        checkpoint = scene._authoring_checkpoint()\n        try:\n            obj, order = scene._allocate_object(key)\n            scene._canonical_bind_retained_text_spec(\n                obj, str(self._retained_handle.specJson())\n            )\n            self._bind_retained(scene, obj, order)\n            scene._retained_text_objects.append(self)\n            return obj\n        except Exception:\n            scene._restore_authoring_checkpoint(checkpoint)\n            raise\n''',
)"""
path.write_text(text[:start] + replacement + text[end:])
