from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected clone-fix fragment not found in {path}:\n{old[:500]}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''    let progress = track_progress(track, time);\n    let before_object = frame.objects[object_index].clone();\n    let before_morph = frame.morphs[object_index];\n    let before_render_geometry = frame.render_geometries[object_index].clone();\n\n    let object = &mut frame.objects[object_index];\n    object.geometry = if progress >= 1.0 {\n        to.geometry.clone()\n    } else {\n        from.geometry.clone()\n    };\n    object.transform = interpolate_transform(from.transform, to.transform, progress);\n    object.style = interpolate_style(from.style, to.style, progress);\n    let geometry_morphs = path_geometry_morphs(from, to);\n    frame.morphs[object_index] = if geometry_morphs { progress } else { 0.0 };\n    frame.render_geometries[object_index] = if geometry_morphs {\n        track.transform_geometry.clone()\n    } else {\n        None\n    };\n\n    frame.objects[object_index] != before_object\n        || frame.morphs[object_index] != before_morph\n        || frame.render_geometries[object_index] != before_render_geometry\n}\n\nfn path_geometry_morphs''',
    '''    let progress = track_progress(track, time);\n    let semantic_geometry = if progress >= 1.0 {\n        &to.geometry\n    } else {\n        &from.geometry\n    };\n    let next_transform = interpolate_transform(from.transform, to.transform, progress);\n    let next_style = interpolate_style(from.style, to.style, progress);\n    let geometry_morphs = path_geometry_morphs(from, to);\n    let next_morph = if geometry_morphs { progress } else { 0.0 };\n    let next_render_geometry = if geometry_morphs {\n        track.transform_geometry.as_ref()\n    } else {\n        None\n    };\n\n    let object = &mut frame.objects[object_index];\n    let mut changed = set_geometry_if_changed(&mut object.geometry, semantic_geometry);\n    if object.transform != next_transform {\n        object.transform = next_transform;\n        changed = true;\n    }\n    if object.style != next_style {\n        object.style = next_style;\n        changed = true;\n    }\n    if frame.morphs[object_index] != next_morph {\n        frame.morphs[object_index] = next_morph;\n        changed = true;\n    }\n    changed |= set_optional_geometry_if_changed(\n        &mut frame.render_geometries[object_index],\n        next_render_geometry,\n    );\n    changed\n}\n\nfn set_geometry_if_changed(current: &mut GeometryRef, next: &GeometryRef) -> bool {\n    if current == next {\n        return false;\n    }\n    current.clone_from(next);\n    true\n}\n\nfn set_optional_geometry_if_changed(\n    current: &mut Option<GeometryRef>,\n    next: Option<&GeometryRef>,\n) -> bool {\n    match next {\n        Some(next) if current.as_ref() == Some(next) => false,\n        Some(next) => {\n            if let Some(current) = current.as_mut() {\n                current.clone_from(next);\n            } else {\n                *current = Some(next.clone());\n            }\n            true\n        }\n        None if current.is_some() => {\n            *current = None;\n            true\n        }\n        None => false,\n    }\n}\n\nfn path_geometry_morphs''',
)

runtime_test = Path("crates/noon-runtime/tests/generic_transform.rs")
text = runtime_test.read_text()
marker = '''#[test]\nfn direct_seek_and_forward_playback_match_for_generic_transform()'''
test = r'''fn path_command_buffers(
    frame: &noon_runtime::FrameState,
) -> (
    *const noon_core::PathCommand,
    *const noon_core::PathCommand,
    *const noon_core::PathCommand,
) {
    let GeometryRef::VectorPath(semantic) = &frame.objects[0].geometry else {
        panic!("expected semantic path");
    };
    let GeometryRef::VectorPath(render) = frame.render_geometry(0) else {
        panic!("expected prepared render path");
    };
    let target = render.morph_target().expect("prepared path must carry target");
    (
        semantic.commands().as_ptr(),
        render.commands().as_ptr(),
        target.commands().as_ptr(),
    )
}

#[test]
fn steady_generic_transform_reuses_path_allocations() {
    let style = stroke_style(Color::WHITE);
    let from = snapshot(path_a(), Transform2D::IDENTITY, style);
    let to = snapshot(path_b(), Transform2D::IDENTITY, style);
    let mut scene = SceneDefinition::new();
    let object = scene.add(from.geometry.clone());
    scene.object_mut(object).unwrap().style = style;
    scene
        .animate_transform(
            object,
            from,
            to,
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    instance.advance_to(0.25).unwrap();
    let first = path_command_buffers(instance.frame());
    instance.advance_to(0.50).unwrap();
    let second = path_command_buffers(instance.frame());
    instance.advance_to(0.75).unwrap();
    let third = path_command_buffers(instance.frame());

    assert_eq!(first, second);
    assert_eq!(second, third);
}

#[test]
fn direct_seek_and_forward_playback_match_for_generic_transform()'''
if "steady_generic_transform_reuses_path_allocations" not in text:
    if marker not in text:
        raise SystemExit("generic Transform runtime test insertion marker not found")
    runtime_test.write_text(text.replace(marker, test, 1))
