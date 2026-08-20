from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected review-fix fragment not found in {path}:\n{old[:500]}")
    file.write_text(text.replace(old, new, 1))


# Compiler: renderer-prepared geometry exists only for real path shape morphs.
# Any VectorPath stroke-width animation would otherwise retessellate every frame,
# and geometry-changing filled paths are unsupported by the current stroke-only
# fixed-topology morph mesh.
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''    if from.geometry == to.geometry {\n        return Ok(Some(from.geometry.clone()));\n    }\n\n    match (&from.geometry, &to.geometry) {\n        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {\n            if from.style.fill != to.style.fill\n                || from.style.stroke_width.to_bits() != to.style.stroke_width.to_bits()\n            {\n                return Err(TransformCompileFailure::RequiresRetessellation);\n            }\n            Ok(Some(GeometryRef::path(\n                source.clone().with_morph_target(target.clone()),\n            )))\n        }\n        _ => Err(TransformCompileFailure::UnsupportedGeometry),\n    }\n''',
    '''    if let (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_)) =\n        (&from.geometry, &to.geometry)\n    {\n        if from.style.stroke_width.to_bits() != to.style.stroke_width.to_bits() {\n            return Err(TransformCompileFailure::RequiresRetessellation);\n        }\n    }\n\n    if from.geometry == to.geometry {\n        return Ok(None);\n    }\n\n    match (&from.geometry, &to.geometry) {\n        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {\n            if from.style.fill.is_some() || to.style.fill.is_some() {\n                return Err(TransformCompileFailure::RequiresRetessellation);\n            }\n            Ok(Some(GeometryRef::path(\n                source.clone().with_morph_target(target.clone()),\n            )))\n        }\n        _ => Err(TransformCompileFailure::UnsupportedGeometry),\n    }\n''',
)

# Runtime semantic state remains an exact object snapshot. Renderer-prepared path
# geometry is a separate optional override so endpoint semantics do not leak a
# morph cache representation into FrameObjectState.geometry.
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''    /// Normalized per-object morph progress, independent from reveal.\n    pub morphs: Vec<f32>,\n}\n\nimpl FrameState {\n''',
    '''    /// Normalized per-object morph progress, independent from reveal.\n    pub morphs: Vec<f32>,\n    /// Optional compiler-prepared geometry used only by the renderer. Semantic\n    /// object geometry remains in `objects` and reaches exact Transform endpoints.\n    pub render_geometries: Vec<Option<GeometryRef>>,\n}\n\nimpl FrameState {\n''',
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''    pub fn morph(&self, object_index: usize) -> f32 {\n        self.morphs[object_index]\n    }\n}\n''',
    '''    pub fn morph(&self, object_index: usize) -> f32 {\n        self.morphs[object_index]\n    }\n\n    pub fn render_geometry(&self, object_index: usize) -> &GeometryRef {\n        self.render_geometries[object_index]\n            .as_ref()\n            .unwrap_or(&self.objects[object_index].geometry)\n    }\n}\n''',
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''        morphs: initial_scalar_property(compiled, objects.len(), Property::Morph, 0.0),\n        objects,\n''',
    '''        morphs: initial_scalar_property(compiled, objects.len(), Property::Morph, 0.0),\n        render_geometries: vec![None; objects.len()],\n        objects,\n''',
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''    let next_geometry = track\n        .transform_geometry\n        .as_ref()\n        .expect("compiled Transform track must carry prepared geometry");\n    let before_object = frame.objects[object_index].clone();\n    let before_morph = frame.morphs[object_index];\n\n    let object = &mut frame.objects[object_index];\n    if object.geometry != *next_geometry {\n        object.geometry = next_geometry.clone();\n    }\n    object.transform = interpolate_transform(from.transform, to.transform, progress);\n    object.style = interpolate_style(from.style, to.style, progress);\n    frame.morphs[object_index] = if path_geometry_morphs(from, to) {\n        progress\n    } else {\n        0.0\n    };\n\n    frame.objects[object_index] != before_object || frame.morphs[object_index] != before_morph\n''',
    '''    let before_object = frame.objects[object_index].clone();\n    let before_morph = frame.morphs[object_index];\n    let before_render_geometry = frame.render_geometries[object_index].clone();\n\n    let object = &mut frame.objects[object_index];\n    object.geometry = if progress >= 1.0 {\n        to.geometry.clone()\n    } else {\n        from.geometry.clone()\n    };\n    object.transform = interpolate_transform(from.transform, to.transform, progress);\n    object.style = interpolate_style(from.style, to.style, progress);\n    let geometry_morphs = path_geometry_morphs(from, to);\n    frame.morphs[object_index] = if geometry_morphs { progress } else { 0.0 };\n    frame.render_geometries[object_index] = if geometry_morphs {\n        track.transform_geometry.clone()\n    } else {\n        None\n    };\n\n    frame.objects[object_index] != before_object\n        || frame.morphs[object_index] != before_morph\n        || frame.render_geometries[object_index] != before_render_geometry\n''',
)

# Renderer batches/tessellates the prepared override, while packing transform/style
# from the semantic FrameObjectState.
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        for (object_index, object) in frame.objects.iter().enumerate() {\n            match &object.geometry {\n''',
    '''        for (object_index, object) in frame.objects.iter().enumerate() {\n            let render_geometry = frame.render_geometry(object_index);\n            match render_geometry {\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        let Some(object) = frame.objects.get(object_index) else {\n            return false;\n        };\n        match self.slots.get(object_index) {\n            Some(PreparedSlot::Circle(index)) => {\n                matches!(object.geometry, GeometryRef::Circle { .. })\n''',
    '''        let Some(object) = frame.objects.get(object_index) else {\n            return false;\n        };\n        let render_geometry = frame.render_geometry(object_index);\n        match self.slots.get(object_index) {\n            Some(PreparedSlot::Circle(index)) => {\n                matches!(render_geometry, GeometryRef::Circle { .. })\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''            Some(PreparedSlot::Rectangle(index)) => {\n                matches!(object.geometry, GeometryRef::Rectangle { .. })\n''',
    '''            Some(PreparedSlot::Rectangle(index)) => {\n                matches!(render_geometry, GeometryRef::Rectangle { .. })\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''            Some(PreparedSlot::Line(index)) => {\n                matches!(object.geometry, GeometryRef::Line { .. })\n''',
    '''            Some(PreparedSlot::Line(index)) => {\n                matches!(render_geometry, GeometryRef::Line { .. })\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''            Some(PreparedSlot::Path { index, batch }) => {\n                let GeometryRef::VectorPath(path) = &object.geometry else {\n''',
    '''            Some(PreparedSlot::Path { index, batch }) => {\n                let GeometryRef::VectorPath(path) = render_geometry else {\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''            Some(PreparedSlot::Unsupported(index)) => {\n                matches!(object.geometry, GeometryRef::External(_))\n''',
    '''            Some(PreparedSlot::Unsupported(index)) => {\n                matches!(render_geometry, GeometryRef::External(_))\n''',
)
