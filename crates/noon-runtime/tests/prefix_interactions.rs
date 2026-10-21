#![cfg(feature = "prepared-prefix")]

use std::sync::Arc;

use noon_compile::{CompiledObject, CompiledResources, CompiledScene, ExecutionPatch};
use noon_core::{
    Color, CompositionTimeMap, FontResourceArena, GeometryRef, GeometryResourceArena, ObjectId,
    Property, RateFunction, Rect, SemanticClickIndicate, Style, TextResource, TextResourceArena,
    TextSourceKind, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
    VectorPath,
};
use noon_runtime::{EvaluationError, PrefixError, PrefixReader, ReplayLimits, SceneInstance};

fn object(id: u64) -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.2),
        Transform2D::IDENTITY,
        Style {
            fill: Some(Color::BLUE),
            ..Style::default()
        },
    )
}

fn source(tracks: &[TrackDefinition], frontier: f64) -> SceneInstance {
    let mut source =
        SceneInstance::new(CompiledScene::compile_objects(vec![object(0)], tracks).unwrap());
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 64,
            payloads: 256,
        })
        .unwrap();
    source.advance_to(frontier).unwrap();
    source
}

fn binding() -> SemanticClickIndicate {
    SemanticClickIndicate::new(1.5, Color::YELLOW, 2.0)
}

fn start(reader: &mut PrefixReader, wall_time: f64) {
    let effect = reader
        .prepare_click_indicate(ObjectId::new(0), binding())
        .unwrap()
        .unwrap();
    reader.start_transient_animation(effect).unwrap();
    reader.advance_interactions(wall_time).unwrap();
}

fn track(property: Property, start: f64, duration: f64) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(1),
        object: ObjectId::new(0),
        property,
        values: match property {
            Property::Rotation => TrackValues::Scalar { from: 0.0, to: 2.0 },
            Property::Scale => TrackValues::Vec2 {
                from: Vec2::new(1.0, 1.0),
                to: Vec2::new(2.0, 2.0),
            },
            _ => unreachable!(),
        },
        timing: TrackTiming::new(start, duration, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    }
}

#[test]
fn paused_effect_changes_only_reader_presentation_and_releases_to_the_same_history() {
    let source = source(&[], 1.0);
    let source_frame = source.frame().clone();
    let source_context = source.publication_context();
    let (_, mut reader) = source.create_prefix_reader().unwrap();
    let baseline = reader.seek(0.5).unwrap().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();

    start(&mut reader, 10.0);
    reader.advance_interactions(11.0).unwrap();
    assert_eq!(reader.frame().time, 0.5);
    assert_eq!(
        reader.frame().objects[0].transform.scale,
        Vec2::new(1.5, 1.5)
    );
    assert_eq!(reader.frame().objects[0].style.fill, Some(Color::YELLOW));
    assert!(reader.interactions_active());
    assert_eq!(reader.replay_stats(), history);
    let publication = reader.take_renderer_publication();
    assert_eq!(
        publication.context().scene_revision(),
        context.scene_revision()
    );
    assert_eq!(
        publication.context().execution_revision(),
        context.execution_revision()
    );
    assert_ne!(publication.context().frame_epoch(), context.frame_epoch());

    reader.advance_interactions(12.0).unwrap();
    assert_eq!(reader.frame(), &baseline);
    assert!(!reader.interactions_active());
    assert_eq!(reader.replay_stats(), history);
    let settled = reader.take_renderer_publication().context();
    reader.advance_interactions(20.0).unwrap();
    let publication = reader.take_renderer_publication();
    assert_eq!(publication.context(), settled);
    assert!(publication.changes().is_empty());
    assert_eq!(source.frame(), &source_frame);
    assert_eq!(source.publication_context(), source_context);
}

#[test]
fn append_keeps_effect_and_frame_while_forward_playback_advances_disjoint_channels() {
    let mut source = source(&[track(Property::Rotation, 0.0, 4.0)], 1.0);
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    reader.seek(0.25).unwrap();
    start(&mut reader, 10.0);
    reader.advance_interactions(10.5).unwrap();
    let displayed = reader.frame().clone();
    let context = reader.take_renderer_publication().context();

    source
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(0),
            style: Style {
                fill: Some(Color::RED),
                ..Style::default()
            },
        })
        .unwrap();
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().revisions, 1);
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &displayed);
    assert!(reader.interactions_active());
    let publication = reader.take_renderer_publication();
    assert_eq!(publication.context(), context);
    assert!(publication.changes().is_empty());

    reader.advance_to(0.75).unwrap();
    reader.advance_interactions(11.0).unwrap();
    assert_eq!(reader.frame().time, 0.75);
    assert_eq!(reader.frame().objects[0].transform.rotation, 0.375);
    assert_eq!(
        reader.frame().objects[0].transform.scale,
        Vec2::new(1.5, 1.5)
    );
    assert_eq!(reader.frame().objects[0].style.fill, Some(Color::YELLOW));
    assert!(reader.interactions_active());
    reader.advance_interactions(12.0).unwrap();
    assert_eq!(reader.frame().objects[0].transform.rotation, 0.375);
    assert_eq!(
        reader.frame().objects[0].transform.scale,
        Vec2::new(1.0, 1.0)
    );
    assert_eq!(reader.frame().objects[0].style.fill, Some(Color::BLUE));
    assert!(!reader.interactions_active());
    reader.advance_to(1.5).unwrap();
    assert_eq!(reader.frame().objects[0].style.fill, Some(Color::RED));
}

#[test]
fn invalid_requests_preserve_active_effect_and_do_not_materialize_future_slots() {
    let mut source = source(&[], 1.0);
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    reader.seek(0.5).unwrap();
    start(&mut reader, 10.0);
    reader.advance_interactions(10.5).unwrap();
    reader.reserve_object_capacity(2).unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(1)))
        .unwrap();
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    let displayed = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    assert_eq!(displayed.objects.len(), 1);

    for time in [-1.0, 2.0, 3.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            reader.advance_to(time),
            Err(PrefixError::OutsideRange { .. })
        ));
        assert_eq!(reader.frame(), &displayed);
    }
    assert!(matches!(
        reader.advance_to(0.25),
        Err(PrefixError::Evaluation(
            EvaluationError::NonMonotonicPreparedAdvance {
                current: 0.5,
                requested: 0.25,
            }
        ))
    ));
    assert!(matches!(
        reader.prepare_click_indicate(ObjectId::new(1), binding()),
        Err(PrefixError::Interaction(_))
    ));
    for wall_time in [10.25, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            reader.advance_interactions(wall_time),
            Err(PrefixError::Interaction(_))
        ));
    }
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.replay_stats(), history);
    assert!(reader.interactions_active());
    let publication = reader.take_renderer_publication();
    assert_eq!(publication.context(), context);
    assert!(publication.changes().is_empty());

    reader.advance_interactions(11.0).unwrap();
    assert_eq!(
        reader.frame().objects[0].transform.scale,
        Vec2::new(1.5, 1.5)
    );
    reader.advance_to(1.5).unwrap();
    reader.advance_interactions(11.5).unwrap();
    assert_eq!(reader.frame().objects.len(), 2);
    assert!(reader
        .prepare_click_indicate(ObjectId::new(1), binding())
        .unwrap()
        .is_some());
}

#[test]
fn foreign_and_stale_prepared_effects_are_rejected_without_publication() {
    let source = source(&[], 2.0);
    let (_, mut first) = source.create_prefix_reader().unwrap();
    let (_, mut second) = source.create_prefix_reader().unwrap();
    first.seek(0.5).unwrap();
    second.seek(0.5).unwrap();
    let prepared = first
        .prepare_click_indicate(ObjectId::new(0), binding())
        .unwrap()
        .unwrap();
    let frame = second.frame().clone();
    let context = second.take_renderer_publication().context();
    assert!(matches!(
        second.start_transient_animation(prepared.clone()),
        Err(PrefixError::Interaction(_))
    ));
    assert_eq!(second.frame(), &frame);
    assert!(!second.interactions_active());
    assert_eq!(second.take_renderer_publication().context(), context);

    first.advance_to(0.75).unwrap();
    let frame = first.frame().clone();
    let context = first.take_renderer_publication().context();
    assert!(matches!(
        first.start_transient_animation(prepared),
        Err(PrefixError::Interaction(_))
    ));
    assert_eq!(first.frame(), &frame);
    assert!(!first.interactions_active());
    let publication = first.take_renderer_publication();
    assert_eq!(publication.context(), context);
    assert!(publication.changes().is_empty());
}

#[test]
fn explicit_same_time_seek_cancels_effect_but_failed_seek_does_not() {
    let source = source(&[], 1.0);
    let (_, mut reader) = source.create_prefix_reader().unwrap();
    let baseline = reader.seek(0.5).unwrap().clone();
    start(&mut reader, 10.0);
    reader.advance_interactions(11.0).unwrap();
    let highlighted = reader.frame().clone();
    let context = reader.take_renderer_publication().context();
    assert!(reader.seek(1.0).is_err());
    assert_eq!(reader.frame(), &highlighted);
    assert!(reader.interactions_active());
    assert_eq!(reader.take_renderer_publication().context(), context);

    assert_eq!(reader.seek(0.5).unwrap(), &baseline);
    assert!(!reader.interactions_active());
    assert_ne!(
        reader.take_renderer_publication().context().frame_epoch(),
        context.frame_epoch()
    );
    reader.advance_interactions(12.0).unwrap();
    assert_eq!(reader.frame(), &baseline);
}

#[test]
fn historical_target_remains_interactive_after_producer_removal_and_destruction() {
    let mut source = source(&[], 1.0);
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    let baseline = reader.seek(0.5).unwrap().clone();
    source
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(0)))
        .unwrap();
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    drop(source);

    start(&mut reader, 10.0);
    reader.advance_interactions(11.0).unwrap();
    assert_eq!(reader.frame().objects[0].style.fill, Some(Color::YELLOW));
    assert_eq!(reader.take_renderer_publication().painter_order(), &[0]);
    reader.advance_to(1.5).unwrap();
    reader.advance_interactions(11.5).unwrap();
    assert!(!reader.interactions_active());
    assert!(reader
        .take_renderer_publication()
        .painter_order()
        .is_empty());
    assert!(matches!(
        reader.prepare_click_indicate(ObjectId::new(0), binding()),
        Err(PrefixError::Interaction(_))
    ));
    assert_eq!(reader.seek(0.5).unwrap(), &baseline);
    start(&mut reader, 20.0);
    reader.advance_interactions(21.0).unwrap();
    assert_eq!(reader.frame().objects[0].style.fill, Some(Color::YELLOW));
}

#[test]
fn authored_scale_ownership_defers_or_supersedes_the_effect_without_stale_restore() {
    let authored = source(&[track(Property::Scale, 0.0, 3.0)], 3.0);
    let (_, mut active) = authored.create_prefix_reader().unwrap();
    active.seek(0.5).unwrap();
    assert!(active
        .prepare_click_indicate(ObjectId::new(0), binding())
        .unwrap()
        .is_none());

    for (duration, destination) in [(1.0, 1.5), (1.0, 2.5), (0.0, 2.5)] {
        let source = source(&[track(Property::Scale, 1.0, duration)], 3.0);
        let (_, mut reader) = source.create_prefix_reader().unwrap();
        let (_, mut reference) = source.create_prefix_reader().unwrap();
        reader.seek(0.5).unwrap();
        start(&mut reader, 10.0);
        reader.advance_interactions(10.5).unwrap();
        reader.advance_to(destination).unwrap();
        reader.advance_interactions(11.0).unwrap();
        assert!(!reader.interactions_active());
        assert_eq!(reader.frame(), reference.seek(destination).unwrap());
    }
}

#[test]
fn nondrawable_historical_targets_cannot_acquire_a_presentation_effect() {
    let mut source = source(&[], 1.0);
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::SetDrawEnabled {
            object: ObjectId::new(0),
            enabled: false,
        })
        .unwrap();
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    reader.advance_to(1.5).unwrap();
    assert!(!reader.frame().objects[0].draw_enabled);
    assert!(matches!(
        reader.prepare_click_indicate(ObjectId::new(0), binding()),
        Err(PrefixError::Interaction(_))
    ));
    reader.seek(0.5).unwrap();
    assert!(reader
        .prepare_click_indicate(ObjectId::new(0), binding())
        .unwrap()
        .is_some());
}

#[test]
fn unsupported_displayed_targets_preserve_the_active_effect_and_publication() {
    let mut texts = TextResourceArena::new();
    let text = texts
        .insert(TextResource {
            source: Arc::from("unsupported text"),
            kind: TextSourceKind::Plain,
            runs: Arc::from([]),
            vector_items: Arc::from([]),
            render_items: Arc::from([]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
            baseline: 0.0,
            layout_artifact: None,
        })
        .unwrap();
    let mut resources = CompiledResources::default();
    let text_bounds = resources
        .capture_text_from_arenas(
            &texts,
            &FontResourceArena::new(),
            &GeometryResourceArena::new(),
            text,
        )
        .unwrap();
    let mut path = object(1);
    path.content = GeometryRef::path(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(0.5, 0.0))
            .line_to(Vec2::new(0.0, 0.5))
            .close(),
    )
    .into();
    let mut text_object = object(2);
    text_object.content = text.into();
    text_object.text_bounds = Some(text_bounds);
    let mut outline = object(3);
    outline.base_style.fill = None;
    outline.base_style.stroke = Some(Color::BLUE);
    let mut line = object(4);
    line.content = GeometryRef::line(Vec2::ZERO, Vec2::ONE).into();
    let mut rectangle = object(5);
    rectangle.content = GeometryRef::rectangle(0.5, 0.25).into();
    let mut compiled = CompiledScene::compile_objects(
        vec![object(0), path, text_object, outline, line, rectangle],
        &[],
    )
    .unwrap();
    compiled.merge_prepared_resources(resources);
    let mut source = SceneInstance::new(compiled);
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 64,
            payloads: 256,
        })
        .unwrap();
    source.advance_to(1.0).unwrap();
    let (_, mut reader) = source.create_prefix_reader().unwrap();
    let baseline = reader.seek(0.5).unwrap().clone();
    start(&mut reader, 10.0);
    reader.advance_interactions(10.5).unwrap();
    let displayed = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();

    for id in 1..=4 {
        assert!(reader.frame().presences[id as usize]);
        assert!(reader.frame().objects[id as usize].draw_enabled);
        assert!(matches!(
            reader.prepare_click_indicate(ObjectId::new(id), binding()),
            Err(PrefixError::Interaction(_))
        ));
        assert_eq!(reader.frame(), &displayed);
        assert_eq!(reader.replay_stats(), history);
        assert!(reader.interactions_active());
        let publication = reader.take_renderer_publication();
        assert_eq!(publication.context(), context);
        assert!(publication.changes().is_empty());
    }
    assert!(reader
        .prepare_click_indicate(ObjectId::new(5), binding())
        .unwrap()
        .is_some());
    reader.advance_interactions(11.0).unwrap();
    assert_eq!(reader.frame().objects[0].style.fill, Some(Color::YELLOW));
    reader.advance_interactions(12.0).unwrap();
    assert_eq!(reader.frame(), &baseline);
    assert!(!reader.interactions_active());
}
