use noon_core::{ObjectContentRef, ObjectId, Style, TextResourceArena, Transform2D, Vec2};
use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};
use noon_text_render_wgpu::{
    PreparedTextItem, RetainedTextIncrementalStats, RetainedTextQuadPreparer, TextDeviceMetrics,
};
use noon_typst::{compile_typst_resource, TypstMode};

const OBJECT_COUNT: usize = 10_000;
const UPDATE_FRAMES: usize = 128;
const CHANGED_INDEX: usize = OBJECT_COUNT / 2;

fn large_text_frame(text: noon_core::TextResourceHandle) -> RetainedFrameState {
    let transform = Transform2D {
        scale: Vec2::new(0.05, 0.05),
        ..Transform2D::IDENTITY
    };
    RetainedFrameState {
        time: 0.0,
        objects: (0..OBJECT_COUNT)
            .map(|index| RetainedFrameObjectState {
                id: ObjectId::new(index as u64),
                content: ObjectContentRef::Text(text),
                transform,
                style: Style::default(),
                appearance: 1.0,
            })
            .collect(),
        presences: vec![true; OBJECT_COUNT],
        reveals: vec![1.0; OBJECT_COUNT],
        morphs: vec![0.0; OBJECT_COUNT],
        render_geometries: vec![None; OBJECT_COUNT],
        render_transforms: vec![None; OBJECT_COUNT],
    }
}

fn single_text_frame(text: noon_core::TextResourceHandle) -> RetainedFrameState {
    let transform = Transform2D {
        scale: Vec2::new(0.05, 0.05),
        ..Transform2D::IDENTITY
    };
    RetainedFrameState {
        time: 0.0,
        objects: vec![RetainedFrameObjectState {
            id: ObjectId::new(1),
            content: ObjectContentRef::Text(text),
            transform,
            style: Style::default(),
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![1.0],
        morphs: vec![0.0],
        render_geometries: vec![None],
        render_transforms: vec![None],
    }
}

#[test]
fn one_changed_text_object_stays_object_local_after_warmup() {
    let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let text = texts.insert(artifact.resource).unwrap();
    let mut frame = large_text_frame(text);
    let metrics = TextDeviceMetrics::uniform(67.5).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

    let prepared = preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::all(),
            &texts,
            &artifact.fonts,
            metrics,
        )
        .unwrap();
    assert_eq!(prepared.stats.text_objects, OBJECT_COUNT);
    assert!(!prepared.mask_quads.is_empty());

    let raster_after_warmup = preparer.raster_stats();
    let atlas_after_warmup = preparer.atlas_stats();

    for frame_index in 1..=UPDATE_FRAMES {
        frame.time = frame_index as f64 / 60.0;
        frame.objects[CHANGED_INDEX].transform.translation =
            Vec2::new(frame_index as f32 * 0.01, -(frame_index as f32) * 0.005);
        frame.objects[CHANGED_INDEX].style.opacity =
            1.0 - (frame_index as f32 / UPDATE_FRAMES as f32) * 0.25;

        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::objects(vec![CHANGED_INDEX]),
                &texts,
                &artifact.fonts,
                metrics,
            )
            .unwrap();
    }

    assert_eq!(preparer.raster_stats(), raster_after_warmup);
    assert_eq!(preparer.atlas_stats(), atlas_after_warmup);
    assert_eq!(
        preparer.incremental_stats(),
        RetainedTextIncrementalStats {
            rebuild_attempts: 1,
            reused_frames: 0,
            object_update_frames: UPDATE_FRAMES as u64,
            objects_updated: UPDATE_FRAMES as u64,
            fallback_rebuilds: 0,
        }
    );
}

#[test]
fn one_resident_outline_object_stays_local_among_static_glyphs() {
    let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let text = texts.insert(artifact.resource).unwrap();
    let mut frame = large_text_frame(text);
    frame.reveals[CHANGED_INDEX] = 0.5;

    let metrics = TextDeviceMetrics::uniform(67.5).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

    let prepared = preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::all(),
            &texts,
            &artifact.fonts,
            metrics,
        )
        .unwrap();
    assert_eq!(prepared.stats.text_objects, OBJECT_COUNT);
    assert_eq!(prepared.stats.outline_runs, 1);
    assert!(prepared.items.iter().any(|item| matches!(
        item,
        PreparedTextItem::OutlineRun {
            object_index,
            reveal,
            ..
        } if *object_index as usize == CHANGED_INDEX && *reveal == 0.5
    )));
    assert!(!prepared.mask_quads.is_empty());

    let raster_after_warmup = preparer.raster_stats();
    let atlas_after_warmup = preparer.atlas_stats();

    for frame_index in 1..=UPDATE_FRAMES {
        frame.time = frame_index as f64 / 60.0;
        frame.objects[CHANGED_INDEX].transform.translation =
            Vec2::new(-(frame_index as f32) * 0.01, frame_index as f32 * 0.004);
        frame.objects[CHANGED_INDEX].style.opacity =
            1.0 - (frame_index as f32 / UPDATE_FRAMES as f32) * 0.2;

        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::objects(vec![CHANGED_INDEX]),
                &texts,
                &artifact.fonts,
                metrics,
            )
            .unwrap();
        assert_eq!(prepared.stats.outline_runs, 1);
    }

    assert_eq!(preparer.raster_stats(), raster_after_warmup);
    assert_eq!(preparer.atlas_stats(), atlas_after_warmup);
    assert_eq!(
        preparer.incremental_stats(),
        RetainedTextIncrementalStats {
            rebuild_attempts: 1,
            reused_frames: 0,
            object_update_frames: UPDATE_FRAMES as u64,
            objects_updated: UPDATE_FRAMES as u64,
            fallback_rebuilds: 0,
        }
    );
}

#[test]
fn atlas_generation_advances_only_for_full_rebuilds() {
    let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let text = texts.insert(artifact.resource).unwrap();
    let mut frame = single_text_frame(text);
    let metrics = TextDeviceMetrics::uniform(67.5).unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

    let initial_generation = preparer.atlas().generation();
    preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::all(),
            &texts,
            &artifact.fonts,
            metrics,
        )
        .unwrap();
    let rebuilt_generation = preparer.atlas().generation();
    assert_eq!(rebuilt_generation, initial_generation + 1);

    preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::default(),
            &texts,
            &artifact.fonts,
            metrics,
        )
        .unwrap();
    assert_eq!(preparer.atlas().generation(), rebuilt_generation);

    frame.objects[0].transform.translation = Vec2::new(1.0, -2.0);
    preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::objects(vec![0]),
            &texts,
            &artifact.fonts,
            metrics,
        )
        .unwrap();
    assert_eq!(preparer.atlas().generation(), rebuilt_generation);

    frame.objects[0].transform.scale = Vec2::new(0.1, 0.1);
    preparer
        .prepare_with_changes(
            &device,
            &queue,
            &frame,
            &FrameChanges::objects(vec![0]),
            &texts,
            &artifact.fonts,
            metrics,
        )
        .unwrap();
    assert_eq!(preparer.atlas().generation(), rebuilt_generation + 1);
}
