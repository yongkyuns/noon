use noon_core::{GeometryRef, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnalyticRevealKey {
    Rectangle(u32, u32),
}

pub(crate) fn analytic_reveal_key(geometry: &GeometryRef) -> Option<AnalyticRevealKey> {
    match geometry {
        GeometryRef::Circle { .. } => None,
        GeometryRef::Rectangle { size } => Some(AnalyticRevealKey::Rectangle(
            size.x.to_bits(),
            size.y.to_bits(),
        )),
        GeometryRef::Line { .. } | GeometryRef::VectorPath(_) | GeometryRef::External(_) => None,
    }
}

pub(crate) fn temporary_reveal_path(
    geometry: &GeometryRef,
    reveal: f32,
) -> Option<(AnalyticRevealKey, VectorPath)> {
    if reveal >= 1.0 {
        return None;
    }
    let key = analytic_reveal_key(geometry)?;
    let path = noon_geometry::canonical_outline_path(geometry)?;
    let partial = noon_geometry::pointwise_partial_path(&path, 0.0, reveal.clamp(0.0, 1.0));
    Some((key, partial))
}

// Regression for renderer history-dependence discovered by #2094's
// fresh-renderer comparison. These do not require a GPU.
#[cfg(test)]
mod replacement_tests {
    use crate::{FramePreparer, LineInstance, PreparedFrame};
    use noon_core::{
        Color, GeometryRef, ObjectContentRef, ObjectId, StrokeCap, Style, Transform2D, Vec2,
        VectorPath,
    };
    use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

    fn path(offset: f32) -> GeometryRef {
        GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-1.3, 0.55 + offset))
                .line_to(Vec2::new(1.1, -0.6)),
        )
    }

    fn frame(reveal: f32) -> FrameState {
        let objects = [path(0.0), GeometryRef::rectangle(0.5, 0.5)]
            .into_iter()
            .enumerate()
            .map(|(index, geometry)| FrameObjectState {
                glow: None,
                spatial: None,
                id: ObjectId::new(index as u64 + 1),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(geometry),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style: Style {
                    fill: None,
                    stroke: Some(Color::WHITE),
                    stroke_width: 0.019,
                    stroke_cap: StrokeCap::Round,
                    opacity: 0.65,
                    ..Style::default()
                },
                appearance: 1.0,
            })
            .collect();
        FrameState {
            time: 0.0,
            objects,
            presences: vec![true; 2],
            reveals: vec![reveal, 1.0],
            morphs: vec![0.0; 2],
            render_geometries: vec![None; 2],
            render_transforms: vec![None; 2],
            family_animations: vec![],
            family_animation_plan_indices: vec![],
        }
    }

    fn preparer(individual: bool) -> FramePreparer {
        if individual {
            FramePreparer::for_individual_path_draws()
        } else {
            FramePreparer::new()
        }
    }

    fn active_heads(prepared: &PreparedFrame<'_>) -> Vec<LineInstance> {
        prepared
            .line_ids
            .iter()
            .zip(prepared.lines)
            .filter_map(|(id, line)| {
                (*id == ObjectId::new(1) && line.style.opacity > 0.0).then_some(*line)
            })
            .collect()
    }

    #[test]
    fn full_to_partial_reveal_has_the_same_head_as_fresh_preparation() {
        for individual in [false, true] {
            let mut state = frame(1.0);
            let mut retained = preparer(individual);
            assert!(retained.prepare(&state).lines.is_empty());
            for reveal in [0.4, 0.65, 1.0, 0.0, 0.25, 0.4] {
                state.reveals[0] = reveal;
                let changed = FrameChanges::objects(vec![0]);
                let prepared = retained.prepare_incremental(&state, &changed);
                let mut fresh = preparer(individual);
                let reference = fresh.prepare(&state);
                assert_eq!(active_heads(&prepared), active_heads(&reference));
                assert_eq!(prepared.lines.len(), 1, "reuse one head record");
                assert_eq!(prepared.stats.full_rebuilds, 0);
                assert_eq!(prepared.stats.geometry_cache_misses, 0);
                assert!(prepared.rectangle_dirty_ranges.is_empty());
            }
            let prepared = retained.prepare_incremental(&state, &FrameChanges::default());
            assert!(!prepared.path_geometry_dirty);
            assert!(prepared.line_dirty_ranges.is_empty());
            assert!(prepared.path_dirty_ranges.is_empty());
            assert_eq!(prepared.stats.instances_repacked, 0);
        }
    }

    #[test]
    fn mesh_and_cap_replacements_reuse_one_head_record() {
        for individual in [false, true] {
            let mut state = frame(0.4);
            let mut retained = preparer(individual);
            assert_eq!(retained.prepare(&state).lines.len(), 1);
            for index in 0..32 {
                state.objects[0].content = ObjectContentRef::Geometry(path(index as f32 * 0.01));
                state.objects[0].style.stroke_width = 0.02 + (index % 3) as f32 * 0.01;
                state.objects[0].style.stroke_cap = match index % 3 {
                    0 => StrokeCap::Round,
                    1 => StrokeCap::Butt,
                    _ => StrokeCap::Square,
                };
                let changed = FrameChanges::objects(vec![0]);
                let prepared = retained.prepare_incremental(&state, &changed);
                let mut fresh = preparer(individual);
                let reference = fresh.prepare(&state);
                assert_eq!(active_heads(&prepared), active_heads(&reference));
                assert_eq!(
                    prepared.lines.len(),
                    1,
                    "replacement must not leak head slots"
                );
                assert_eq!(prepared.stats.full_rebuilds, 0);
                assert!(prepared.rectangle_dirty_ranges.is_empty());
            }
        }
    }

    #[test]
    fn temporary_analytic_reveal_keeps_an_existing_path_head_inactive() {
        for individual in [false, true] {
            let mut state = frame(0.4);
            let mut retained = preparer(individual);
            assert_eq!(retained.prepare(&state).lines.len(), 1);
            for index in 0..8 {
                let geometry = if index % 2 == 0 {
                    GeometryRef::rectangle(1.4, 1.0)
                } else {
                    path(0.0)
                };
                state.objects[0].content = ObjectContentRef::Geometry(geometry);
                let changed = FrameChanges::objects(vec![0]);
                let prepared = retained.prepare_incremental(&state, &changed);
                let mut fresh = preparer(individual);
                let reference = fresh.prepare(&state);
                assert_eq!(active_heads(&prepared), active_heads(&reference));
                assert_eq!(prepared.lines.len(), 1);
                assert_eq!(prepared.stats.full_rebuilds, 0);
            }
        }
    }
}
