use noon::{LiveProgramStatus, RustHostCallbackTable};
use noon_core::{Rect, Vec2};
use noon_render_wgpu::{text::TextDeviceMetrics, RetainedFramePreparer};

#[test]
fn diagnose_family_state_zero_frame() {
    let mut program = noon::example_scenes::family_state::program().expect("family state program");
    let mut callbacks = RustHostCallbackTable::new();
    let status = program.resume().expect("resume stage zero");
    assert!(matches!(status, LiveProgramStatus::Awaiting(_)));
    let initial_time = program.session().frame().time;
    program
        .drive_to(&mut callbacks, initial_time)
        .expect("drive initial await");

    let viewport = Rect::new(Vec2::new(-8.0, -4.5), Vec2::new(8.0, 4.5));
    let visible = program.query_viewport(viewport);
    println!("VISIBLE {:?}", visible.object_indices());
    println!("SPATIAL {:?}", visible.spatial_stats());

    let publication = program.take_renderer_publication();
    println!("CHANGES all={} objects={:?} structural={} painter={} active_plans={:?}",
        publication.changes().is_all(),
        publication.changes().object_indices(),
        publication.changes().is_structural(),
        publication.changes().has_painter_order_change(),
        publication.active_family_animation_indices());
    println!("PAINTER {:?}", publication.painter_order());
    for (index, object) in publication.frame().objects.iter().enumerate() {
        println!(
            "ROW {index}: id={} present={} transform={:?} content={:?} render_geometry={:?} render_transform={:?} fill={:?} opacity={} reveal={} morph={} family={:?} plan={:?}",
            object.id.get(),
            publication.frame().presences[index],
            object.transform,
            object.content,
            publication.frame().render_geometries[index],
            publication.frame().render_transforms[index],
            object.style.fill,
            object.style.opacity,
            publication.frame().reveals[index],
            publication.frame().morphs[index],
            publication.frame().family_animations[index],
            publication.frame().family_animation_plan_indices[index],
        );
    }

    assert_eq!(publication.frame().objects.len(), 2);
    assert_eq!(visible.object_indices(), &[0, 1]);

    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let metrics = TextDeviceMetrics::uniform(120.0).unwrap();
    let mut preparer = RetainedFramePreparer::new();
    let prepared = preparer
        .prepare_planned_publication_visible(
            &device,
            &queue,
            &publication,
            visible.object_indices(),
            metrics,
        )
        .expect("prepare t=0 family publication");
    let mut submitted_instances = 0u32;
    let mut batch_count = 0usize;
    for chunk in prepared.geometry_render_chunks() {
        for batch in chunk.render_batches {
            println!("BATCH {:?} {:?}", batch.primitive, batch.instance_range);
            submitted_instances += batch.instance_range.end - batch.instance_range.start;
            batch_count += 1;
        }
    }
    println!("PREPARED batches={batch_count} instances={submitted_instances} stats={:?}", prepared.geometry_stats());
    assert_eq!(submitted_instances, 2, "both visible stable members must be submitted at t=0");
}
