//! Source edits preserve allocated Code layout and use ordinary live animation.
#![cfg(all(feature = "code", feature = "bundled-fonts", feature = "image-decode"))]

use noon::{
    example_scenes::code_source_update::{self, PADDED_DECLARATION},
    AnimationCompositionRequest, AnimationOptions, CodeHighlightOptions, FadeEndpoint,
    LiveProgramStatus, RateFunction, RustHostCallbackTable, Scene,
    SemanticAnimationCompositionKind, SemanticFadeDirection,
};

// This widescreen proof leaves the square constructors to the other raster tests.
#[allow(dead_code)]
mod raster_support;
use raster_support::Raster;

fn retain(name: &str, pixels: &[u8], width: u32, height: u32) {
    if let Some(directory) = std::env::var_os("NOON_CODE_PROOF_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            std::path::Path::new(&directory).join(format!("{name}.png")),
            pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn native_source_edit_continuation_completes_normal_fade_and_write() {
    let mut program = code_source_update::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    for end in [0.2, 0.95, 1.15] {
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        match program.drive_to(&mut callbacks, end).unwrap() {
            LiveProgramStatus::PublicationPending(expected) => {
                let context = program.take_renderer_publication().context();
                assert_eq!(context, expected);
                program.admit_publication(context).unwrap();
            }
            LiveProgramStatus::ReadyToResume => {}
            status => panic!("expected completion at {end}, got {status:?}"),
        }
    }
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert!(program
        .session()
        .planned_family_frame()
        .family_animations
        .iter()
        .all(Option::is_none));
}

#[test]
#[ignore = "requires software Vulkan; run with --ignored --nocapture"]
fn native_code_source_edit_changes_only_one_row_and_settles_without_uploads() {
    let mut scene = Scene::new();
    let code = code_source_update::listing(&mut scene).unwrap();
    let unchanged: Vec<_> = (1..10)
        .map(|row| {
            let line = code.line(row).unwrap();
            (line.node_id(), line.state().unwrap())
        })
        .collect();
    let fixed_families = [
        code.background(),
        code.line_numbers().unwrap(),
        code.highlights(),
    ];
    let fixed_bounds: Vec<_> = fixed_families
        .iter()
        .map(|family| family.layout_bounds().unwrap())
        .collect();
    let region = code
        .get_highlighted_lines(0..1, CodeHighlightOptions::default())
        .unwrap()
        .layout_bounds()
        .unwrap()
        .unwrap();
    scene.add_many(&[code.family().into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let unchanged_slots: Vec<_> = unchanged
        .iter()
        .map(|(id, _)| session.execution_object_id(*id).unwrap())
        .collect();
    let (width, height) = (960, 540);
    let mut raster = pollster::block_on(Raster::with_dimensions(width, height));
    let before = raster.capture(&session.take_renderer_publication());
    let change;
    let segment;
    {
        let mut live = scene.live(&mut session);
        change = code
            .replace_line_in_live_session(&mut live, 0, PADDED_DECLARATION)
            .unwrap();
        live.add(&change.previous).unwrap();
        let options = AnimationOptions::new()
            .run_time(0.75)
            .rate_func(RateFunction::Linear);
        segment = live
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &[
                    AnimationCompositionRequest::Fade {
                        target: &change.previous,
                        direction: SemanticFadeDirection::Out,
                        endpoint: FadeEndpoint::default(),
                        options: options.remover(true),
                    },
                    AnimationCompositionRequest::TextWrite {
                        target: &change.current,
                        reverse_member_order: false,
                        options: options.introducer(true),
                    },
                ],
                options,
                AnimationOptions::new(),
            )
            .unwrap();
        live.advance_segment_to(segment, 0.375).unwrap();
    }
    let halfway = raster.capture(&session.take_renderer_publication());
    {
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    let after = raster.capture(&session.take_renderer_publication());
    assert_ne!(
        halfway, before,
        "the ordinary row transition must be visible"
    );
    assert_ne!(halfway, after, "the transition must finish visibly");
    assert_ne!(change.previous.node_id(), change.current.node_id());
    assert_eq!(code.line(0).unwrap().node_id(), change.current.node_id());
    assert_eq!(&*code.line_source(0).unwrap(), PADDED_DECLARATION);
    for (index, (id, state)) in unchanged.iter().enumerate() {
        let line = code.line(index + 1).unwrap();
        assert_eq!(line.node_id(), *id);
        assert_eq!(line.state().unwrap(), *state);
        assert_eq!(
            session.execution_object_id(*id),
            Some(unchanged_slots[index])
        );
    }
    for (family, bounds) in fixed_families.iter().zip(fixed_bounds) {
        assert_eq!(family.layout_bounds().unwrap(), bounds);
    }
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        [code.family().node_id()],
        "FadeOut removes the previous row; Write does not promote the child to a scene root",
    );

    let mut changed = 0;
    let units_per_pixel = 8.0 / f64::from(height);
    for (index, (old, new)) in before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0)
        .enumerate()
    {
        if old == new {
            continue;
        }
        changed += 1;
        let x = (index % width as usize) as f64 + 0.5 - f64::from(width) * 0.5;
        let y = f64::from(height) * 0.5 - ((index / width as usize) as f64 + 0.5);
        let (x, y) = (x * units_per_pixel, y * units_per_pixel);
        assert!(
            x >= region.min_x - 2.0 * units_per_pixel
                && x <= region.max_x + 2.0 * units_per_pixel
                && y >= region.min_y - 2.0 * units_per_pixel
                && y <= region.max_y + 2.0 * units_per_pixel,
            "source replacement changed a pixel outside its allocated row at ({x}, {y})",
        );
    }
    assert!(
        changed > 100,
        "the longer source must change actual glyph pixels"
    );

    let mut reference_session = code_source_update::session(true).unwrap();
    let mut reference_raster = pollster::block_on(Raster::with_dimensions(width, height));
    let reference = reference_raster.capture(&reference_session.take_renderer_publication());
    assert_eq!(
        after, reference,
        "live completion must match independent cold admission"
    );
    let repeated = raster.capture(&session.take_renderer_publication());
    assert_eq!(after, repeated);
    assert_eq!(raster.last_scene_upload_bytes, 0);
    assert_eq!(raster.last_scene_repacked, 0);
    eprintln!("Code source replacement changed {changed} pixels within the reserved first row");
    retain("code-source-before", &before, width, height);
    retain("code-source-halfway", &halfway, width, height);
    retain("code-source-after", &after, width, height);
}
