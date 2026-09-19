//! Real software-Vulkan pixel proof. Scene-only capture is intentionally exercised
//! on the same renderer while the separate interactive GPU overlay stays resident.
use noon::integration::{
    NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind,
    NativePointerPosition,
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
