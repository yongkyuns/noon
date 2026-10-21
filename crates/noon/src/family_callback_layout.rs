//! Shared effective-layout replacement for callback phase overlays.

use crate::{
    AuthoringError, ExecutionSessionCallbackError, LayoutAnchor, SemanticNodeId, Transform2D,
};
use noon_core::{Rect, SceneRevision, SemanticLoweringError, SemanticVec3};
use std::collections::BTreeMap;

/// One fully staged callback replacement row. The caller publishes all rows only
/// after the complete operation has succeeded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CallbackLayoutChange {
    pub node: SemanticNodeId,
    pub transform: Transform2D,
    pub bounds: Option<Rect>,
}

#[derive(Debug)]
pub enum CallbackLayoutError {
    Authoring(AuthoringError),
    Callback(ExecutionSessionCallbackError),
    Family(crate::FamilyCallbackPaintError),
    Empty,
    MissingBounds(SemanticNodeId),
}

impl std::fmt::Display for CallbackLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(f),
            Self::Callback(error) => error.fmt(f),
            Self::Family(error) => error.fmt(f),
            Self::Empty => f.write_str("callback layout anchor has no semantic leaves"),
            Self::MissingBounds(node) => write!(f, "callback layout has no bounds for {node:?}"),
        }
    }
}
impl std::error::Error for CallbackLayoutError {}
impl From<AuthoringError> for CallbackLayoutError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}
impl From<ExecutionSessionCallbackError> for CallbackLayoutError {
    fn from(error: ExecutionSessionCallbackError) -> Self {
        Self::Callback(error)
    }
}
impl From<crate::FamilyCallbackPaintError> for CallbackLayoutError {
    fn from(error: crate::FamilyCallbackPaintError) -> Self {
        Self::Family(error)
    }
}
impl From<SemanticLoweringError> for CallbackLayoutError {
    fn from(error: SemanticLoweringError) -> Self {
        Self::Authoring(error.into())
    }
}

fn addressed_nodes(
    anchor: &LayoutAnchor,
    revision: SceneRevision,
) -> Result<Vec<SemanticNodeId>, CallbackLayoutError> {
    let node = anchor.resolve()?;
    let store = anchor.integration_store().borrow();
    if matches!(
        store.node(node).map(|item| item.kind()),
        Some(noon_core::SemanticNodeKind::Family(_))
    ) {
        Ok(
            crate::MobjectFamily::from_node(std::rc::Rc::clone(anchor.integration_store()), node)?
                .callback_leaf_nodes(revision)?,
        )
    } else {
        Ok(vec![node])
    }
}

fn union_bounds(
    nodes: &[SemanticNodeId],
    rows: &BTreeMap<SemanticNodeId, (Transform2D, Option<Rect>)>,
) -> Result<Rect, CallbackLayoutError> {
    let mut bounds: Option<Rect> = None;
    for node in nodes {
        let (_, row_bounds) = rows
            .get(node)
            .copied()
            .ok_or(ExecutionSessionCallbackError::UnknownObject(*node))?;
        let next = row_bounds.ok_or(CallbackLayoutError::MissingBounds(*node))?;
        bounds = Some(match bounds {
            None => next,
            Some(mut value) => {
                value.min.x = value.min.x.min(next.min.x);
                value.min.y = value.min.y.min(next.min.y);
                value.max.x = value.max.x.max(next.max.x);
                value.max.y = value.max.y.max(next.max.y);
                value
            }
        });
    }
    bounds.ok_or(CallbackLayoutError::Empty)
}

/// Prepare a replace/stretch from phase-effective rows. Both anchors may address
/// ordinary objects or families. No overlay write is produced until every row,
/// factor, and transformed bound has passed validation.
pub fn prepare_callback_layout_replace(
    source: &LayoutAnchor,
    target: &LayoutAnchor,
    revision: SceneRevision,
    dimension: crate::LayoutDimension,
    stretch: bool,
    mut read: impl FnMut(
        SemanticNodeId,
    ) -> Result<(Transform2D, Option<Rect>), ExecutionSessionCallbackError>,
) -> Result<Vec<CallbackLayoutChange>, CallbackLayoutError> {
    if !std::rc::Rc::ptr_eq(source.integration_store(), target.integration_store()) {
        return Err(AuthoringError::ForeignStore.into());
    }
    let source_nodes = addressed_nodes(source, revision)?;
    let target_nodes = addressed_nodes(target, revision)?;
    let mut rows = BTreeMap::new();
    for node in source_nodes.iter().chain(&target_nodes).copied() {
        if let std::collections::btree_map::Entry::Vacant(entry) = rows.entry(node) {
            entry.insert(read(node)?);
        }
    }
    prepare_layout_replace_rows(&source_nodes, &target_nodes, &rows, dimension, stretch)
}

/// The same pure row operation is used by callback evaluation and native
/// expression specialization. The latter never invokes a host callable.
pub(crate) fn prepare_layout_replace_rows(
    source_nodes: &[SemanticNodeId],
    target_nodes: &[SemanticNodeId],
    rows: &BTreeMap<SemanticNodeId, (Transform2D, Option<Rect>)>,
    dimension: crate::LayoutDimension,
    stretch: bool,
) -> Result<Vec<CallbackLayoutChange>, CallbackLayoutError> {
    let source_bounds = union_bounds(source_nodes, rows)?;
    let target_bounds = union_bounds(target_nodes, rows)?;
    let (sx, sy) = if stretch {
        let sx = if source_bounds.width() == 0.0 {
            1.0
        } else {
            f64::from(target_bounds.width() / source_bounds.width())
        };
        let sy = if source_bounds.height() == 0.0 {
            1.0
        } else {
            f64::from(target_bounds.height() / source_bounds.height())
        };
        (sx, sy)
    } else {
        let (from, to) = match dimension {
            crate::LayoutDimension::Width => (source_bounds.width(), target_bounds.width()),
            crate::LayoutDimension::Height => (source_bounds.height(), target_bounds.height()),
            crate::LayoutDimension::Depth => (0.0, 0.0),
        };
        let factor = if from == 0.0 {
            1.0
        } else {
            f64::from(to / from)
        };
        (factor, factor)
    };
    crate::semantic_mobject::authoring_render_f64("callback replace scale.x", sx)?;
    crate::semantic_mobject::authoring_render_f64("callback replace scale.y", sy)?;
    let source_center = (
        (f64::from(source_bounds.min.x) + f64::from(source_bounds.max.x)) * 0.5,
        (f64::from(source_bounds.min.y) + f64::from(source_bounds.max.y)) * 0.5,
    );
    let target_center = (
        (f64::from(target_bounds.min.x) + f64::from(target_bounds.max.x)) * 0.5,
        (f64::from(target_bounds.min.y) + f64::from(target_bounds.max.y)) * 0.5,
    );
    let mut changes = Vec::with_capacity(source_nodes.len());
    for &node in source_nodes {
        let (previous, bounds) = rows[&node];
        let (local_x, local_y) =
            crate::dimension_fit::world_scale_factors(f64::from(previous.rotation), sx, sy)?;
        let scale = SemanticVec3::new(
            f64::from(previous.scale.x) * local_x,
            f64::from(previous.scale.y) * local_y,
            0.0,
        )
        .lower_xy_f32()?;
        let translation = SemanticVec3::new(
            target_center.0 + (f64::from(previous.translation.x) - source_center.0) * sx,
            target_center.1 + (f64::from(previous.translation.y) - source_center.1) * sy,
            0.0,
        )
        .lower_xy_f32()?;
        let transform = Transform2D {
            translation,
            scale,
            ..previous
        };
        let bounds = bounds
            .map(|rect| scale_bounds(rect, source_center, target_center, sx, sy))
            .transpose()?;
        changes.push(CallbackLayoutChange {
            node,
            transform,
            bounds,
        });
    }
    Ok(changes)
}

fn scale_bounds(
    bounds: Rect,
    source: (f64, f64),
    target: (f64, f64),
    sx: f64,
    sy: f64,
) -> Result<Rect, SemanticLoweringError> {
    let point = |x: f32, y: f32| -> Result<noon_core::Vec2, SemanticLoweringError> {
        SemanticVec3::new(
            target.0 + (f64::from(x) - source.0) * sx,
            target.1 + (f64::from(y) - source.1) * sy,
            0.0,
        )
        .lower_xy_f32()
    };
    let points = [
        point(bounds.min.x, bounds.min.y)?,
        point(bounds.min.x, bounds.max.y)?,
        point(bounds.max.x, bounds.min.y)?,
        point(bounds.max.x, bounds.max.y)?,
    ];
    let mut min = points[0];
    let mut max = points[0];
    for point in points.into_iter().skip(1) {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    Ok(Rect::new(min, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnimationCompositionRequest as Request, AnimationOptions, ExecutionSegment,
        ExecutionSession, RateFunction, RustHostCallbackTable, Scene, TransformToRequest,
    };
    use std::{cell::Cell, rc::Rc};

    struct Fixture {
        scene: Scene,
        session: ExecutionSession,
        source: crate::Mobject,
        target: crate::Mobject,
        segment: ExecutionSegment,
        callbacks: RustHostCallbackTable,
        calls: Rc<Cell<usize>>,
    }

    impl Fixture {
        fn new(
            native: bool,
            reverse: bool,
            end_scale: (f64, f64),
            stroke: f64,
            rotation: f64,
        ) -> Self {
            Self::with_setup(native, reverse, end_scale, stroke, rotation, |_, _, _| {})
        }

        fn with_setup(
            native: bool,
            reverse: bool,
            end_scale: (f64, f64),
            stroke: f64,
            rotation: f64,
            setup: impl FnOnce(&mut Scene, &crate::Mobject, &crate::Mobject),
        ) -> Self {
            let mut scene = Scene::new();
            let mut source = scene.rectangle(6.5, 1.5).unwrap();
            source.set_stroke_width(stroke).unwrap();
            source.set_translation(1.0, -1.0).unwrap();
            let mut target = scene.rectangle(6.0, 1.0).unwrap();
            target.set_stroke_width(0.2).unwrap();
            target.set_scale(0.3, 0.9).unwrap();
            target.set_translation(-2.0, 2.0).unwrap();
            scene.add(&source).unwrap();
            scene.add(&target).unwrap();
            setup(&mut scene, &source, &target);
            let mut end = target.target_editor().unwrap();
            end.set_scale(end_scale.0, end_scale.1).unwrap();
            end.set_translation(3.0, -2.0).unwrap();
            end.set_rotation(rotation).unwrap();
            let source_authored = source.state().unwrap().transform;
            let mut session = scene.execution_session().unwrap();
            session.begin_replay_retention(Default::default()).unwrap();
            let options = AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Smooth)
                .reverse_rate_function(reverse);
            let callback = noon_core::HostCallbackId::new(19);
            let expression = if native {
                Request::LayoutReplaceInterval {
                    target: &source,
                    layout_target: &target,
                    fallback: callback,
                    options: AnimationOptions::new().run_time(1.0),
                }
            } else {
                Request::CallbackInterval {
                    target: &source,
                    callback,
                    options: AnimationOptions::new().run_time(1.0),
                }
            };
            let requests = [
                Request::TransformTo(TransformToRequest::new(&target, &end, options)),
                expression,
            ];
            let segment = scene
                .live(&mut session)
                .declare_and_activate_animation_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &requests,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
                .unwrap();
            assert_eq!(
                source.state().unwrap().transform,
                source_authored,
                "specialization must not rewrite authored base state at activation"
            );
            let source_anchor = crate::LayoutAnchor::from(&source);
            let target_anchor = crate::LayoutAnchor::from(&target);
            let calls = Rc::new(Cell::new(0));
            let observed_calls = Rc::clone(&calls);
            let mut callbacks = RustHostCallbackTable::new();
            callbacks
                .insert(callback, move |context| {
                    observed_calls.set(observed_calls.get() + 1);
                    context
                        .replace_layout(
                            &source_anchor,
                            &target_anchor,
                            crate::LayoutDimension::Width,
                            false,
                        )
                        .map_err(std::io::Error::other)?;
                    Ok::<(), std::io::Error>(())
                })
                .unwrap();
            Self {
                scene,
                session,
                source,
                target,
                segment,
                callbacks,
                calls,
            }
        }

        fn sample(&mut self, time: f64) -> Transform2D {
            self.callbacks
                .advance_segment_to(&mut self.session, self.segment, time)
                .unwrap();
            self.transform()
        }

        fn transform(&self) -> Transform2D {
            let id = self
                .session
                .execution_object_id(self.source.node_id())
                .unwrap();
            self.session
                .frame()
                .objects
                .iter()
                .find(|row| row.id == id)
                .unwrap()
                .transform
        }

        fn finish(&mut self) {
            self.sample(1.0);
            self.scene
                .live(&mut self.session)
                .complete_segment(self.segment)
                .unwrap();
        }
    }

    #[test]
    fn native_layout_tracks_match_host_replacement_and_replay_at_unsampled_times() {
        for (reverse, end_scale, stroke) in [
            (false, (1.0, 3.0), 0.0),
            (true, (1.0, 3.0), 0.0),
            (false, (0.3, 0.9), 0.0),
            (true, (0.3, 0.9), 0.0),
            (false, (1.0, 0.5), 0.1),
        ] {
            let mut native = Fixture::new(true, reverse, end_scale, stroke, 0.0);
            let mut host = Fixture::new(false, reverse, end_scale, stroke, 0.0);
            let times = [0.0, 0.113, 0.471, 0.937, 1.0];
            let mut expected = Vec::new();
            for time in times {
                let actual = native.sample(time);
                let reference = host.sample(time);
                let target_anchor = crate::LayoutAnchor::from(&native.target);
                let expected_width = native
                    .scene
                    .live(&mut native.session)
                    .dimension_length(&target_anchor, crate::LayoutDimension::Width)
                    .unwrap();
                assert!(
                    (f64::from(actual.scale.x) * 6.5 - expected_width).abs() <= 1.0e-6,
                    "callback replacement must match stroke-free authored layout at t={time}"
                );
                for (a, b) in [
                    (actual.translation.x, reference.translation.x),
                    (actual.translation.y, reference.translation.y),
                    (actual.scale.x, reference.scale.x),
                    (actual.scale.y, reference.scale.y),
                ] {
                    assert!(
                        (a - b).abs() <= 1.0e-6,
                        "t={time} reverse={reverse}: {actual:?} != {reference:?}"
                    );
                }
                expected.push((time, actual));
            }
            assert_eq!(
                native.calls.get(),
                0,
                "the qualified expression must not invoke host code"
            );
            assert!(host.calls.get() > 0);
            native.finish();
            host.finish();
            assert_eq!(
                host.session.seal_replay(),
                Err(noon_runtime::ReplayError::UnsupportedDomain)
            );
            native.session.seal_replay().unwrap();
            let authored = native.source.state().unwrap();
            for &(time, expected) in expected.iter().rev().chain(expected.iter()) {
                native.session.seek(time).unwrap();
                assert_eq!(
                    native.transform(),
                    expected,
                    "direct seek must use the ordinary evaluator at t={time}, reverse={reverse}"
                );
                assert_eq!(native.source.state().unwrap(), authored);
            }
            assert_eq!(native.calls.get(), 0);
        }
    }

    #[test]
    fn childless_geometry_owning_family_does_not_widen_the_leaf_layout_profile() {
        use crate::execution_session::SemanticCompositionRequest as SemanticRequest;
        let mut scene = Scene::new();
        let source = scene.rectangle(6.0, 1.0).unwrap();
        let target = scene.rectangle(6.0, 1.0).unwrap();
        let end = scene.rectangle(6.0, 1.0).unwrap();
        let family = scene.family(&[]).unwrap();
        let mut content = noon_core::SemanticMutationTransaction::new();
        content.replace_content(
            family.node_id(),
            noon_core::StoredGeometry::Rectangle {
                size: noon_core::Vec2::new(6.0, 1.0),
            },
        );
        scene.apply_semantic_transaction(content).unwrap();
        scene
            .add_many(&[(&source).into(), (&target).into(), (&family).into()])
            .unwrap();
        let session = scene.execution_session().unwrap();
        assert!(session.execution_object_id(family.node_id()).is_some());
        let store = scene.integration_store().borrow();
        let request = |source, target, end| SemanticRequest::Composition {
            kind: noon_core::SemanticAnimationCompositionKind::Parallel,
            children: vec![
                SemanticRequest::TransformTo {
                    source: target,
                    target_state: end,
                    interpolation: noon_core::SemanticTransformInterpolation::Affine,
                    complete_priority: false,
                    options: AnimationOptions::new().run_time(1.0),
                },
                SemanticRequest::LayoutReplaceInterval {
                    target: source,
                    layout_target: target,
                    fallback: noon_core::HostCallbackId::new(19),
                    options: AnimationOptions::new().run_time(1.0),
                },
            ],
            options: AnimationOptions::new(),
        };
        let ordinary = [source.node_id(), target.node_id(), end.node_id()];
        assert!(session
            .specialize_layout_replace(
                &store,
                &request(ordinary[0], ordinary[1], ordinary[2]),
                AnimationOptions::new()
            )
            .is_some());
        for position in 0..3 {
            let mut ids = ordinary;
            ids[position] = family.node_id();
            assert!(
                session
                    .specialize_layout_replace(
                        &store,
                        &request(ids[0], ids[1], ids[2]),
                        AnimationOptions::new()
                    )
                    .is_none(),
                "family at position {position} must keep the host callback"
            );
        }
    }

    #[test]
    fn native_click_bindings_keep_the_callback_interval_instead_of_frozen_layout() {
        for bind_source in [false, true] {
            let mut fixture = Fixture::with_setup(
                true,
                false,
                (1.0, 3.0),
                0.0,
                0.0,
                |scene, source, target| {
                    let mut bound = if bind_source {
                        source.clone()
                    } else {
                        target.clone()
                    };
                    bound.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
                    scene
                        .on_click_indicate(
                            &bound,
                            crate::IndicateOptions::default(),
                            AnimationOptions::new(),
                        )
                        .unwrap();
                },
            );
            fixture.sample(0.471);
            assert!(fixture.calls.get() > 0);
            fixture.finish();
            assert_eq!(
                fixture.session.seal_replay(),
                Err(noon_runtime::ReplayError::UnsupportedDomain)
            );
        }
    }

    #[test]
    fn active_segment_publication_barrier_preserves_the_folded_content_assumption() {
        let mut fixture = Fixture::new(true, false, (1.0, 3.0), 0.0, 0.0);
        fixture.sample(0.471);
        let frame = fixture.session.frame().clone();
        let authored = fixture.source.state().unwrap();
        let error = fixture
            .scene
            .live(&mut fixture.session)
            .become_mobject(
                &fixture.source,
                &fixture.target,
                crate::ManimBecomeOptions::default(),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("active animation segment must be completed"),
            "{error}"
        );
        assert_eq!(fixture.session.frame(), &frame);
        assert_eq!(fixture.source.state().unwrap(), authored);
        fixture.finish();
    }

    #[test]
    fn unsupported_layout_profiles_keep_the_host_guard_and_atomic_seek_denial() {
        for (scale, stroke, rotation) in [
            ((-1.0, 3.0), 0.0, 0.0),
            ((0.0, 3.0), 0.0, 0.0),
            ((1.0, 3.0), 0.0, 0.4),
        ] {
            let mut fixture = Fixture::new(true, false, scale, stroke, rotation);
            fixture.sample(0.47);
            assert!(fixture.calls.get() > 0);
            fixture.finish();
            assert_eq!(
                fixture.session.seal_replay(),
                Err(noon_runtime::ReplayError::UnsupportedDomain)
            );
            let frame = fixture.session.frame().clone();
            let publication = fixture.session.publication_context();
            assert!(fixture.session.seek(0.3).is_err());
            assert_eq!(fixture.session.frame(), &frame);
            assert_eq!(fixture.session.publication_context(), publication);
        }
    }
}
