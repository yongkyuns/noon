use super::*;
// Uses the enclosing direct presentation fixture.
use crate::browser_pointer_input::BrowserPointerKind;
use noon::{AnimationOptions, IndicateOptions};

const CENTER: Vec2 = Vec2::new(400.0, 200.0);
fn scroll(f: &mut Fixture, point: Vec2, delta: f64) -> Result<Option<bool>, String> {
    f.host
        .scroll(&mut f.session, &mut f.binding, 1, SIZE, point, delta)
}
fn present(f: &mut Fixture) {
    let frame = f
        .host
        .capture(&f.session, f.session.inspection_camera().unwrap())
        .unwrap();
    f.session.take_renderer_publication();
    f.host.did_present(frame);
}

#[test]
fn direct_inspection_keeps_anchor_and_uses_the_composed_camera_for_picking() {
    let mut f = Fixture::new();
    // Move the circle away from the wheel anchor so picking must change pixels.
    // Source creation below deliberately uses a normal shared Scene, not a fixture transform.
    let mut scene = noon::Scene::new();
    let mut circle = scene.circle(0.2).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    circle.set_translation(2.0, 0.0).unwrap();
    scene.add(&circle).unwrap();
    f.session = scene.execution_session().unwrap();
    f.session.enable_pointer_fill_selection(4.0).unwrap();
    present(&mut f);
    let original = f.session.frame().clone();
    let point = Vec2::new(450.0, 150.0);
    let anchor = f
        .host
        .presented
        .as_ref()
        .unwrap()
        .position(point)
        .unwrap()
        .scene();
    assert_eq!(
        scroll(&mut f, point, -500.0 * 2_f64.ln()).unwrap(),
        Some(true)
    );
    assert!(f.host.presented.is_none());
    assert_eq!(f.session.frame(), &original);
    present(&mut f);
    let frame = f.host.presented.as_ref().unwrap();
    let current = frame.position(point).unwrap().scene();
    assert!((current.x - anchor.x).abs() < 1e-5 && (current.y - anchor.y).abs() < 1e-5);
    let camera = frame.view().camera();
    let x = 400.0 + (2.0 - camera.center.x) * SIZE.y / camera.height;
    let y = 200.0 + camera.center.y * SIZE.y / camera.height;
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        let mut wire = input(kind);
        wire.surface_x = Some(x);
        wire.surface_y = Some(y);
        assert!(f.send(wire).unwrap());
    }
    assert_eq!(f.session.selected_pointer_target(), Some(circle.node_id()));
}

#[test]
fn direct_inspection_before_presentation_does_not_mutate_or_bind_input() {
    let mut f = Fixture::new();
    let before = f.session.frame().clone();
    for _ in 0..100 {
        assert_eq!(scroll(&mut f, CENTER, -100.0).unwrap(), None);
    }
    assert_eq!(f.session.frame(), &before);
    assert_eq!(f.session.inspection_view_revision(), 0);
    assert!(f.binding.is_none());
    assert_eq!(f.sequence, 0);
    present(&mut f);
    assert_eq!(scroll(&mut f, CENTER, -100.0).unwrap(), Some(true));
}

#[test]
fn direct_inspection_burst_does_not_relabel_or_queue_unpresented_samples() {
    let mut f = Fixture::new();
    present(&mut f);
    scroll(&mut f, CENTER, -100.0).unwrap();
    let revision = f.session.inspection_view_revision();
    let camera = f.session.inspection_camera().unwrap();
    for _ in 0..100 {
        assert_eq!(scroll(&mut f, CENTER, -100.0).unwrap(), None);
    }
    assert_eq!(f.session.inspection_view_revision(), revision);
    assert_eq!(f.session.inspection_camera().unwrap(), camera);
    present(&mut f);
    assert!(!f.host.needs_refresh(&f.session));
    assert!(f.session.wake_state().is_quiescent());
}

#[test]
fn direct_inspection_aba_and_old_surface_geometry_do_not_authorize_zoom() {
    let mut f = Fixture::new();
    present(&mut f);
    let old = f.host.presented.clone();
    scroll(&mut f, CENTER, -500.0 * 2_f64.ln()).unwrap();
    present(&mut f);
    scroll(&mut f, CENTER, 500.0 * 2_f64.ln()).unwrap();
    present(&mut f);
    assert_eq!(
        f.host.presented.as_ref().unwrap().view(),
        old.as_ref().unwrap().view()
    );
    f.host.presented = old;
    assert_eq!(scroll(&mut f, CENTER, -100.0).unwrap(), None);
    assert_eq!(f.session.inspection_view_revision(), 2);
    f.host.set_view(2, SIZE).unwrap();
    let frame = f
        .host
        .capture(&f.session, f.session.inspection_camera().unwrap())
        .unwrap();
    f.host.did_present(frame);
    assert_eq!(scroll(&mut f, CENTER, -100.0).unwrap(), None);
    assert_eq!(f.session.inspection_view_revision(), 2);
}

#[test]
fn direct_inspection_invalid_input_and_noop_preserve_a_held_contact() {
    let mut f = Fixture::new();
    present(&mut f);
    f.send(input(BrowserPointerKind::Press)).unwrap();
    let binding = f.binding;
    let frame = f.host.presented.clone();
    for delta in [f64::NAN, f64::INFINITY] {
        assert!(scroll(&mut f, CENTER, delta).is_err());
    }
    assert!(scroll(&mut f, Vec2::new(f32::NAN, 0.0), 1.0).is_err());
    assert!(f
        .host
        .scroll(&mut f.session, &mut f.binding, 1, Vec2::ZERO, CENTER, 1.0)
        .is_err());
    assert_eq!(scroll(&mut f, CENTER, 0.0).unwrap(), Some(false));
    assert_eq!(f.binding, binding);
    assert_eq!(f.host.presented, frame);
    assert_eq!(f.session.inspection_view_revision(), 0);
    f.send(input(BrowserPointerKind::Release)).unwrap();
    assert_eq!(f.session.selected_pointer_target(), Some(f.target));
}

#[test]
fn direct_inspection_retires_press_without_a_second_cancellation() {
    let mut f = Fixture::new();
    present(&mut f);
    f.send(input(BrowserPointerKind::Press)).unwrap();
    let sequence = f.sequence;
    assert_eq!(scroll(&mut f, CENTER, -100.0).unwrap(), Some(true));
    assert_eq!(f.sequence, sequence);
    present(&mut f);
    assert!(f.send(input(BrowserPointerKind::Release)).is_err());
    assert_eq!(f.session.selected_pointer_target(), None);
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        let mut wire = input(kind);
        wire.source_id = 2;
        assert!(f.send(wire).unwrap());
    }
    assert_eq!(f.session.selected_pointer_target(), Some(f.target));
}

#[test]
fn direct_inspection_callback_rejection_preserves_receipt_and_gesture() {
    let mut f = Fixture::new();
    present(&mut f);
    f.send(input(BrowserPointerKind::Press)).unwrap();
    let binding = f.binding;
    let frame = f.host.presented.clone();
    let phase = f
        .session
        .begin_required_callback_phase(0.0, [f.target])
        .unwrap();
    assert!(scroll(&mut f, CENTER, -100.0).is_err());
    assert_eq!(f.binding, binding);
    assert_eq!(f.host.presented, frame);
    assert_eq!(f.session.inspection_view_revision(), 0);
    f.session
        .commit_required_callback_phase(phase.finish())
        .unwrap();
}

#[test]
fn direct_inspection_and_real_indicate_restore_independently() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    let original = scene.live(&mut session).effective(&circle).unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_indicate(
            &circle,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let midpoint = scene.live(&mut session).effective(&circle).unwrap();
    assert!(midpoint.transform.scale.x > original.transform.scale.x);
    let mut host = DirectPointerPresentation::default();
    host.set_view(1, SIZE).unwrap();
    let frame = host
        .capture(&session, session.inspection_camera().unwrap())
        .unwrap();
    host.did_present(frame);
    assert_eq!(
        host.scroll(&mut session, &mut None, 1, SIZE, CENTER, -100.0)
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&circle)
            .unwrap()
            .transform,
        midpoint.transform
    );
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let final_state = scene.live(&mut session).effective(&circle).unwrap();
    assert_eq!(final_state.transform, original.transform);
    assert_eq!(final_state.style, original.style);
    assert!(session.inspection_camera().unwrap().height < session.camera().unwrap().height);
}
