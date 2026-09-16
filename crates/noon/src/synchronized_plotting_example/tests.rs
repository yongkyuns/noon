use super::*;
use crate::{GeometryRef, LiveProgramStatus, RustHostCallbackTable};

fn drive(program: &mut LiveProgram<SynchronizedPlayback>, time: f64) {
    let mut callbacks = RustHostCallbackTable::new();
    for _ in 0..80 {
        match program.status() {
            LiveProgramStatus::ReadyToResume => { program.resume().unwrap(); }
            LiveProgramStatus::PublicationPending(_) => {
                let context = program.take_renderer_publication().context();
                program.admit_publication(context).unwrap();
            }
            LiveProgramStatus::Awaiting(_) => {
                if (program.session().frame().time - time).abs() < 1e-9 { return; }
                program.drive_to(&mut callbacks, time).unwrap();
            }
            LiveProgramStatus::Finished => return,
            other => panic!("unexpected status {other:?}"),
        }
    }
    panic!("synchronized playback did not converge");
}

#[test]
fn both_markers_and_cursor_share_time_without_touching_unrelated_text() {
    let mut program = program().unwrap();
    drive(&mut program, 0.0);
    let sentinel = program.session().frame().objects.iter().find(|o| o.text().is_some()).unwrap().clone();
    drive(&mut program, 1.8); // data t=3, between different source knots.
    let frame = program.session().frame();
    for (tint, value) in [(BLUE, 0.8 + 0.4 / 3.0), (ORANGE, 2.2)] {
        let marker = frame.objects.iter().find(|o| {
            o.style.fill == Some(tint) && matches!(o.geometry(), Some(GeometryRef::Circle { .. }))
        }).unwrap();
        assert!((f64::from(marker.transform.translation.x) + 2.0).abs() < 2e-5);
        assert!((f64::from(marker.transform.translation.y) - (value * 4.0 / 3.0 - 2.0)).abs() < 2e-5);
    }
    let cursor = frame.objects.iter().find(|o| o.style.stroke == Some(GREEN)).unwrap();
    assert!((cursor.transform.translation.x - 3.0).abs() < 2e-5);
    assert_eq!(frame.objects.iter().find(|o| o.id == sentinel.id).unwrap(), &sentinel);
    let revision = program.session().publication_context().scene_revision();
    drive(&mut program, 1.9);
    assert_eq!(program.session().publication_context().scene_revision(), revision);
    drive(&mut program, RUN_TIME);
    assert_eq!(program.status(), LiveProgramStatus::Finished);
    assert_eq!(program.session().frame().objects.len(), 43);
}

#[test]
fn forward_and_jump_frames_agree_across_shared_boundaries() {
    let mut direct = program().unwrap();
    let mut forward = program().unwrap();
    drive(&mut direct, 4.5);
    for i in 0..=45 { drive(&mut forward, f64::from(i) * 0.1); }
    let project = |p: &LiveProgram<SynchronizedPlayback>| {
        p.session().frame().objects.iter().map(|o| (o.transform, o.style, o.appearance)).collect::<Vec<_>>()
    };
    assert_eq!(project(&direct), project(&forward));
}
