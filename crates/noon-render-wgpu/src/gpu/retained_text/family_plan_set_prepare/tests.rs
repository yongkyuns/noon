use std::collections::BTreeSet;

use noon_core::{
    FamilyAnimationMode, FamilyAnimationState, RateFunction,
    RetainedFamilyAnimationPlanBuilder, SemanticStore,
};
use noon_text::shaping::{NativeFontFace, NativeTextCompiler, NativeTextOptions};

use super::*;

struct Fixture {
    frame: FrameState,
    plans: Vec<RetainedFamilyAnimationPlan>,
    texts: TextResourceArena,
    fonts: FontResourceArena,
    geometries: GeometryResourceArena,
}

impl Fixture {
    fn new() -> Self {
        // Borrow an existing bundled test face, then use the real plain-text
        // shaper. Family Write intentionally does not accept Typst resources.
        let seed = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let face = &seed.resource.runs[0].font;
        let data = seed.fonts.get_for_face(face).unwrap().data.clone();
        let font = NativeFontFace::new(face.family.clone(), data, face.face_index).unwrap();
        let artifact = NativeTextCompiler::new()
            .compile_plain("ABA", &font, &NativeTextOptions::new(1.0)).unwrap();
        let bounds = artifact.resource.bounds;
        let mut texts = TextResourceArena::new();
        let text = texts.insert(artifact.resource).unwrap();
        let objects = vec![
            FrameObjectState {
                id: ObjectId::new(1),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            },
            FrameObjectState {
                id: ObjectId::new(2),
                z_index: 0.0,
                content: ObjectContentRef::Text(text),
                text_bounds: Some(bounds),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            },
        ];
        let mut store = SemanticStore::new();
        let leaf = store.insert_authoring_object();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, leaf).unwrap();
        builder.accept_leaf(leaf, objects[1].id, &objects[1].content, &texts).unwrap();
        let plan = builder.finish().unwrap();
        Self {
            frame: FrameState {
                time: 0.0,
                objects,
                presences: vec![true; 2],
                reveals: vec![0.5, 1.0],
                morphs: vec![0.0; 2],
                render_geometries: vec![None; 2],
                render_transforms: vec![None; 2],
                family_animations: vec![None, Some(FamilyAnimationState {
                    mode: FamilyAnimationMode::DrawBorderThenFill,
                    overall_progress: 0.8,
                    lag_ratio: 0.0,
                    rate_function: RateFunction::Linear,
                    reverse_rate_function: false,
                    reverse_member_order: false,
                })],
                family_animation_plan_indices: vec![None, Some(0)],
            },
            plans: vec![plan],
            texts,
            fonts: artifact.fonts,
            geometries: GeometryResourceArena::new(),
        }
    }

    fn prepare<'a>(
        &self,
        preparer: &'a mut RetainedFramePreparer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        changes: &FrameChanges,
        visible: Option<&[usize]>,
    ) -> PreparedRetainedGpuFrame<'a> {
        let frame = RetainedPlannedFamilyFrame {
            retained: &self.frame,
            family_animations: &self.frame.family_animations,
            family_plan_indices: &self.frame.family_animation_plan_indices,
        };
        preparer.prepare_family_plan_set_with_changes_inner(
            device, queue, &frame, &self.plans, changes, &self.texts, &self.fonts,
            &self.geometries, TextDeviceMetrics::uniform(100.0).unwrap(), visible,
            Some(&BTreeSet::from([1])),
        ).unwrap()
    }
}

fn assert_valid_draw_ranges(prepared: &PreparedRetainedGpuFrame<'_>) {
    for item in prepared.render_items {
        if let RetainedRenderItem::Geometry { batch, .. } = item {
            match batch.primitive {
                RenderPrimitive::Path { batch: index } => {
                    let path = prepared.geometry.path_batches.get(index)
                        .expect("mixed painter item must reference a current path batch");
                    assert!(batch.instance_range.start >= path.instance_range.start);
                    assert!(batch.instance_range.end <= path.instance_range.end);
                    assert!(path.index_range.end as usize <= prepared.geometry.path_indices.len());
                }
                RenderPrimitive::Circle => assert!(batch.instance_range.end as usize <= prepared.geometry.circles.len()),
                RenderPrimitive::Rectangle => assert!(batch.instance_range.end as usize <= prepared.geometry.rectangles.len()),
                RenderPrimitive::Line => assert!(batch.instance_range.end as usize <= prepared.geometry.lines.len()),
                RenderPrimitive::MegaPath { .. } => panic!("mixed text uses individual path draws"),
            }
        }
    }
}

#[test]
fn cached_family_geometry_rebuild_remaps_painter_items() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    for visible in [None, Some(&[0, 1][..]), Some(&[1][..])] {
        let mut fixture = Fixture::new();
        let mut preparer = RetainedFramePreparer::new();
        let initial_batches = {
            let initial = fixture.prepare(&mut preparer, &device, &queue, &FrameChanges::all(), visible);
            assert_valid_draw_ranges(&initial);
            initial.geometry.path_batches.len()
        };
        let before = preparer.incremental_stats;
        // The unrelated analytic Create reaches its endpoint while the same text
        // family remains active. Its temporary path becomes a circle; the child
        // must repack paths, but the parent's active-family signature is unchanged.
        fixture.frame.reveals[0] = 1.0;
        fixture.frame.time = 0.1;
        let observed = {
            let prepared = fixture.prepare(&mut preparer, &device, &queue, &FrameChanges::objects(vec![0]), visible);
            assert_eq!(prepared.geometry.stats.full_rebuilds, 1);
            assert!(prepared.geometry.path_batches.len() < initial_batches);
            assert_valid_draw_ranges(&prepared);
            prepared.render_items.to_vec()
        };
        assert_eq!(preparer.incremental_stats.scratch_rebuilds, before.scratch_rebuilds);
        assert_eq!(preparer.incremental_stats.mixed_order_rebuilds, before.mixed_order_rebuilds + 1);
        // Independent full preparation is the oracle; do not merely check that
        // stale indices are in bounds or discard draws that no longer fit.
        let expected = fixture.prepare(&mut RetainedFramePreparer::new(), &device, &queue,
            &FrameChanges::all(), visible).render_items.to_vec();
        assert_eq!(observed, expected);
        let rebuilt = preparer.incremental_stats;
        let quiet = fixture.prepare(&mut preparer, &device, &queue, &FrameChanges::default(), visible);
        assert_valid_draw_ranges(&quiet);
        assert_eq!(quiet.geometry.stats.full_rebuilds, 0);
        assert_eq!(quiet.render_items, observed);
        assert_eq!(preparer.incremental_stats.mixed_order_rebuilds, rebuilt.mixed_order_rebuilds);
    }
}

#[test]
fn cached_family_transform_update_keeps_painter_topology_local() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut fixture = Fixture::new();
    fixture.frame.reveals[0] = 1.0;
    let mut preparer = RetainedFramePreparer::new();
    let before_items = fixture.prepare(&mut preparer, &device, &queue, &FrameChanges::all(),
        Some(&[0, 1])).render_items.to_vec();
    let before = preparer.incremental_stats;
    fixture.frame.objects[0].transform.translation.x = 2.0;
    let prepared = fixture.prepare(&mut preparer, &device, &queue, &FrameChanges::objects(vec![0]),
        Some(&[0, 1]));
    assert_valid_draw_ranges(&prepared);
    assert_eq!(prepared.geometry.stats.full_rebuilds, 0);
    assert_eq!(prepared.render_items, before_items);
    assert_eq!(preparer.incremental_stats.scratch_rebuilds, before.scratch_rebuilds);
    assert_eq!(preparer.incremental_stats.mixed_order_rebuilds, before.mixed_order_rebuilds);
}
