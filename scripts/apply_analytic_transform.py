from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected analytic-Transform fragment not found in {path}:\n{old[:700]}")
    file.write_text(text.replace(old, new, 1))


# Compiler: replace the path-only renderer override with an explicit geometry plan.
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''#[derive(Clone, Debug, PartialEq)]\npub struct CompiledTrack {\n    pub id: TrackId,\n    pub object_index: u32,\n    pub property: Property,\n    pub values: TrackValues,\n    pub timing: TrackTiming,\n    /// Stable geometry used by an atomic Transform. For path morphing this is\n    /// the source path carrying its target correspondence; it does not change\n    /// during steady-state playback.\n    pub transform_geometry: Option<GeometryRef>,\n}\n''',
    '''#[derive(Clone, Debug, PartialEq)]\npub enum TransformGeometryPlan {\n    Static,\n    Circle {\n        from_radius: f32,\n        to_radius: f32,\n    },\n    Rectangle {\n        from_size: noon_core::Vec2,\n        to_size: noon_core::Vec2,\n    },\n    Line {\n        from_start: noon_core::Vec2,\n        from_end: noon_core::Vec2,\n        to_start: noon_core::Vec2,\n        to_end: noon_core::Vec2,\n    },\n    /// Fixed source/target topology prepared once for the path renderer.\n    PathPair(GeometryRef),\n}\n\n#[derive(Clone, Debug, PartialEq)]\npub struct CompiledTrack {\n    pub id: TrackId,\n    pub object_index: u32,\n    pub property: Property,\n    pub values: TrackValues,\n    pub timing: TrackTiming,\n    /// Compiler-selected geometry interpolation strategy for an atomic Transform.\n    /// Non-Transform tracks carry `None`.\n    pub transform_geometry_plan: Option<TransformGeometryPlan>,\n}\n''',
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''        timing: track.timing,\n        transform_geometry: compile_transform_geometry(track)?,\n    })\n}\n\nfn compile_transform_geometry(\n    track: &TrackDefinition,\n) -> Result<Option<GeometryRef>, TransformCompileFailure> {\n    if track.property != Property::Transform {\n        return Ok(None);\n    }\n    let TrackValues::Object { from, to } = &track.values else {\n        unreachable!("validated Transform track must contain object snapshots");\n    };\n\n    if let (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_)) = (&from.geometry, &to.geometry)\n    {\n        if from.style.stroke_width.to_bits() != to.style.stroke_width.to_bits() {\n            return Err(TransformCompileFailure::RequiresRetessellation);\n        }\n    }\n\n    if from.geometry == to.geometry {\n        return Ok(None);\n    }\n\n    match (&from.geometry, &to.geometry) {\n        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {\n            if from.style.fill.is_some() || to.style.fill.is_some() {\n                return Err(TransformCompileFailure::RequiresRetessellation);\n            }\n            Ok(Some(GeometryRef::path(\n                source.clone().with_morph_target(target.clone()),\n            )))\n        }\n        _ => Err(TransformCompileFailure::UnsupportedGeometry),\n    }\n}\n''',
    '''        timing: track.timing,\n        transform_geometry_plan: compile_transform_geometry_plan(track)?,\n    })\n}\n\nfn compile_transform_geometry_plan(\n    track: &TrackDefinition,\n) -> Result<Option<TransformGeometryPlan>, TransformCompileFailure> {\n    if track.property != Property::Transform {\n        return Ok(None);\n    }\n    let TrackValues::Object { from, to } = &track.values else {\n        unreachable!("validated Transform track must contain object snapshots");\n    };\n\n    if let (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_)) = (&from.geometry, &to.geometry)\n    {\n        if from.style.stroke_width.to_bits() != to.style.stroke_width.to_bits() {\n            return Err(TransformCompileFailure::RequiresRetessellation);\n        }\n    }\n\n    if from.geometry == to.geometry {\n        return Ok(Some(TransformGeometryPlan::Static));\n    }\n\n    let plan = match (&from.geometry, &to.geometry) {\n        (\n            GeometryRef::Circle { radius: from_radius },\n            GeometryRef::Circle { radius: to_radius },\n        ) => TransformGeometryPlan::Circle {\n            from_radius: *from_radius,\n            to_radius: *to_radius,\n        },\n        (\n            GeometryRef::Rectangle { size: from_size },\n            GeometryRef::Rectangle { size: to_size },\n        ) => TransformGeometryPlan::Rectangle {\n            from_size: *from_size,\n            to_size: *to_size,\n        },\n        (\n            GeometryRef::Line {\n                start: from_start,\n                end: from_end,\n            },\n            GeometryRef::Line {\n                start: to_start,\n                end: to_end,\n            },\n        ) => TransformGeometryPlan::Line {\n            from_start: *from_start,\n            from_end: *from_end,\n            to_start: *to_start,\n            to_end: *to_end,\n        },\n        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {\n            if from.style.fill.is_some() || to.style.fill.is_some() {\n                return Err(TransformCompileFailure::RequiresRetessellation);\n            }\n            TransformGeometryPlan::PathPair(GeometryRef::path(\n                source.clone().with_morph_target(target.clone()),\n            ))\n        }\n        _ => return Err(TransformCompileFailure::UnsupportedGeometry),\n    };\n    Ok(Some(plan))\n}\n''',
)

# Runtime: interpolate tiny analytic geometry values directly; only PathPair gets a renderer override.
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''use noon_compile::{CompilePatchError, CompiledScene, CompiledTrack};\n''',
    '''use noon_compile::{\n    CompilePatchError, CompiledScene, CompiledTrack, TransformGeometryPlan,\n};\n''',
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''    let progress = track_progress(track, time);\n    let semantic_geometry = if progress >= 1.0 {\n        &to.geometry\n    } else {\n        &from.geometry\n    };\n    let next_transform = interpolate_transform(from.transform, to.transform, progress);\n    let next_style = interpolate_style(from.style, to.style, progress);\n    let geometry_morphs = path_geometry_morphs(from, to);\n    let next_morph = if geometry_morphs { progress } else { 0.0 };\n    let next_render_geometry = if geometry_morphs {\n        track.transform_geometry.as_ref()\n    } else {\n        None\n    };\n\n    let object = &mut frame.objects[object_index];\n    let mut changed = set_geometry_if_changed(&mut object.geometry, semantic_geometry);\n    if object.transform != next_transform {\n        object.transform = next_transform;\n        changed = true;\n    }\n    if object.style != next_style {\n        object.style = next_style;\n        changed = true;\n    }\n    if frame.morphs[object_index] != next_morph {\n        frame.morphs[object_index] = next_morph;\n        changed = true;\n    }\n    changed |= set_optional_geometry_if_changed(\n        &mut frame.render_geometries[object_index],\n        next_render_geometry,\n    );\n    changed\n}\n''',
    '''    let progress = track_progress(track, time);\n    let plan = track\n        .transform_geometry_plan\n        .as_ref()\n        .expect("compiled Transform track must carry a geometry plan");\n    let next_transform = interpolate_transform(from.transform, to.transform, progress);\n    let next_style = interpolate_style(from.style, to.style, progress);\n    let next_morph = if matches!(plan, TransformGeometryPlan::PathPair(_)) {\n        progress\n    } else {\n        0.0\n    };\n    let next_render_geometry = match plan {\n        TransformGeometryPlan::PathPair(prepared) => Some(prepared),\n        _ => None,\n    };\n\n    let object = &mut frame.objects[object_index];\n    let mut changed = apply_transform_geometry(&mut object.geometry, plan, from, to, progress);\n    if object.transform != next_transform {\n        object.transform = next_transform;\n        changed = true;\n    }\n    if object.style != next_style {\n        object.style = next_style;\n        changed = true;\n    }\n    if frame.morphs[object_index] != next_morph {\n        frame.morphs[object_index] = next_morph;\n        changed = true;\n    }\n    changed |= set_optional_geometry_if_changed(\n        &mut frame.render_geometries[object_index],\n        next_render_geometry,\n    );\n    changed\n}\n\nfn apply_transform_geometry(\n    current: &mut GeometryRef,\n    plan: &TransformGeometryPlan,\n    from: &ObjectSnapshot,\n    to: &ObjectSnapshot,\n    progress: f32,\n) -> bool {\n    match plan {\n        TransformGeometryPlan::Static => set_geometry_if_changed(current, &from.geometry),\n        TransformGeometryPlan::Circle {\n            from_radius,\n            to_radius,\n        } => {\n            let next = lerp(*from_radius, *to_radius, progress);\n            match current {\n                GeometryRef::Circle { radius } if *radius == next => false,\n                GeometryRef::Circle { radius } => {\n                    *radius = next;\n                    true\n                }\n                _ => {\n                    *current = GeometryRef::circle(next);\n                    true\n                }\n            }\n        }\n        TransformGeometryPlan::Rectangle { from_size, to_size } => {\n            let next = interpolate_vec2(*from_size, *to_size, progress);\n            match current {\n                GeometryRef::Rectangle { size } if *size == next => false,\n                GeometryRef::Rectangle { size } => {\n                    *size = next;\n                    true\n                }\n                _ => {\n                    *current = GeometryRef::rectangle(next.x, next.y);\n                    true\n                }\n            }\n        }\n        TransformGeometryPlan::Line {\n            from_start,\n            from_end,\n            to_start,\n            to_end,\n        } => {\n            let next_start = interpolate_vec2(*from_start, *to_start, progress);\n            let next_end = interpolate_vec2(*from_end, *to_end, progress);\n            match current {\n                GeometryRef::Line { start, end } if *start == next_start && *end == next_end => false,\n                GeometryRef::Line { start, end } => {\n                    *start = next_start;\n                    *end = next_end;\n                    true\n                }\n                _ => {\n                    *current = GeometryRef::line(next_start, next_end);\n                    true\n                }\n            }\n        }\n        TransformGeometryPlan::PathPair(_) => {\n            let semantic_geometry = if progress >= 1.0 {\n                &to.geometry\n            } else {\n                &from.geometry\n            };\n            set_geometry_if_changed(current, semantic_geometry)\n        }\n    }\n}\n\nfn interpolate_vec2(from: Vec2, to: Vec2, progress: f32) -> Vec2 {\n    Vec2::new(\n        lerp(from.x, to.x, progress),\n        lerp(from.y, to.y, progress),\n    )\n}\n''',
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    '''fn path_geometry_morphs(from: &ObjectSnapshot, to: &ObjectSnapshot) -> bool {\n    from.geometry != to.geometry\n        && matches!(\n            (&from.geometry, &to.geometry),\n            (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_))\n        )\n}\n\n''',
    '''''',
)

# Existing path compiler tests now assert the explicit plan.
replace_once(
    "crates/noon-compile/tests/generic_transform.rs",
    '''use noon_compile::{CompileError, CompiledScene};\n''',
    '''use noon_compile::{CompileError, CompiledScene, TransformGeometryPlan};\n''',
)
replace_once(
    "crates/noon-compile/tests/generic_transform.rs",
    '''    let GeometryRef::VectorPath(prepared) = compiled_track.transform_geometry.as_ref().unwrap()\n    else {\n        panic!("path Transform must carry a prepared path pair");\n    };\n''',
    '''    let Some(TransformGeometryPlan::PathPair(GeometryRef::VectorPath(prepared))) =\n        compiled_track.transform_geometry_plan.as_ref()\n    else {\n        panic!("path Transform must carry a prepared path pair");\n    };\n''',
)
replace_once(
    "crates/noon-compile/tests/generic_transform.rs",
    '''    assert!(compiled.tracks()[0].transform_geometry.is_none());\n''',
    '''    assert!(matches!(\n        compiled.tracks()[0].transform_geometry_plan,\n        Some(TransformGeometryPlan::Static)\n    ));\n''',
)

Path("crates/noon-compile/tests/analytic_transform.rs").write_text(r'''use noon_compile::{CompiledScene, TransformGeometryPlan};
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming, Transform2D, Vec2,
};

fn add_transform(scene: &mut SceneDefinition, from: GeometryRef, to: GeometryRef) {
    let object = scene.add(from.clone());
    scene
        .animate_transform(
            object,
            ObjectSnapshot {
                geometry: from,
                transform: Transform2D::IDENTITY,
                style: scene.object(object).unwrap().style,
            },
            ObjectSnapshot {
                geometry: to,
                transform: Transform2D::IDENTITY,
                style: scene.object(object).unwrap().style,
            },
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();
}

#[test]
fn compiler_selects_analytic_geometry_plans() {
    let mut scene = SceneDefinition::new();
    add_transform(&mut scene, GeometryRef::circle(1.0), GeometryRef::circle(3.0));
    add_transform(
        &mut scene,
        GeometryRef::rectangle(2.0, 4.0),
        GeometryRef::rectangle(6.0, 8.0),
    );
    add_transform(
        &mut scene,
        GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)),
    );

    let compiled = CompiledScene::compile(&scene).unwrap();
    assert!(matches!(
        compiled.tracks()[0].transform_geometry_plan,
        Some(TransformGeometryPlan::Circle {
            from_radius: 1.0,
            to_radius: 3.0
        })
    ));
    assert!(matches!(
        compiled.tracks()[1].transform_geometry_plan,
        Some(TransformGeometryPlan::Rectangle {
            from_size: Vec2 { x: 2.0, y: 4.0 },
            to_size: Vec2 { x: 6.0, y: 8.0 }
        })
    ));
    assert!(matches!(
        compiled.tracks()[2].transform_geometry_plan,
        Some(TransformGeometryPlan::Line {
            from_start: Vec2 { x: -1.0, y: 0.0 },
            from_end: Vec2 { x: 1.0, y: 0.0 },
            to_start: Vec2 { x: 0.0, y: -2.0 },
            to_end: Vec2 { x: 0.0, y: 2.0 }
        })
    ));
}
''')

Path("crates/noon-runtime/tests/analytic_transform.rs").write_text(r'''use noon_compile::CompiledScene;
use noon_core::{
    Color, Easing, GeometryRef, ObjectSnapshot, SceneDefinition, Style, TrackTiming, Transform2D,
    Vec2,
};
use noon_runtime::SceneInstance;

fn snapshot(geometry: GeometryRef, transform: Transform2D, style: Style) -> ObjectSnapshot {
    ObjectSnapshot {
        geometry,
        transform,
        style,
    }
}

fn build_scene() -> SceneDefinition {
    let mut scene = SceneDefinition::new();
    let style_a = Style {
        fill: Some(Color::rgb(0.2, 0.3, 0.4)),
        stroke: None,
        stroke_width: 1.0,
        opacity: 1.0,
    };
    let style_b = Style {
        fill: Some(Color::rgb(0.8, 0.6, 0.4)),
        opacity: 0.5,
        ..style_a
    };
    let transform_a = Transform2D::IDENTITY;
    let transform_b = Transform2D {
        translation: Vec2::new(4.0, -2.0),
        rotation: 0.8,
        scale: Vec2::new(2.0, 0.5),
    };

    let circle = scene.add(GeometryRef::circle(1.0));
    scene.object_mut(circle).unwrap().style = style_a;
    scene
        .animate_transform(
            circle,
            snapshot(GeometryRef::circle(1.0), transform_a, style_a),
            snapshot(GeometryRef::circle(3.0), transform_b, style_b),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let rectangle = scene.add(GeometryRef::rectangle(2.0, 4.0));
    scene
        .animate_transform(
            rectangle,
            snapshot(
                GeometryRef::rectangle(2.0, 4.0),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            snapshot(
                GeometryRef::rectangle(6.0, 8.0),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let line = scene.add(GeometryRef::line(
        Vec2::new(-1.0, 0.0),
        Vec2::new(1.0, 0.0),
    ));
    scene
        .animate_transform(
            line,
            snapshot(
                GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            snapshot(
                GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();
    scene
}

#[test]
fn analytic_transform_has_exact_endpoints_and_midpoints() {
    let mut instance = SceneInstance::new(CompiledScene::compile(&build_scene()).unwrap());
    let frame = instance.seek(1.0).unwrap();

    assert_eq!(frame.objects[0].geometry, GeometryRef::circle(2.0));
    assert_eq!(frame.objects[0].transform.translation, Vec2::new(2.0, -1.0));
    assert_eq!(frame.objects[0].transform.rotation, 0.4);
    assert_eq!(frame.objects[0].transform.scale, Vec2::new(1.5, 0.75));
    assert!((frame.objects[0].style.opacity - 0.75).abs() < 1.0e-6);

    assert_eq!(
        frame.objects[1].geometry,
        GeometryRef::rectangle(4.0, 6.0)
    );
    assert_eq!(
        frame.objects[2].geometry,
        GeometryRef::line(Vec2::new(-0.5, -1.0), Vec2::new(0.5, 1.0))
    );
    assert!(frame.render_geometries.iter().all(Option::is_none));
    assert!(frame.morphs.iter().all(|value| *value == 0.0));

    let end = instance.seek(2.0).unwrap();
    assert_eq!(end.objects[0].geometry, GeometryRef::circle(3.0));
    assert_eq!(end.objects[1].geometry, GeometryRef::rectangle(6.0, 8.0));
    assert_eq!(
        end.objects[2].geometry,
        GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0))
    );
}

#[test]
fn direct_seek_and_forward_playback_match_for_analytic_transform() {
    let compiled = CompiledScene::compile(&build_scene()).unwrap();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);
    for step in 1..=13 {
        sequential.advance_to(step as f64 * 0.1).unwrap();
    }
    direct.seek(1.3).unwrap();
    assert_eq!(sequential.frame(), direct.frame());
}

#[test]
fn sequential_circle_transforms_are_continuous_at_boundary() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let style = Style::default();
    scene
        .animate_transform(
            object,
            snapshot(GeometryRef::circle(1.0), Transform2D::IDENTITY, style),
            snapshot(GeometryRef::circle(3.0), Transform2D::IDENTITY, style),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .unwrap();
    scene
        .animate_transform(
            object,
            snapshot(GeometryRef::circle(3.0), Transform2D::IDENTITY, style),
            snapshot(GeometryRef::circle(5.0), Transform2D::IDENTITY, style),
            TrackTiming::new(1.0, 1.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    assert_eq!(instance.seek(1.0).unwrap().objects[0].geometry, GeometryRef::circle(3.0));
    assert_eq!(instance.seek(1.5).unwrap().objects[0].geometry, GeometryRef::circle(4.0));
}
''')

Path("crates/noon-render-wgpu/tests/analytic_transform.rs").write_text(r'''use noon_compile::CompiledScene;
use noon_core::{Easing, GeometryRef, ObjectSnapshot, SceneDefinition, Style, TrackTiming, Transform2D};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

#[test]
fn analytic_geometry_transform_dirties_only_one_instance_without_path_work() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let style = Style::default();
    scene
        .animate_transform(
            object,
            ObjectSnapshot {
                geometry: GeometryRef::circle(1.0),
                transform: Transform2D::IDENTITY,
                style,
            },
            ObjectSnapshot {
                geometry: GeometryRef::circle(3.0),
                transform: Transform2D::IDENTITY,
                style,
            },
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();

    let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut preparer = FramePreparer::new();
    let initial_changes = instance.take_frame_changes();
    let initial = preparer.prepare_incremental(instance.frame(), &initial_changes);
    assert_eq!(initial.stats.geometry_cache_misses, 0);
    assert_eq!(preparer.cached_path_mesh_count(), 0);

    instance.advance_to(0.5).unwrap();
    let changes = instance.take_frame_changes();
    let steady = preparer.prepare_incremental(instance.frame(), &changes);
    assert_eq!(steady.stats.instances_repacked, 1);
    assert_eq!(steady.stats.dirty_instance_count, 1);
    assert_eq!(steady.circle_dirty_ranges.len(), 1);
    assert_eq!(steady.circle_dirty_ranges[0], 0..1);
    assert_eq!(steady.stats.geometry_cache_misses, 0);
    assert!(!steady.path_geometry_dirty);
    assert_eq!(preparer.cached_path_mesh_count(), 0);
}
''')

Path("web/python/test_analytic_transform.py").write_text(r'''import unittest

from noon import Circle, Color, Line, Rectangle, Scene, Transform


class AnalyticTransformTests(unittest.TestCase):
    def test_detached_analytic_targets_serialize_as_atomic_transforms(self) -> None:
        scene = Scene()
        circle = scene.circle(1.0, key="circle")
        rectangle = scene.rectangle(2.0, 3.0, key="rectangle")
        line = scene.line((-1.0, 0.0), (1.0, 0.0), key="line")

        scene.play(
            Transform(circle, Circle(3.0, position=(2.0, -1.0), opacity=0.5)),
            Transform(rectangle, Rectangle(6.0, 8.0, rotation=0.4)),
            Transform(
                line,
                Line(
                    (0.0, -2.0),
                    (0.0, 2.0),
                    stroke=Color(0.2, 0.7, 0.9),
                ),
            ),
            duration=2.0,
        )

        document = scene.to_document()
        self.assertEqual(len(document["objects"]), 3)
        self.assertEqual([track["property"] for track in document["tracks"]], ["transform"] * 3)
        self.assertEqual(
            document["tracks"][0]["values"]["object"]["to"]["geometry"],
            {"circle": {"radius": 3.0}},
        )
        self.assertEqual(
            document["tracks"][1]["values"]["object"]["to"]["geometry"]["rectangle"]["size"],
            {"x": 6.0, "y": 8.0},
        )
        self.assertEqual(
            document["tracks"][2]["values"]["object"]["to"]["geometry"]["line"],
            {
                "start": {"x": 0.0, "y": -2.0},
                "end": {"x": 0.0, "y": 2.0},
            },
        )


if __name__ == "__main__":
    unittest.main()
''')
