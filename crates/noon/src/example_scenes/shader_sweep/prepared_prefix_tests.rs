use super::*;
use crate::{LiveProgramStatus, PreparedSource, PreparedSourceStatus, RustHostCallbackTable};
use noon_core::EffectDefinition;
use noon_runtime::{FrameObjectState, FrameState, ReplayLimits};
use std::sync::Arc;

fn drive(program: &mut LiveProgram<ShaderSweep>, time: f64) {
    let mut callbacks = RustHostCallbackTable::new();
    for _ in 0..100 {
        match program.status() {
            LiveProgramStatus::ReadyToResume => {
                program.resume().unwrap();
            }
            LiveProgramStatus::PublicationPending(_) => {
                let context = program.take_renderer_publication().context();
                program.admit_publication(context).unwrap();
            }
            LiveProgramStatus::Awaiting(_) => {
                if (program.session().frame().time - time).abs() < 1e-9 {
                    return;
                }
                program.drive_to(&mut callbacks, time).unwrap();
            }
            LiveProgramStatus::Finished => return,
            status => panic!("unexpected shader source status: {status:?}"),
        }
    }
    panic!("shader source did not reach {time}");
}

fn panel(frame: &FrameState, right: bool) -> FrameObjectState {
    frame
        .objects
        .iter()
        .find(|object| {
            object.style.fill == Some(Color::WHITE)
                && object.effects.as_ref().is_some_and(|stack| {
                    stack
                        .definitions()
                        .iter()
                        .any(|definition| definition.as_shader().is_some())
                })
                && (object.transform.translation.x > 0.0) == right
        })
        .expect("shader panel")
        .clone()
}

fn shader_module(object: &FrameObjectState) -> Arc<str> {
    let stack = object.effects.as_ref().expect("shader stack");
    stack
        .definitions()
        .iter()
        .find_map(|definition: &EffectDefinition| definition.as_shader())
        .expect("attached shader")
        .program()
        .module()
        .clone()
}

fn prepare_until<C: LiveContinuation>(
    source: &mut PreparedSource<C>,
    expected: PreparedSourceStatus,
) where
    C::Error: std::fmt::Debug,
{
    for _ in 0..100 {
        let status = source.prepare_next().unwrap();
        if status == expected {
            return;
        }
        assert_eq!(status, PreparedSourceStatus::Ready);
    }
    panic!("prepared shader source did not reach {expected:?}");
}

#[test]
fn shader_sweep_prepared_history_matches_source_and_pins_shader_versions() {
    let mut reference = program().unwrap();
    let mut source_program = program().unwrap();
    // Match PlaybackProgram's renderer policy. Both shader modules are attached
    // in the bootstrap frame; the suffix changes only typed parameter values.
    source_program.require_effect_resource_preparation();
    let mut source = source_program
        .into_prepared_source(ReplayLimits::default())
        .unwrap();

    // The first wait publishes the editable scene. Keep the reader paused away
    // from the frontier while the producer prepares the entire suffix.
    prepare_until(&mut source, PreparedSourceStatus::ReaderReady);
    let mut reader = source.take_reader().expect("initial prepared reader");
    reader.seek(0.25).unwrap();
    let paused = reader.frame().clone();
    let retained_shader_bytes = reader.retained_resource_bytes();
    assert!(
        retained_shader_bytes > 0,
        "shader modules are retained by history"
    );
    let left_module = shader_module(&panel(&paused, false));
    let right_module = shader_module(&panel(&paused, true));

    loop {
        match source.prepare_next().unwrap() {
            PreparedSourceStatus::Ready => continue,
            PreparedSourceStatus::BatchReady => {
                let batch = source.pending_batch().expect("prepared suffix");
                reader.preflight_append(batch).unwrap();
                reader.append(source.take_batch().unwrap()).unwrap();
                assert_eq!(
                    reader.frame(),
                    &paused,
                    "append must preserve the paused frame"
                );
                assert_eq!(reader.retained_resource_bytes(), retained_shader_bytes);
            }
            PreparedSourceStatus::Finished => break,
            status => panic!("unexpected prepared shader status: {status:?}"),
        }
    }
    assert_eq!(reader.end_time(), Some(5.5));

    let mut reference_frames = Vec::new();
    for time in [0.5, 1.5, 2.5, 5.5] {
        drive(&mut reference, time);
        reference_frames.push((
            time,
            panel(reference.session().frame(), false),
            panel(reference.session().frame(), true),
        ));
    }

    for time in [0.5, 1.5, 2.5, 5.5, 1.5, 0.5] {
        reader.seek(time).unwrap();
        let (_, expected_left, expected_right) = reference_frames
            .iter()
            .find(|(sample_time, _, _)| *sample_time == time)
            .unwrap();
        assert_eq!(
            panel(reader.frame(), false),
            *expected_left,
            "left shader panel at {time}"
        );
        assert_eq!(
            panel(reader.frame(), true),
            *expected_right,
            "right shader panel at {time}"
        );

        let left = panel(reader.frame(), false);
        let right = panel(reader.frame(), true);
        assert!(Arc::ptr_eq(&left_module, &shader_module(&left)));
        assert!(Arc::ptr_eq(&right_module, &shader_module(&right)));
    }
}

#[test]
fn cancelling_late_resource_preparation_keeps_the_delivered_reader_usable() {
    let mut source = late_program()
        .unwrap()
        .into_prepared_source(ReplayLimits::default())
        .unwrap();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.25).unwrap();
    let baseline = reader.frame().clone();

    loop {
        match source.prepare_next().unwrap() {
            PreparedSourceStatus::Ready => {}
            PreparedSourceStatus::BatchReady => {
                reader.append(source.take_batch().unwrap()).unwrap();
                assert_eq!(reader.frame(), &baseline);
            }
            PreparedSourceStatus::PreparingResources => break,
            status => panic!("unexpected late shader status: {status:?}"),
        }
    }
    assert!(source.pending_effect_programs().is_some());
    source.cancel();
    assert_eq!(source.status(), PreparedSourceStatus::Cancelled);
    assert!(source.pending_effect_programs().is_none());
    assert!(source.publish_prepared_effects().is_err());
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::Cancelled
    );
    assert_eq!(reader.frame(), &baseline);
}
