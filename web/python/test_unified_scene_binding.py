from pathlib import Path

import _noon_ir as _ir
import _manim_compat as _compat
import _manim_typst as _typst
import noon as _base


def test_scene_allocator_is_shared_across_content_projections():
    scene = _ir.Scene()
    first = scene.add(_ir.Circle(1.0))
    retained, retained_order = scene._allocate_object()
    third = scene.add(_ir.Circle(2.0))

    assert (first.id, retained.id, third.id) == (0, 1, 2)
    assert retained_order == 1
    assert [obj["id"] for obj in scene.to_document()["objects"]] == [0, 2]
    assert [entry["id"] for entry in scene.identity_document()["objects"]] == [0, 2]


def test_phase_b_binding_is_content_polymorphic():
    source = (Path(__file__).parent / "_manim_phase_b.py").read_text()
    assert "member._bind_to_scene(scene, key=key)" in source
    assert "_base.Scene.add(scene, raw" not in source


def test_text_construction_requires_shared_rust_authoring_runtime():
    _compat.install()
    _typst.install()
    try:
        _typst.Text("AB")
    except RuntimeError as error:
        assert str(error) == "Text requires Noon's shared Rust authoring runtime"
    else:
        raise AssertionError("CPython must not construct a second Text scene authority")
