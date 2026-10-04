//! Explicit diagnostic codec over the current shared execution frame.
use crate::ExecutionSession;
use noon_core::{Color, GeometryRef, GeometryResource, GeometryResourceLookup, Rect, Vec2};
use serde_json::{json, Value};

fn paint_json(color: Option<Color>, opacity: f32) -> Value {
    match color {
        Some(color) => json!({
            "red": color.red,
            "green": color.green,
            "blue": color.blue,
            "alpha": color.alpha * opacity,
        }),
        None => Value::Null,
    }
}

fn bounds_json(bounds: Option<Rect>) -> Value {
    match bounds {
        Some(bounds) => json!({
            "min": [bounds.min.x, bounds.min.y],
            "max": [bounds.max.x, bounds.max.y],
            "width": bounds.width(),
            "height": bounds.height(),
        }),
        None => Value::Null,
    }
}

fn diagnostic_geometry_at_progress(
    geometry: &GeometryRef,
    morph: f32,
    reveal: f32,
) -> Option<GeometryRef> {
    if let GeometryRef::VectorPath(path) = geometry {
        if let Some(target) = path.morph_target() {
            return noon_geometry::interpolate_revealed_path_preserving_order(
                path, target, morph, reveal,
            )
            .ok()
            .map(GeometryRef::VectorPath);
        }
        if morph != 0.0 {
            return None;
        }
    } else if morph != 0.0 {
        return None;
    }

    if reveal >= 1.0 {
        return Some(geometry.clone());
    }
    let path = noon_geometry::canonical_outline_path(geometry)?;
    Some(GeometryRef::VectorPath(noon_geometry::authored_partial_path(
        &path,
        0.0,
        reveal.clamp(0.0, 1.0),
    )))
}

/// Capture derived current-frame observations for debugging and test artifacts.
///
/// This opt-in codec performs O(frame size) work and never advances or mutates execution.
pub fn execution_frame_value(session: &ExecutionSession) -> Value {
    let frame = session.frame();
    let objects = session
        .painter_order()
        .iter()
        .map(|&index| {
            let index = index as usize;
            let object = &frame.objects[index];
            let transform = frame.render_transform(index);
            // Resource resolution is diagnostic work only. Engine layers continue
            // sharing immutable typed resources; this snapshot cannot update them.
            let geometry = match frame.render_geometry(index) {
                Some(GeometryRef::External(id)) => session
                    .geometry_resources()
                    .current_handle(*id)
                    .and_then(|handle| session.geometry_resources().get(handle))
                    .map(|resource| match resource {
                        GeometryResource::VectorPath(path) => {
                            GeometryRef::VectorPath((**path).clone())
                        }
                        GeometryResource::Mesh(_) => GeometryRef::External(*id),
                    }),
                geometry => geometry.cloned(),
            };
            let geometry = geometry.and_then(|geometry| {
                diagnostic_geometry_at_progress(
                    &geometry,
                    frame.morph(index),
                    frame.reveal(index),
                )
            });
            let bounds = geometry
                .as_ref()
                .and_then(|geometry| geometry.world_bounds(transform))
                .or_else(|| {
                    object.text_bounds.and_then(|bounds| {
                        Rect::from_points(
                            [
                                bounds.min,
                                Vec2::new(bounds.max.x, bounds.min.y),
                                bounds.max,
                                Vec2::new(bounds.min.x, bounds.max.y),
                            ]
                            .map(|point| transform.transform_point(point)),
                        )
                    })
                });
            let center = bounds.map(Rect::center).unwrap_or(transform.translation);
            let opacity = object.style.opacity * object.appearance;
            let spatial = object.spatial.as_deref().map(|state| {
                let world = state.world;
                let projection = state.camera_projection.map(|projection| match projection {
                    noon_core::SemanticProjection3D::Perspective {
                        vertical_fov_radians,
                        near,
                        far,
                    } => {
                        json!({ "kind": "perspective", "vertical_fov_radians": vertical_fov_radians,
                            "near": near, "far": far })
                    }
                    noon_core::SemanticProjection3D::Orthographic { height, near, far } => {
                        json!({ "kind": "orthographic", "height": height,
                            "near": near, "far": far })
                    }
                });
                json!({
                    "draw_kind": state.draw_kind,
                    "composition_domain": state.composition_domain,
                    "fixed_orientation_center": state.fixed_orientation_center.map(|v| [v.x, v.y, v.z]),
                    "translation": [world.translation.x, world.translation.y, world.translation.z],
                    "rotation_wxyz": world.rotation.components(),
                    "scale": [world.scale.x, world.scale.y, world.scale.z],
                    "camera_projection": projection,
                    "material": if state.material == noon_core::SemanticSpatialMaterial::PointLit {
                        "point_lit"
                    } else { "unlit" },
                    "point_light": state.point_light,
                })
            });
            json!({
                "id": object.id.get(), "present": frame.is_present(index),
                "center": [center.x, center.y], "bounds": bounds_json(bounds),
                "transform": transform,
                "spatial": spatial,
                "fill": paint_json(object.style.fill, opacity),
                "stroke": paint_json(object.style.stroke, opacity),
                "stroke_width": object.style.stroke_width,
                "stroke_width_mode": object.style.stroke_width_mode,
                "stroke_join": object.style.stroke_join, "stroke_cap": object.style.stroke_cap,
                "style_opacity": object.style.opacity, "appearance": object.appearance,
                "reveal": frame.reveal(index), "morph": frame.morph(index),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "engine": "noon", "time": frame.time,
        "publication": session.publication_context(),
        "present_object_count": objects.iter().filter(|object| object["present"] == true).count(),
        "objects": objects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnimationOptions, LiveProgramStatus, ManimArrowOptions, MobjectTarget, RateFunction,
        RustHostCallbackTable, Scene,
    };

    #[test]
    fn spatial_capture_reads_exact_effective_pose_and_light_material() {
        let mut session = crate::example_scenes::spatial_surface::lighting_session().unwrap();
        session.advance_to(0.5).unwrap();
        let before = session.publication_context();
        let capture = execution_frame_value(&session);
        let rows = capture["objects"].as_array().unwrap();
        let light = rows
            .iter()
            .find(|row| row["spatial"]["point_light"] == true)
            .unwrap();
        assert_eq!(light["spatial"]["translation"], json!([0.5, 0.0, 5.0]));
        assert!(rows
            .iter()
            .any(|row| row["spatial"]["material"] == "point_lit"));
        assert!(rows
            .iter()
            .any(|row| row["spatial"]["camera_projection"]["far"] == 30.0));
        assert_eq!(session.publication_context(), before);
    }

    #[test]
    fn debug_capture_reads_the_current_shared_frame_without_advancing_it() {
        let mut program = crate::example_scenes::scale_in_place::program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.drive_to(&mut callbacks, 0.125).unwrap();
        let before = program.session().publication_context();
        let value = execution_frame_value(program.session());
        assert_eq!(value["time"], 0.125);
        assert_eq!(value["present_object_count"], 1);
        assert_eq!(value["objects"][0]["center"][0], 0.25);
        assert_eq!(value["objects"][0]["center"][1], 0.125);
        assert_eq!(program.session().publication_context(), before);
    }

    #[test]
    fn debug_capture_resolves_retained_paths_and_effective_transforms() {
        let mut scene = Scene::new();
        let mut shape =
            crate::Mobject::manim_square(std::rc::Rc::clone(scene.integration_store()), 2.0)
                .unwrap();
        shape.set_fill_color(0.25, 0.5, 0.75, 0.4).unwrap();
        shape.set_fill_opacity(0.4).unwrap();
        shape.set_object_opacity(0.5).unwrap();
        scene.add(&shape).unwrap();
        let mut target = shape.target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_transform_to(
                &shape,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        live.advance_segment_to(segment, 0.5).unwrap();
        let value = execution_frame_value(&session);
        assert_eq!(value["objects"][0]["center"], json!([1.0, -0.5]));
        assert_eq!(value["objects"][0]["bounds"]["width"], 2.0);
        assert!((value["objects"][0]["fill"]["alpha"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn debug_capture_bounds_follow_line_arrow_morph_at_begin_midpoint_and_end() {
        let mut scene = Scene::new();
        let mut horizontal = ManimArrowOptions::arrow(0.0, 0.0, 1.0, 0.0).unwrap();
        horizontal.set_buff(0.0).unwrap();
        horizontal.set_tip_length(0.25).unwrap();
        let mut vertical = ManimArrowOptions::arrow(0.0, 0.0, 0.0, 1.0).unwrap();
        vertical.set_buff(0.0).unwrap();
        vertical.set_tip_length(0.25).unwrap();
        let source = scene.manim_arrow(horizontal).unwrap();
        let target = scene.manim_arrow(vertical).unwrap();
        scene
            .add_many(&[MobjectTarget::Family(source.family())])
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let shaft_id = session
            .execution_object_id(source.shaft().node_id())
            .unwrap()
            .get();
        let segment = {
            let mut live = scene.live(&mut session);
            live.declare_and_activate_family_transform_to(
                source.family(),
                target.family(),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap()
        };

        for (time, progress) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)] {
            {
                let mut live = scene.live(&mut session);
                live.advance_segment_to(segment, time).unwrap();
            }
            let capture = execution_frame_value(&session);
            let row = capture["objects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    row["id"] == shaft_id
                        && row["stroke_width_mode"] == "screen_space"
                        && row["stroke"]["alpha"] != Value::Null
                })
                .unwrap();
            let expected_end = [0.75 * (1.0 - progress), 0.75 * progress];
            let bounds = &row["bounds"];
            assert_eq!(bounds["min"], json!([0.0, 0.0]));
            assert!((bounds["max"][0].as_f64().unwrap() - expected_end[0]).abs() < 1e-6);
            assert!((bounds["max"][1].as_f64().unwrap() - expected_end[1]).abs() < 1e-6);
        }
    }
}
