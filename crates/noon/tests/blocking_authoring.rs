//! Behavioral qualification of the synchronous source contract without a GPU.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use noon::{
    integration::{
        BlockingBoundary, BlockingContinuation, BlockingSceneHost, BlockingSceneSetup,
        ForwardSample, ForwardSampleStatus,
    },
    BlockingScene, LiveProgram, LiveProgramStatus, RustHostCallbackTable, Scene, ScriptError,
};

enum State {
    Cold(Box<BlockingSceneSetup>),
    Live(Box<LiveProgram<BlockingContinuation>>),
}

struct HeadlessHost {
    state: RefCell<Option<State>>,
    observations: RefCell<Vec<f64>>,
    cancel_next: Cell<bool>,
}

impl HeadlessHost {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(Some(State::Cold(Box::new(BlockingSceneSetup::new())))),
            observations: RefCell::new(Vec::new()),
            cancel_next: Cell::new(false),
        })
    }

    fn capability(self: &Rc<Self>) -> BlockingScene {
        let host: Rc<dyn BlockingSceneHost> = self.clone();
        BlockingScene::for_host(Rc::downgrade(&host))
    }

    fn initialize(&self) -> Result<(), ScriptError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        if matches!(*state, Some(State::Cold(_))) {
            let Some(State::Cold(setup)) = state.take() else {
                unreachable!()
            };
            *state = Some(State::Live(Box::new(setup.into_program()?)));
        }
        Ok(())
    }

    fn program<T>(&self, operation: impl FnOnce(&mut LiveProgram<BlockingContinuation>) -> T) -> T {
        let mut state = self.state.borrow_mut();
        let Some(State::Live(program)) = state.as_mut() else {
            panic!("expected running program")
        };
        operation(program)
    }

    fn finish(&self) -> Result<(), ScriptError> {
        self.initialize()?;
        self.program(LiveProgram::finish_authoring)
    }
}

impl BlockingSceneHost for HeadlessHost {
    fn with_scene(
        &self,
        operation: &mut dyn FnMut(
            &mut Scene,
            &mut RustHostCallbackTable,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        match state.as_mut().ok_or(ScriptError::Inactive)? {
            State::Cold(setup) => setup.with_scene(operation),
            State::Live(program) => program.with_authoring_scene(operation),
        }
    }

    fn wait(&self, boundary: BlockingBoundary) -> Result<(), ScriptError> {
        self.initialize()?;
        self.program(|program| program.begin_boundary(boundary))?;
        if self.cancel_next.replace(false) {
            self.program(LiveProgram::cancel_authoring)?;
            return Err(ScriptError::Cancelled);
        }
        // Each host step reacquires the sole program, proving that neither the
        // capability nor its synchronous call retains a RefCell borrow here.
        for _ in 0..64 {
            let done = self.program(|program| -> Result<bool, ScriptError> {
                match program.status() {
                    LiveProgramStatus::Awaiting(_) => {
                        let time = program.session().frame().time + 0.125;
                        program.drive_to(&mut RustHostCallbackTable::new(), time)
                            .map_err(|error| ScriptError::Host(error.to_string()))?;
                    }
                    LiveProgramStatus::PublicationPending(expected) => {
                        assert!(!program.boundary_complete());
                        assert!(program.with_authoring_scene(&mut |_, _| Ok(())).is_err());
                        assert!(program.admit_publication(expected).is_err());
                        let publication = program.take_renderer_publication().context();
                        assert_eq!(publication, expected);
                        program.admit_publication(publication)
                            .map_err(|error| ScriptError::Host(error.to_string()))?;
                    }
                    LiveProgramStatus::ReadyToResume => {
                        assert!(program.boundary_complete());
                        assert!(!program.can_resume());
                        let actual = program.session().frame().time;
                        let mut callbacks = RustHostCallbackTable::new();
                        let mut sample = ForwardSample::new(program, &mut callbacks, actual + 1.0, 1).unwrap();
                        let ForwardSampleStatus::Ready(observation) = sample.advance().unwrap() else {
                            panic!("empty relay must remain idle without consuming a source transition")
                        };
                        assert_eq!(observation.published_time, actual);
                        self.observations.borrow_mut().push(actual);
                        return Ok(true);
                    }
                    other => panic!("unexpected synchronous boundary phase: {other:?}"),
                }
                Ok(false)
            })?;
            if done {
                return Ok(());
            }
        }
        panic!("bounded scene did not complete");
    }
}

#[test]
fn ordinary_helpers_loops_updaters_and_post_play_reads_share_one_runtime() {
    let host = HeadlessHost::new();
    let scene = host.capability();
    noon::example_scenes::blocking_authoring::construct(&scene).unwrap();
    assert_eq!(&*host.observations.borrow(), &[0.25, 0.5, 0.75, 1.0]);
    let identity = host.program(|program| program.session().runtime_identity());
    let square = scene.square(0.5).unwrap();
    scene.add(&square).unwrap();
    scene.wait(0.0).unwrap();
    assert_eq!(
        host.program(|program| program.session().runtime_identity()),
        identity
    );
    host.finish().unwrap();
    assert_eq!(
        host.program(|program| program.status()),
        LiveProgramStatus::Finished
    );
    assert!(scene.square(0.1).is_err());
}

#[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
#[test]
fn first_lesson_constructs_text_and_math_with_an_ordinary_blocking_source() {
    let host = HeadlessHost::new();
    let scene = host.capability();
    noon::example_scenes::first_lesson::construct(&scene).unwrap();
    assert_eq!(&*host.observations.borrow(), &[0.5, 1.0, 1.5, 2.0]);
    assert_eq!(scene.time().unwrap(), 2.0);
    host.finish().unwrap();
    host.program(|program| {
        assert_eq!(program.status(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().objects.len(), 5);
    });
    assert!(matches!(scene.text("retired"), Err(ScriptError::Inactive)));
    assert!(matches!(
        scene.math_typst(noon::MathTypst::new("x")),
        Err(ScriptError::Inactive)
    ));
}

#[test]
fn invalid_boundaries_are_catchable_and_do_not_poison_the_next_call() {
    let host = HeadlessHost::new();
    let scene = host.capability();
    assert!(scene.wait(-1.0).is_err());
    scene.wait(0.25).unwrap();
    let before = host.program(|program| {
        (
            program.session().publication_context(),
            program.session().frame().time,
        )
    });
    assert!(scene.wait(f64::NAN).is_err());
    assert_eq!(
        host.program(|program| (
            program.session().publication_context(),
            program.session().frame().time
        )),
        before
    );
    scene.wait(0.25).unwrap();
    assert_eq!(scene.time().unwrap(), 0.5);
    host.finish().unwrap();
}

#[test]
fn cancellation_is_terminal_and_a_capability_does_not_retain_its_host() {
    let host = HeadlessHost::new();
    let scene = host.capability();
    host.cancel_next.set(true);
    assert!(matches!(scene.wait(1.0), Err(ScriptError::Cancelled)));
    assert_eq!(
        host.program(|program| program.status()),
        LiveProgramStatus::Terminal
    );
    assert!(scene.wait(0.0).is_err());
    assert!(scene.square(0.1).is_err());
    let weak = Rc::downgrade(&host);
    drop(host);
    assert!(weak.upgrade().is_none());
    assert!(matches!(scene.wait(0.0), Err(ScriptError::Inactive)));
}

#[test]
fn callback_reentrancy_rejects_source_access_without_panicking() {
    let host = HeadlessHost::new();
    let scene = host.capability();
    let line = scene
        .line(noon::Vec2::ZERO, noon::Vec2::new(1.0, 0.0))
        .unwrap();
    let callback_scene = scene.clone();
    let attempts = Rc::new(Cell::new(0));
    let observed = attempts.clone();
    scene
        .add_updater(&line, move |target, frame| {
            assert!(matches!(callback_scene.time(), Err(ScriptError::Reentrant)));
            assert!(matches!(
                callback_scene.image_rgba8(1, 1, vec![0; 4]),
                Err(ScriptError::Reentrant)
            ));
            observed.set(observed.get() + 1);
            target.rotate_about_origin(frame.dt())
        })
        .unwrap();
    scene.add(&line).unwrap();
    scene.wait(0.25).unwrap();
    assert!(attempts.get() >= 2);
    host.finish().unwrap();
}

#[test]
fn a_source_with_no_wait_can_finish_without_being_reexecuted() {
    let host = HeadlessHost::new();
    let scene = host.capability();
    let object = scene.square(1.0).unwrap();
    scene.add(&object).unwrap();
    host.finish().unwrap();
    host.program(|program| {
        assert_eq!(program.status(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().objects.len(), 1);
        assert_eq!(program.session().frame().time, 0.0);
        assert!(program.finish_authoring().is_err());
    });
}
