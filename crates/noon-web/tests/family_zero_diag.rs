use noon::{LiveProgramStatus, RustHostCallbackTable};
use noon_core::{Rect, Vec2};
use noon_render_wgpu::{text::TextDeviceMetrics, RetainedFramePreparer};

fn submitted_instances(prepared: &noon_render_wgpu::PreparedRetainedGpuFrame<'_>) -> u32 {
    prepared
        .geometry_render_chunks()
        .flat_map(|chunk| chunk.render_batches.iter())
        .map(|batch| batch.instance_range.end - batch.instance_range.start)
        .sum()
}

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
    println!("INITIAL VISIBLE {:?}", visible.object_indices());

    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let metrics = TextDeviceMetrics::uniform(120.0).unwrap();
    let mut preparer = RetainedFramePreparer::new();

    let initial_context = {
        let publication = program.take_renderer_publication();
        println!(
            "INITIAL CHANGES all={} objects={:?} structural={} painter={} active_plans={:?}",
            publication.changes().is_all(),
            publication.changes().object_indices(),
            publication.changes().is_structural(),
            publication.changes().has_painter_order_change(),
            publication.active_family_animation_indices()
        );
        println!("INITIAL PAINTER {:?}", publication.painter_order());
        for (index, object) in publication.frame().objects.iter().enumerate() {
            println!(
                "INITIAL ROW {index}: id={} present={} content={:?} fill={:?} family={:?} plan={:?}",
                object.id.get(),
                publication.frame().presences[index],
                object.content,
                object.style.fill,
                publication.frame().family_animations[index],
                publication.frame().family_animation_plan_indices[index],
            );
        }
        assert_eq!(publication.frame().objects.len(), 2);
        assert_eq!(visible.object_indices(), &[0, 1]);
        let prepared = preparer
            .prepare_planned_publication_visible(
                &device,
                &queue,
                &publication,
                visible.object_indices(),
                metrics,
            )
            .expect("prepare initial family publication");
        let batches = prepared
            .geometry_render_chunks()
            .flat_map(|chunk| chunk.render_batches.iter())
            .map(|batch| (format!("{:?}", batch.primitive), batch.instance_range.clone()))
            .collect::<Vec<_>>();
        println!("INITIAL BATCHES {:?}", batches);
        println!("INITIAL PREPARED {:?}", prepared.geometry_stats());
        assert_eq!(submitted_instances(&prepared), 2);
        publication.context()
    };

    println!("STATUS BEFORE ADMIT {:?}", program.status());
    if matches!(program.status(), LiveProgramStatus::PublicationPending(_)) {
        program
            .admit_publication(initial_context)
            .expect("admit initial publication");
    }
    println!("STATUS AFTER ADMIT {:?}", program.status());
    if matches!(program.status(), LiveProgramStatus::ReadyToResume) {
        program.resume().expect("resume after initial admission");
    }

    // Mirror ExecutionCanvasRenderer::advanceDirectRealtime(0): drive to the same
    // authored time once the constructor publication has actually been presented.
    program
        .drive_to(&mut callbacks, 0.0)
        .expect("drive post-admission t=0");
    if matches!(program.status(), LiveProgramStatus::ReadyToResume) {
        program.resume().expect("resume after post-admission t=0 drive");
    }
    println!("STATUS AFTER SECOND DRIVE {:?}", program.status());
    println!("WAKE AFTER SECOND DRIVE {:?}", program.wake_state());

    let second_visible = program.query_viewport(viewport);
    println!("SECOND VISIBLE {:?}", second_visible.object_indices());
    assert_eq!(second_visible.object_indices(), &[0, 1]);

    let second = program.take_renderer_publication();
    println!(
        "SECOND CHANGES all={} empty={} objects={:?} structural={} painter={} active_plans={:?}",
        second.changes().is_all(),
        second.changes().is_empty(),
        second.changes().object_indices(),
        second.changes().is_structural(),
        second.changes().has_painter_order_change(),
        second.active_family_animation_indices()
    );
    println!("SECOND PAINTER {:?}", second.painter_order());
    for (index, object) in second.frame().objects.iter().enumerate() {
        println!(
            "SECOND ROW {index}: id={} present={} content={:?} fill={:?} family={:?} plan={:?}",
            object.id.get(),
            second.frame().presences[index],
            object.content,
            object.style.fill,
            second.frame().family_animations[index],
            second.frame().family_animation_plan_indices[index],
        );
    }

    // Only prepare when the post-admission drive actually published a new frame.
    // An empty change set means the browser correctly keeps the already-presented
    // initial GPU frame at 0 ms.
    if !second.changes().is_empty() {
        let prepared = preparer
            .prepare_planned_publication_visible(
                &device,
                &queue,
                &second,
                second_visible.object_indices(),
                metrics,
            )
            .expect("prepare post-admission t=0 publication");
        let batches = prepared
            .geometry_render_chunks()
            .flat_map(|chunk| chunk.render_batches.iter())
            .map(|batch| (format!("{:?}", batch.primitive), batch.instance_range.clone()))
            .collect::<Vec<_>>();
        println!("SECOND BATCHES {:?}", batches);
        println!("SECOND PREPARED {:?}", prepared.geometry_stats());
        assert_eq!(submitted_instances(&prepared), 2);
    }
}
