use super::*;
use crate::replay_revision::SavedRow;
use crate::{CompiledObject, CompiledTrack};
use noon_core::{
    CompositionTimeMap, ImageMapping, ImagePaint, ObjectId, PaintTransition, Property,
    RasterImageResourceArena, RasterImageResourceLookup, RasterImageSampling, RateFunction, Style,
    TrackId, TrackTiming, Transform2D,
};
use std::sync::Arc;

fn blank_scene() -> CompiledScene {
    CompiledScene::compile_objects(Vec::new(), &[]).unwrap()
}

fn image(handle: RasterImageResourceHandle) -> SemanticPaint {
    SemanticPaint::Image(Arc::new(ImagePaint::new(
        handle,
        ImageMapping::default(),
        RasterImageSampling::Linear,
    )))
}

#[test]
fn image_fill_and_historical_track_endpoints_are_pinned_once_per_handle() {
    let mut arena = RasterImageResourceArena::new();
    let row_handle = arena.intern_rgba8(1, 1, [11, 31, 73, 255]).unwrap();
    let track_handle = arena.intern_rgba8(1, 1, [17, 43, 97, 128]).unwrap();
    let row_paint = image(row_handle);
    let track_paint = image(track_handle);
    let mut row = CompiledObject::new(
        ObjectId::new(0),
        GeometryRef::rectangle(2.0, 1.0),
        Transform2D::IDENTITY,
        Style::default(),
    );
    row.fill_paint = Some(Arc::new(row_paint.clone()));
    let track = CompiledTrack {
        id: TrackId::new(0),
        object_index: 0,
        property: Property::Fill,
        values: TrackValues::Paint {
            transition: Arc::new(
                PaintTransition::new(
                    Some(track_paint.clone()),
                    1.0,
                    Some(track_paint.clone()),
                    0.25,
                )
                .unwrap(),
            ),
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
        transform_geometry_plan: None,
        reconciled: false,
    };
    let revision = CompiledReplayRevision {
        rows: vec![SavedRow {
            index: 0,
            object: Some(row),
            order: None,
        }],
        inset_2d_view: None,
        channels: Vec::new(),
        tracks: vec![(track.id, Some(track))],
        camera_motion_payloads: 0,
        path_motion_commands: 0,
        geometry_payload_bytes: 0,
    };
    let revisions = std::slice::from_ref(&revision);
    let mut scene = blank_scene();
    assert_eq!(
        scene.prefix_resource_delta(revisions, &CompiledResources::default()),
        Err(CompiledResourceError::MissingImage(row_handle))
    );
    let mut row_only = CompiledResources::default();
    row_only
        .capture_paint_from_arena(&arena, &row_paint)
        .unwrap();
    scene.merge_prepared_resources(row_only);
    assert_eq!(
        scene.prefix_resource_delta(revisions, &CompiledResources::default()),
        Err(CompiledResourceError::MissingImage(track_handle))
    );
    let mut track_pixels = CompiledResources::default();
    track_pixels
        .capture_paint_from_arena(&arena, &track_paint)
        .unwrap();
    scene.merge_prepared_resources(track_pixels);
    let bytes = arena.get(row_handle).unwrap().retained_bytes()
        + arena.get(track_handle).unwrap().retained_bytes();
    let resource = arena.get_shared(track_handle).unwrap();
    let weak = Arc::downgrade(&resource);
    drop(resource);
    // Neither the current object table nor current tracks reference these
    // pixels. The supplied historical revisions are the dependency authority.
    assert!(scene.objects().is_empty());
    assert!(scene.tracks().is_empty());
    let delta = scene
        .prefix_resource_delta(revisions, &CompiledResources::default())
        .unwrap();
    assert_eq!(delta.image_count(), 2);
    assert_eq!(delta.prefix_payload_count(), 2);
    assert_eq!(delta.prefix_resource_bytes(), Some(bytes));
    let next = scene.prefix_resource_delta(revisions, &delta).unwrap();
    assert_eq!(next.prefix_payload_count(), 0);
    assert_eq!(next.prefix_resource_bytes(), Some(0));
    drop((scene, arena));
    assert!(weak.upgrade().is_some());
    assert_eq!(
        RasterImageResourceLookup::get(&delta, row_handle)
            .unwrap()
            .rgba8(),
        &[11, 31, 73, 255]
    );
    assert_eq!(
        RasterImageResourceLookup::get(&delta, track_handle)
            .unwrap()
            .rgba8(),
        &[17, 43, 97, 128]
    );
    drop(delta);
    assert!(weak.upgrade().is_none());
}
