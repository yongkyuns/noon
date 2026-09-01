from pathlib import Path

import _noon_ir as _ir


def test_scene_allocator_is_shared_across_content_projections():
    scene = _ir.Scene()
    first = scene.add(_ir.Circle(1.0))
    retained, retained_order = scene._allocate_object()
    third = scene.add(_ir.Circle(2.0))

    assert (first.id, retained.id, third.id) == (0, 1, 2)
    assert retained_order == 1
    assert [obj["id"] for obj in scene.to_document()["objects"]] == [0, 2]
    assert [entry["id"] for entry in scene.identity_document()["objects"]] == [0, 1, 2]


def test_retained_content_modules_do_not_own_scene_lifecycle():
    root = Path(__file__).parent
    typst = (root / "_manim_typst.py").read_text()
    retained_animation = (root / "_manim_retained_animate.py").read_text()

    forbidden = (
        "_compat.Scene.add =",
        "_compat.Scene.remove =",
        "_compat.Scene._is_present =",
    )
    for source in (typst, retained_animation):
        for assignment in forbidden:
            assert assignment not in source


def test_phase_b_binding_is_content_polymorphic():
    source = (Path(__file__).parent / "_manim_phase_b.py").read_text()
    assert "member._bind_to_scene(scene, key=key)" in source
    assert "_base.Scene.add(scene, raw" not in source
