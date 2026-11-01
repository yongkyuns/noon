use super::*;
use crate::{LiveProgramStatus, RustHostCallbackTable};

struct FollowPath {
    target: Mobject,
    path: Mobject,
    stage: usize,
    calls: Rc<Cell<usize>>,
}
impl LiveContinuation for FollowPath {
    const EFFECTS: crate::ContinuationEffects = crate::ContinuationEffects::SceneOnly;
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.stage;
        self.stage += 1;
        self.calls.set(self.calls.get() + 1);
        Ok(match step {
            0 => ContinuationStep::Await(live.wait_segment(1.0)?),
            1 => ContinuationStep::Await(
                live.declare_and_activate_move_along_path(
                    &self.target,
                    &self.path,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )?,
            ),
            2 => {
                assert_eq!(
                    live.effective(&self.target)?.transform.translation,
                    Vec2::new(2.0, 0.0)
                );
                ContinuationStep::Finished
            }
            _ => panic!("completed source resumed"),
        })
    }
}

#[test]
fn unsupported_suffix_is_detected_before_headless_drive_and_runs_once_after_transfer() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    let path = scene
        .path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(2.0, 0.0)),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
    scene.add(&target).unwrap();
    scene.add(&path).unwrap();
    let calls = Rc::new(Cell::new(0));
    let mut source = scene
        .into_prepared_source(
            FollowPath {
                target,
                path,
                stage: 0,
                calls: calls.clone(),
            },
            ReplayLimits::default(),
        )
        .unwrap();
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let publication = reader.publication_context();
    let identity = source
        .program
        .as_ref()
        .unwrap()
        .session()
        .runtime_identity();
    let required = PreparedSourceStatus::ExecutionRequired(
        SourceExecutionRequirement::ReplayUnavailable(ReplayError::UnsupportedDomain),
    );
    assert_eq!(source.prepare_next().unwrap(), required);
    assert_eq!(source.prepare_until(100.0).unwrap(), Some(required));
    assert_eq!(calls.get(), 2);
    assert!(source.pending_batch().is_none());
    let mut program = source.take_execution_program().unwrap();
    assert_eq!(program.session().runtime_identity(), identity);
    assert_eq!(program.session().frame().time, 1.0);
    assert_eq!(
        program.session().frame().objects[0].transform.translation,
        Vec2::ZERO
    );
    assert!(!program.session().replay_scope_active());
    let mut callbacks = RustHostCallbackTable::new();
    for (time, x) in [(1.25, 0.5), (1.5, 1.0), (2.0, 2.0)] {
        program.drive_to(&mut callbacks, time).unwrap();
        assert!((program.session().frame().objects[0].transform.translation.x - x).abs() < 1e-6);
        assert_eq!(calls.get(), 2, "no continuation reconstruction");
    }
    if matches!(program.status(), LiveProgramStatus::PublicationPending(_)) {
        let context = program.take_renderer_publication().context();
        program.admit_publication(context).unwrap();
    }
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(calls.get(), 3);
    drop(program);
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.publication_context(), publication);
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.end_time(), None);
}

struct StageAttachment {
    target: Mobject,
    calls: Rc<Cell<usize>>,
}
impl LiveContinuation for StageAttachment {
    const EFFECTS: crate::ContinuationEffects = crate::ContinuationEffects::SceneOnly;
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        Ok(match step {
            0 => ContinuationStep::Await(live.wait_segment(1.0)?),
            1 => ContinuationStep::Publish(live.prepare_effect_attachments([(
                &self.target,
                crate::effects::Glow::default().into(),
                "held",
            )])?),
            2 => ContinuationStep::Finished,
            _ => panic!("completed source resumed"),
        })
    }
}

#[test]
fn staged_publication_survives_transfer_and_keeps_its_original_admission_barrier() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let calls = Rc::new(Cell::new(0));
    let mut source = scene
        .into_prepared_source(
            StageAttachment {
                target,
                calls: calls.clone(),
            },
            ReplayLimits::default(),
        )
        .unwrap();
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let required =
        PreparedSourceStatus::ExecutionRequired(SourceExecutionRequirement::ExternalResources);
    assert_eq!(source.prepare_next().unwrap(), required);
    assert_eq!(source.prepare_next().unwrap(), required);
    assert_eq!(calls.get(), 2);
    let mut program = source.take_execution_program().unwrap();
    assert_eq!(program.status(), LiveProgramStatus::PreparingResources);
    assert!(program.pending_effect_programs().is_some());
    assert!(program.session().frame().objects[0].effects.is_none());
    assert!(program.resume().is_err());
    let LiveProgramStatus::PublicationPending(context) =
        program.publish_prepared_effects().unwrap()
    else {
        panic!("staged attachment needs the ordinary publication barrier");
    };
    assert!(program.session().frame().objects[0].effects.is_some());
    assert!(program.resume().is_err());
    assert_eq!(program.take_renderer_publication().context(), context);
    program.admit_publication(context).unwrap();
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    assert_eq!(calls.get(), 3);
    drop(program);
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.end_time(), None);
}

#[test]
fn bootstrap_profile_does_not_poison_later_local_suffixes() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    let path = scene
        .path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(2.0, 0.0)),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
    scene.add(&target).unwrap();
    scene.add(&path).unwrap();
    let calls = Rc::new(Cell::new(0));
    let mut source = scene
        .into_prepared_source(
            FollowPath {
                target,
                path,
                stage: 1,
                calls: calls.clone(),
            },
            ReplayLimits::default(),
        )
        .unwrap();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    assert!((reader.frame().objects[0].transform.translation.x - 1.0).abs() < 1e-6);
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    reader.append(source.take_batch().unwrap()).unwrap();
    assert_eq!(source.status(), PreparedSourceStatus::Finished);
    assert_eq!(reader.end_time(), Some(1.0));
    reader.seek(1.0).unwrap();
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(2.0, 0.0)
    );
    assert_eq!(calls.get(), 2);
}
