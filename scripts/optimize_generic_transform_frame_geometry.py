from pathlib import Path

path = Path("crates/noon-runtime/src/lib.rs")
text = path.read_text()
old = '''    let before_object = frame.objects[object_index].clone();
    let before_morph = frame.morphs[object_index];
    let before_render_geometry = frame.render_geometries[object_index].clone();

    let object = &mut frame.objects[object_index];
    object.geometry = if progress >= 1.0 {
        to.geometry.clone()
    } else {
        from.geometry.clone()
    };
    object.transform = interpolate_transform(from.transform, to.transform, progress);
    object.style = interpolate_style(from.style, to.style, progress);
    let geometry_morphs = path_geometry_morphs(from, to);
    frame.morphs[object_index] = if geometry_morphs { progress } else { 0.0 };
    frame.render_geometries[object_index] = if geometry_morphs {
        track.transform_geometry.clone()
    } else {
        None
    };

    frame.objects[object_index] != before_object
        || frame.morphs[object_index] != before_morph
        || frame.render_geometries[object_index] != before_render_geometry
'''
new = '''    let before_transform = frame.objects[object_index].transform;
    let before_style = frame.objects[object_index].style;
    let before_morph = frame.morphs[object_index];
    let semantic_geometry = if progress >= 1.0 {
        &to.geometry
    } else {
        &from.geometry
    };
    let semantic_geometry_changed = frame.objects[object_index].geometry != *semantic_geometry;
    if semantic_geometry_changed {
        frame.objects[object_index].geometry = semantic_geometry.clone();
    }

    let object = &mut frame.objects[object_index];
    object.transform = interpolate_transform(from.transform, to.transform, progress);
    object.style = interpolate_style(from.style, to.style, progress);

    let geometry_morphs = path_geometry_morphs(from, to);
    frame.morphs[object_index] = if geometry_morphs { progress } else { 0.0 };
    let render_geometry_changed = if geometry_morphs {
        let prepared = track
            .transform_geometry
            .as_ref()
            .expect("compiled path Transform must carry prepared geometry");
        if frame.render_geometries[object_index].as_ref() != Some(prepared) {
            frame.render_geometries[object_index] = Some(prepared.clone());
            true
        } else {
            false
        }
    } else if frame.render_geometries[object_index].is_some() {
        frame.render_geometries[object_index] = None;
        true
    } else {
        false
    };

    semantic_geometry_changed
        || object.transform != before_transform
        || object.style != before_style
        || frame.morphs[object_index] != before_morph
        || render_geometry_changed
'''
if new in text:
    raise SystemExit(0)
if old not in text:
    raise SystemExit("reviewed Transform frame geometry fragment not found")
path.write_text(text.replace(old, new, 1))
