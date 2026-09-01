from pathlib import Path

ir = Path("web/python/_noon_ir.py")
text = ir.read_text()
old = """                for object_id, key in self._object_keys.items()
            ],"""
new = """                for object_id, key in self._object_keys.items()
                if object_id in self._object_positions
            ],"""
if text.count(old) != 1:
    raise RuntimeError("identity projection anchor must match exactly once")
ir.write_text(text.replace(old, new, 1))

test = Path("web/python/test_unified_scene_binding.py")
text = test.read_text()
old = """    assert [entry[\"id\"] for entry in scene.identity_document()[\"objects\"]] == [0, 1, 2]
"""
new = """    # The legacy identity payload remains definition-scoped even though retained
    # content shares the same scene-global allocation domain.
    assert [entry[\"id\"] for entry in scene.identity_document()[\"objects\"]] == [0, 2]
"""
if text.count(old) != 1:
    raise RuntimeError("mixed identity assertion anchor must match exactly once")
test.write_text(text.replace(old, new, 1))
