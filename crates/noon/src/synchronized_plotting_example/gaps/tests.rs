use super::*;
use crate::{LiveProgramStatus, RustHostCallbackTable};

fn drive(program: &mut LiveProgram<GappedPlayback>, time: f64) {
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
            other => panic!("unexpected gap playback status {other:?}"),
        }
    }
    panic!("gap playback did not converge");
}

#[test]
fn marker_disappears_without_a_bridge_and_returns_with_the_same_identity() {
    let mut program = program().unwrap();
    drive(&mut program, 0.6);
    let frame = program.session().frame();
    let (index, blue) = frame.objects.iter().enumerate().find(|(_, o)| o.style.fill == Some(BLUE)).unwrap();
    assert!(frame.is_present(index));
    let identity = blue.id;
    let text = frame.objects.iter().find(|o| o.text().is_some()).unwrap().clone();
    drive(&mut program, 1.8);
    let frame = program.session().frame();
    let index = frame.objects.iter().position(|o| o.id == identity).unwrap();
    assert!(!frame.is_present(index));
    let orange = frame.objects.iter().find(|o| o.style.fill == Some(ORANGE)).unwrap();
    assert!((orange.transform.translation.x + 2.0).abs() < 2e-5);
    assert!((orange.transform.translation.y - (2.2 * 4.0 / 3.0 - 2.0)).abs() < 2e-5);
    assert_eq!(frame.objects.iter().find(|o| o.id == text.id).unwrap(), &text);
    let revision = program.session().publication_context().scene_revision();
    drive(&mut program, 1.9);
    assert_eq!(program.session().publication_context().scene_revision(), revision);
    drive(&mut program, 3.1);
    let frame = program.session().frame();
    let (index, blue) = frame.objects.iter().enumerate().find(|(_, o)| o.id == identity).unwrap();
    assert!(frame.is_present(index));
    assert!((blue.transform.translation.x - (3.1 / 6.0 * 10.0 - 5.0)).abs() < 2e-5);
    drive(&mut program, RUN_TIME);
    assert_eq!(program.status(), LiveProgramStatus::Finished);
}

#[test]
fn jump_and_forward_execution_agree_across_gap_removal_and_readmission() {
    let mut direct = program().unwrap();
    let mut forward = program().unwrap();
    drive(&mut direct, 4.5);
    for step in 1..=45 { drive(&mut forward, f64::from(step) * 0.1); }
    let a = direct.session().frame();
    let b = forward.session().frame();
    assert_eq!(a.presences, b.presences);
    assert_eq!(a.reveals, b.reveals);
    let project = |frame: &noon_runtime::FrameState| frame.objects.iter()
        .map(|o| (o.transform, o.style, o.appearance)).collect::<Vec<_>>();
    assert_eq!(project(a), project(b));
}
