//! Real software-Vulkan pixel proof. Scene-only capture is intentionally exercised
//! on the same renderer while the separate interactive GPU overlay stays resident.
use noon::integration::{
    NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerPosition,
};
use noon::{ExecutionSession, Scene, Text};
use noon_core::{Color, Vec2};
use noon_render_wgpu::AnalyticOverlay;

mod raster_support;
use raster_support::{Raster, SIZE, WORLD_SIZE};

fn click(session: &mut ExecutionSession, x: f32, sequence: u64) {
    let position = NativePointerPosition::new(
        Vec2::new(x, 0.0),
        Vec2::new((x / WORLD_SIZE + 0.5) * SIZE as f32, SIZE as f32 * 0.5),
    )
    .unwrap();
    for (offset, kind) in [
        NativePointerInputKind::Press {
            position,
            button: 0,
        },
        NativePointerInputKind::Release {
            position,
            button: 0,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let token = session.native_pointer_input_token().unwrap();
        let event = NativePointerInput::new(
            sequence + offset as u64,
            token.pointer(),
            token.context(),
            NativeInputModifiers::default(),
            kind,
        );
        session.submit_native_pointer_input(&token, event).unwrap();
    }
}

fn pointer_position(x: f32, y: f32) -> NativePointerPosition {
    NativePointerPosition::new(
        Vec2::new(x, y),
        Vec2::new(
            (x / WORLD_SIZE + 0.5) * SIZE as f32,
            (0.5 - y / WORLD_SIZE) * SIZE as f32,
        ),
    )
    .unwrap()
}

fn drag_event(
    scene: &Scene,
    session: &mut ExecutionSession,
    sequence: u64,
    kind: NativePointerInputKind,
) -> noon::TranslationDragReceipt {
    let token = session.native_pointer_input_token().unwrap();
    let input = NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    );
    let mut store = scene.integration_store().borrow_mut();
    session
        .submit_translation_drag_input(&mut store, &token, input)
        .unwrap()
}

fn pixel_at(pixels: &[u8], x: f32, y: f32) -> [u8; 4] {
    let px = ((x / WORLD_SIZE + 0.5) * SIZE as f32).floor() as usize;
    let py = ((0.5 - y / WORLD_SIZE) * SIZE as f32).floor() as usize;
    let offset = (py * SIZE as usize + px) * 4;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn assert_region_equal(before: &[u8], after: &[u8], x: f32, y: f32) {
    let center_x = ((x / WORLD_SIZE + 0.5) * SIZE as f32).floor() as isize;
    let center_y = ((0.5 - y / WORLD_SIZE) * SIZE as f32).floor() as isize;
    for row in center_y - 4..=center_y + 4 {
        for column in center_x - 4..=center_x + 4 {
            let offset = (row as usize * SIZE as usize + column as usize) * 4;
            assert_eq!(
                &before[offset..offset + 4],
                &after[offset..offset + 4],
                "unrelated/painter pixel changed at ({column},{row})"
            );
        }
    }
}

fn overlay(session: &ExecutionSession) -> Option<AnalyticOverlay> {
    session.pointer_selection_highlight().map(|highlight| {
        AnalyticOverlay::new(
            &highlight.geometry,
            highlight.transform,
            Color::rgba(1.0, 1.0, 0.0, 0.35),
        )
        .unwrap()
    })
}

fn retain(name: &str, pixels: &[u8]) {
    use std::io::Write;
    if let Some(dir) = std::env::var_os("NOON_SELECTION_PROOF_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let path = std::path::Path::new(&dir).join(format!("{name}.ppm"));
        let mut file = std::fs::File::create(path).unwrap();
        write!(file, "P6\n{SIZE} {SIZE}\n255\n").unwrap();
        for rgba in pixels.as_chunks::<4>().0 {
            file.write_all(&rgba[..3]).unwrap();
        }
    }
}

#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn paused_click_overlay_changes_only_exact_fill_pixels_and_never_exports() {
    let mut scene = Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    circle.set_stroke_width(0.0).unwrap();
    circle.set_translation(-1.5, 0.0).unwrap();
    circle.set_scale(-1.3, 0.6).unwrap();
    circle.set_rotation(0.6).unwrap();
    scene.add(&circle).unwrap();
    let mut rectangle = scene.square(1.6).unwrap();
    rectangle.set_fill(0.0, 0.65, 0.0, 1.0).unwrap();
    rectangle.set_stroke_width(0.0).unwrap();
    rectangle.set_translation(1.5, 0.0).unwrap();
    rectangle.set_rotation(-0.4).unwrap();
    scene.add(&rectangle).unwrap();
    let mut label = scene
        .text(Text::new("Paused selection").with_font_size(28.0))
        .unwrap();
    label.shift(0.0, -2.5).unwrap();
    scene.add(&label).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: 1,
                pointer: 0,
            },
            0,
        )
        .unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    let context = session.publication_context();
    let frame = session.frame().clone();
    let mut raster = pollster::block_on(Raster::new());
    let original = raster.capture(&session.take_renderer_publication());
    retain("scene", &original);

    for (step, x) in [-1.5, 1.5].into_iter().enumerate() {
        click(&mut session, x, (step * 2) as u64);
        let highlight = session.pointer_selection_highlight().unwrap();
        let projection = overlay(&session);
        let publication = session.take_renderer_publication();
        assert!(
            publication.changes().is_empty(),
            "selection is not scene dirtiness"
        );
        let pixels = raster.capture_interactive(&publication, projection);
        assert_eq!(raster.last_scene_upload_bytes, 0);
        assert_eq!(raster.last_scene_repacked, 0);
        assert_eq!(
            raster.last_overlay_upload_bytes,
            std::mem::size_of::<noon_render_wgpu::CircleInstance>()
        );
        let (sin, cos) = highlight.transform.rotation.sin_cos();
        let mut changed = 0;
        let original_pixels = original.as_chunks::<4>().0;
        let selected_pixels = pixels.as_chunks::<4>().0;
        for (i, (a, b)) in original_pixels.iter().zip(selected_pixels).enumerate() {
            if a == b {
                continue;
            }
            changed += 1;
            let world = Vec2::new(
                ((i % SIZE as usize) as f32 + 0.5) / SIZE as f32 * WORLD_SIZE - WORLD_SIZE * 0.5,
                WORLD_SIZE * 0.5 - ((i / SIZE as usize) as f32 + 0.5) / SIZE as f32 * WORLD_SIZE,
            );
            let d = world - highlight.transform.translation;
            let local = Vec2::new(
                (cos * d.x + sin * d.y) / highlight.transform.scale.x,
                (-sin * d.x + cos * d.y) / highlight.transform.scale.y,
            );
            let within = match highlight.geometry {
                noon_core::GeometryRef::Circle { radius } => {
                    local.x.hypot(local.y) <= radius + 0.08
                }
                noon_core::GeometryRef::Rectangle { size } => {
                    local.x.abs() <= size.x * 0.5 + 0.08 && local.y.abs() <= size.y * 0.5 + 0.08
                }
                _ => panic!("supported analytic projection"),
            };
            assert!(
                within,
                "overlay contaminated unrelated pixel {i}: {a:?} -> {b:?}"
            );
        }
        assert!(changed > 100, "highlight must visibly change actual pixels");
        let repeated = raster.capture_interactive(&session.take_renderer_publication(), projection);
        assert_eq!(pixels, repeated, "no accumulation across redraws");
        assert_eq!(raster.last_overlay_upload_bytes, 0);
        assert_eq!(raster.last_scene_upload_bytes, 0);
        let exported = raster.capture(&session.take_renderer_publication());
        assert_eq!(
            exported, original,
            "ordinary capture must omit a resident interactive overlay"
        );
        assert_eq!(session.frame(), &frame);
        assert_eq!(session.publication_context(), context);
        retain(
            if step == 0 {
                "circle-selected"
            } else {
                "rectangle-selected"
            },
            &pixels,
        );
        eprintln!(
            "selection pixels: target={step} changed={changed} authored_time={}",
            session.frame().time
        );
    }
    click(&mut session, 3.8, 4);
    assert!(session.selected_pointer_target().is_none());
    let cleared = raster.capture_interactive(&session.take_renderer_publication(), None);
    assert_eq!(
        cleared, original,
        "background click must remove every overlay pixel"
    );
    assert_eq!(raster.last_scene_upload_bytes, 0);
    assert_eq!(raster.last_overlay_upload_bytes, 0);
    assert!(session.wake_state().is_quiescent());
    retain("cleared", &cleared);
}

#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn paused_scene_drag_changes_target_pixels_then_release_or_cancel_resolves_once() {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.9).unwrap();
    target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    target.set_stroke_width(0.0).unwrap();
    target.set_translation(-1.5, 0.0).unwrap();
    scene.add(&target).unwrap();

    // This opaque later painter overlaps the target's destination. Its center
    // must remain red when the dragged circle moves underneath it.
    let mut foreground = scene.rectangle(1.0, 1.0).unwrap();
    foreground.set_fill(1.0, 0.0, 0.0, 1.0).unwrap();
    foreground.set_stroke_width(0.0).unwrap();
    foreground.set_translation(-0.5, 0.0).unwrap();
    scene.add(&foreground).unwrap();

    let mut unrelated = scene.square(1.0).unwrap();
    unrelated.set_fill(0.0, 0.75, 0.0, 1.0).unwrap();
    unrelated.set_stroke_width(0.0).unwrap();
    unrelated.set_translation(2.5, 1.0).unwrap();
    scene.add(&unrelated).unwrap();

    let mut session = scene.execution_session().unwrap();
    let pointer = NativePointerId {
        source: 2,
        pointer: 0,
    };
    session.configure_native_pointer_input(pointer, 0).unwrap();
    session
        .set_translation_drag_targets([target.node_id()])
        .unwrap();
    let mut raster = pollster::block_on(Raster::new());
    let original = raster.capture(&session.take_renderer_publication());
    assert!(
        pixel_at(&original, -1.5, 0.0)[2] > 180,
        "initial target is blue"
    );
    assert!(pixel_at(&original, -0.5, 0.0)[0] > 180, "foreground is red");
    let unrelated_before = pixel_at(&original, 2.5, 1.0);
    assert!(unrelated_before[1] > 120, "unrelated square is green");

    let initial_authored = target.state().unwrap().transform.translation;
    let initial_revision = scene.revision();
    for (sequence, x) in [(0, -1.5), (1, -0.5)] {
        let kind = if sequence == 0 {
            NativePointerInputKind::Press {
                position: pointer_position(x, 0.0),
                button: 0,
            }
        } else {
            NativePointerInputKind::Move(pointer_position(x, 0.0))
        };
        drag_event(&scene, &mut session, sequence, kind);
        if sequence == 0 {
            let _ = session.take_renderer_publication();
        }
    }
    assert_eq!(
        target.state().unwrap().transform.translation,
        initial_authored
    );
    assert_eq!(
        scene.revision(),
        initial_revision,
        "move stays session-local"
    );
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(-0.5, 0.0)
    );

    let moved = raster.capture(&session.take_renderer_publication());
    assert_ne!(moved, original, "drag must change real rendered pixels");
    assert!(
        pixel_at(&moved, -1.5, 0.0)[2] < 20,
        "old target location is vacated"
    );
    assert!(
        pixel_at(&moved, 0.1, 0.0)[2] > 180,
        "target appears at its new location"
    );
    assert_eq!(pixel_at(&moved, -0.5, 0.0), pixel_at(&original, -0.5, 0.0));
    assert_eq!(pixel_at(&moved, 2.5, 1.0), unrelated_before);
    assert_region_equal(&original, &moved, -0.5, 0.0);
    assert_region_equal(&original, &moved, 2.5, 1.0);
    let mut changed = 0;
    for (index, (before, after)) in original
        .as_chunks::<4>()
        .0
        .iter()
        .zip(moved.as_chunks::<4>().0)
        .enumerate()
    {
        if before == after {
            continue;
        }
        changed += 1;
        let world = Vec2::new(
            ((index % SIZE as usize) as f32 + 0.5) / SIZE as f32 * WORLD_SIZE - WORLD_SIZE * 0.5,
            WORLD_SIZE * 0.5 - ((index / SIZE as usize) as f32 + 0.5) / SIZE as f32 * WORLD_SIZE,
        );
        let in_target_sweep = [Vec2::new(-1.5, 0.0), Vec2::new(-0.5, 0.0)]
            .into_iter()
            .any(|center| (world - center).length() <= 1.0);
        assert!(in_target_sweep, "drag changed unrelated pixel {index}");
    }
    assert!(changed > 100, "drag must move visible target pixels");
    assert_eq!(raster.last_scene_repacked, 1);
    assert!(raster.last_scene_upload_bytes > 0);

    drag_event(
        &scene,
        &mut session,
        2,
        NativePointerInputKind::Cancel(NativePointerCancellation::CaptureLost),
    );
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(-1.5, 0.0)
    );
    assert_eq!(
        target.state().unwrap().transform.translation,
        initial_authored
    );
    assert_eq!(
        scene.revision(),
        initial_revision,
        "cancel does not author a move"
    );
    let cancelled = raster.capture(&session.take_renderer_publication());
    assert_eq!(
        cancelled, original,
        "cancel restores the original rendered frame"
    );

    drag_event(
        &scene,
        &mut session,
        3,
        NativePointerInputKind::Press {
            position: pointer_position(-1.5, 0.0),
            button: 0,
        },
    );
    let _ = session.take_renderer_publication();
    drag_event(
        &scene,
        &mut session,
        4,
        NativePointerInputKind::Move(pointer_position(-0.5, 0.0)),
    );
    assert_eq!(
        target.state().unwrap().transform.translation,
        initial_authored
    );
    let _ = raster.capture(&session.take_renderer_publication());
    let release_revision = scene.revision();
    let receipt = drag_event(
        &scene,
        &mut session,
        5,
        NativePointerInputKind::Release {
            position: pointer_position(-0.5, 0.0),
            button: 0,
        },
    );
    assert!(!session.translation_drag_active());
    assert_eq!(
        target.state().unwrap().transform.translation,
        noon_core::SemanticVec3::new(-0.5, 0.0, 0.0)
    );
    assert_eq!(scene.revision(), release_revision.checked_next().unwrap());
    assert!(
        receipt.undo.is_some(),
        "one release returns one authored undo"
    );
    let released = raster.capture(&session.take_renderer_publication());
    assert_eq!(
        released, moved,
        "release preserves the visible moved target"
    );
}
